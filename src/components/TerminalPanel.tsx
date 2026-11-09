import { useEffect, useRef, useState } from "react";
import type { PointerEvent } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { errMsg, fs } from "../lib/fs";
import type { TerminalShell } from "../lib/fs";
import { isVirtual, samePath } from "../lib/path";
import { cdCommand, parseCwdReport, terminalTheme, trackInput } from "../lib/terminalCommands";
import { selectActiveTab, useNav } from "../stores/nav";
import { usePrefs } from "../stores/prefs";
import { useTerminal } from "../stores/terminal";
import { useUi } from "../stores/ui";
import { RefreshCw, X } from "./icons";

const MIN = 120;

const activePath = (): string | null => {
  const t = selectActiveTab(useNav.getState());
  return t && !isVirtual(t.path) ? t.path : null;
};

const theme = () => {
  const cs = getComputedStyle(document.documentElement);
  return terminalTheme((n) => cs.getPropertyValue(n).trim());
};

/** Panel de terminal. Se carga solo al abrirlo; al cerrarlo termina su proceso y libera xterm. */
export default function TerminalPanel() {
  const shell = usePrefs((s) => s.terminalShell);
  const savedHeight = usePrefs((s) => s.terminalHeight);
  const sync = usePrefs((s) => s.terminalSync);
  const themeName = usePrefs((s) => s.theme);
  const update = usePrefs((s) => s.update);
  const path = useNav((s) => { const t = selectActiveTab(s); return t && !isVirtual(t.path) ? t.path : null; });
  const close = useTerminal((s) => s.hide);
  const [height, setHeight] = useState<number | null>(null);
  const [run, setRun] = useState(0);
  const [ended, setEnded] = useState<number | null>(null);
  const host = useRef<HTMLDivElement>(null);
  const live = useRef<{ id: number | null; term: Terminal | null; input: { dirty: boolean } }>({ id: null, term: null, input: { dirty: false } });
  const lastCd = useRef<string | null>(null);

  useEffect(() => {
    if (!host.current) return;
    const term = new Terminal({ fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim() || "Consolas, monospace", fontSize: 13, cursorBlink: true, scrollback: 5000, theme: theme(), allowProposedApi: false });
    const fit = new FitAddon();
    term.loadAddon(fit);
    // Con la sincronización activa, un `cd` en la terminal lleva el explorador a esa carpeta. Desactivada, la terminal va por su cuenta.
    term.parser.registerOscHandler(9, (data) => {
      const reported = parseCwdReport(data);
      if (!reported || !usePrefs.getState().terminalSync) return false;
      const current = activePath();
      lastCd.current = reported;
      if (!current || !samePath(current, reported)) useNav.getState().navigate(reported);
      return true;
    });
    term.open(host.current);
    fit.fit();
    term.focus();
    const state = { id: null as number | null, term, input: { dirty: false } };
    live.current = state;
    lastCd.current = activePath();
    setEnded(null);
    let disposed = false;
    fs.termOpen(shell, activePath(), term.cols, term.rows, (bytes) => {
      if (disposed) return;
      term.write(new Uint8Array(bytes), () => {
        if (state.id !== null) void fs.termAck(state.id, bytes.byteLength).catch(() => {});
      });
    }, (code) => !disposed && setEnded(code)).then((id) => {
      if (disposed) return void fs.termClose(id).catch(() => {});
      state.id = id;
      fs.termResize(id, term.cols, term.rows).catch(() => {});
    }).catch((e) => useUi.getState().toast(errMsg(e), "error"));
    const send = term.onData((d) => {
      trackInput(state.input, d);
      if (state.id !== null) void fs.termWrite(state.id, d).catch(() => {});
    });
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown" || !e.ctrlKey || e.key.toLowerCase() === "c" && !term.hasSelection()) return true;
      const k = e.key.toLowerCase();
      if (k === "c") return void navigator.clipboard.writeText(term.getSelection()), term.clearSelection(), false;
      if (k === "v") return void navigator.clipboard.readText().then((t) => term.paste(t)), false;
      return true;
    });
    let timer = 0;
    const observer = new ResizeObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        try {
          fit.fit();
        } catch {
          return;
        }
        if (state.id !== null) void fs.termResize(state.id, term.cols, term.rows).catch(() => {});
      }, 60);
    });
    observer.observe(host.current);
    return () => {
      disposed = true;
      observer.disconnect();
      window.clearTimeout(timer);
      send.dispose();
      if (state.id !== null) void fs.termClose(state.id).catch(() => {});
      term.dispose();
    };
  }, [shell, run]);

  useEffect(() => {
    const t = live.current.term;
    if (t) t.options.theme = theme();
  }, [themeName]);

  /** Solo escribe un cambio de carpeta, y solo si el shell está libre y no hay una línea a medias. */
  const goTo = async (target: string, explicit: boolean) => {
    const { id, input } = live.current;
    const command = cdCommand(shell, target);
    if (id === null || !command) return explicit ? useUi.getState().toast("Esa ruta no se puede escribir en este shell.", "info") : undefined;
    if (input.dirty || (await fs.termBusy(id).catch(() => true))) return explicit ? useUi.getState().toast("La terminal está ocupada o hay una línea a medio escribir.", "info") : undefined;
    lastCd.current = target;
    await fs.termWrite(id, command).catch(() => {});
  };

  useEffect(() => {
    if (!sync || !path || path === lastCd.current || ended !== null) return;
    void goTo(path, false);
  }, [path, sync, ended]); // eslint-disable-line react-hooks/exhaustive-deps

  const startResize = (e: PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const startY = e.clientY;
    const startH = height ?? savedHeight;
    let last = startH;
    const move = (ev: globalThis.PointerEvent) => {
      last = Math.round(Math.max(MIN, Math.min(window.innerHeight * 0.75, startH + startY - ev.clientY)));
      setHeight(last);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      update({ terminalHeight: last });
      setHeight(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <section className="term" style={{ height: height ?? savedHeight }} aria-label="Terminal">
      <div className="term-resize" onPointerDown={startResize} role="separator" aria-orientation="horizontal" aria-label="Cambiar la altura de la terminal" />
      <header className="term-head">
        <div className="term-tabs" role="tablist" aria-label="Shell">
          {([["powershell", "PowerShell"], ["cmd", "CMD"]] as [TerminalShell, string][]).map(([id, label]) => <button key={id} role="tab" aria-selected={shell === id} onClick={() => update({ terminalShell: id })}>{label}</button>)}
        </div>
        {ended !== null && <span className="term-note">Proceso terminado (código {ended})</span>}
        <span className="term-actions">
          <label className="term-sync" title="El explorador y la terminal siguen la misma carpeta. La terminal solo cambia si está libre y sin una línea a medias. Desactivado, cada uno va por su cuenta."><input type="checkbox" checked={sync} onChange={(e) => update({ terminalSync: e.target.checked })} />Sincronizar con el explorador</label>
          <button className="btn" disabled={!path || ended !== null} onClick={() => path && void goTo(path, true)}>Ir a la carpeta del panel</button>
          <button className="ibtn" title="Reiniciar la terminal" aria-label="Reiniciar la terminal" onClick={() => setRun((n) => n + 1)}><RefreshCw /></button>
          <button className="ibtn" title="Cerrar la terminal" aria-label="Cerrar la terminal" onClick={close}><X /></button>
        </span>
      </header>
      <div className="term-host" ref={host} />
    </section>
  );
}
