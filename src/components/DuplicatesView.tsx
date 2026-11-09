import { useEffect, useMemo, useRef, useState } from "react";
import { DUP_FLAG, canSelect, selectAllButNewest, selectedBytes, warningsOf } from "../lib/duplicateSelection";
import { errMsg, fs } from "../lib/fs";
import type { DupGroup, DupPending, DupProgress } from "../lib/fs";
import { formatBytes, formatDate } from "../lib/format";
import { parent } from "../lib/path";
import { useApp } from "../stores/app";
import { etaSeconds, formatEta, nextRate } from "../stores/ops";
import { stepPercent, useDuplicates } from "../stores/duplicates";
import { useNav } from "../stores/nav";
import { useOps } from "../stores/ops";
import { paneActions } from "../stores/paneActions";
import { useUi } from "../stores/ui";
import { X } from "./icons";

const PHASE: Record<DupProgress["phase"], string> = { scanning: "Buscando archivos", grouping: "Agrupando por tamaño", hashing: "Comparando el contenido", finishing: "Guardando los resultados" };
const STEP = ["", "Paso 1 de 3 · resumiendo cada candidato", "Paso 2 de 3 · leyendo el contenido completo", "Paso 3 de 3 · verificando las copias"];
const unitDone = (step: number, n: number) => (step === 2 ? formatBytes(n) : n.toLocaleString("es"));

function Setup() {
  const d = useDuplicates();
  const drives = useApp((s) => s.drives);
  const [typed, setTyped] = useState("");
  const current = paneActions()?.path;
  const add = () => {
    d.addRoot(typed);
    setTyped("");
  };
  return (
    <section className="dup-card">
      <h3>Dónde buscar</h3>
      <div className="dup-chips">
        {d.roots.map((r) => <span className="dup-chip" key={r}>{r}<button className="ibtn" aria-label={`Quitar ${r}`} onClick={() => d.removeRoot(r)}><X /></button></span>)}
        {!d.roots.length && <span className="dup-hint">Ninguna carpeta elegida todavía.</span>}
      </div>
      <form className="ap-row" onSubmit={(e) => (e.preventDefault(), add())}>
        <input value={typed} spellCheck={false} placeholder="Escribe una ruta, por ejemplo D:\Fotos" aria-label="Ruta de la carpeta" onChange={(e) => setTyped(e.target.value)} />
        <button className="btn" type="submit" disabled={!typed.trim()}>Añadir</button>
      </form>
      <div className="dup-chips">
        {current && <button className="btn" onClick={() => d.addRoot(current)}>Carpeta actual</button>}
        {drives.map((x) => <button className="btn" key={x.path} onClick={() => d.addRoot(x.path)}>{x.label}</button>)}
      </div>
      <details className="dup-more">
        <summary>Exclusiones y límites</summary>
        <label className="ap-field"><span>Excluir (una por línea: una ruta completa o un nombre de carpeta)</span>
          <textarea rows={6} spellCheck={false} value={d.exclusions} onChange={(e) => d.setOptions({ exclusions: e.target.value })} />
        </label>
        <div className="dup-options">
          <label className="ap-field"><span>Tamaño mínimo (KB)</span><input type="number" min={0} value={d.minKb} onChange={(e) => d.setOptions({ minKb: Number(e.target.value) })} /></label>
          <label className="ap-field"><span>Lectura máxima (MB/s, 0 sin límite)</span><input type="number" min={0} value={d.maxMbps} onChange={(e) => d.setOptions({ maxMbps: Number(e.target.value) })} /></label>
        </div>
      </details>
      <button className="btn primary dup-go" disabled={!d.roots.length} onClick={d.start}>Detectar duplicados</button>
    </section>
  );
}

function Running({ p }: { p: DupProgress }) {
  const d = useDuplicates();
  const pct = stepPercent(p);
  const speed = useRef({ at: 0, done: 0, step: 0, rate: 0 });
  useEffect(() => {
    const s = speed.current;
    const now = performance.now();
    if (s.step !== p.step || p.paused) speed.current = { at: now, done: p.stepDone, step: p.step, rate: 0 };
    else if (now - s.at >= 800) speed.current = { at: now, done: p.stepDone, step: p.step, rate: nextRate(s.rate, p.stepDone - s.done, now - s.at) };
  }, [p]);
  const eta = pct !== null && !p.paused ? etaSeconds(p.stepTotal - p.stepDone, speed.current.rate) : null;
  return (
    <section className="dup-card" role="status">
      <h3>{p.paused ? "En pausa" : p.step > 0 ? STEP[p.step] : PHASE[p.phase]}</h3>
      <div className="bar" data-indeterminate={pct === null || undefined}><i style={{ width: pct === null ? undefined : `${pct}%` }} /></div>
      {pct !== null && <p className="dup-hint">{unitDone(p.step, p.stepDone)} de {unitDone(p.step, p.stepTotal)} · {Math.floor(pct)} %{eta !== null ? ` · quedan unos ${formatEta(eta)}` : ""}</p>}
      <p className="dup-current" title={p.current}>{p.current || " "}</p>
      <p className="dup-hint">{p.files.toLocaleString("es")} archivos · {p.candidates.toLocaleString("es")} candidatos · {formatBytes(p.readBytes)} leídos{p.reused ? ` · ${p.reused.toLocaleString("es")} reutilizados` : ""}{p.skippedCloud ? ` · ${p.skippedCloud.toLocaleString("es")} en la nube, sin tocar` : ""}</p>
      <div className="ap-row">
        <button className="btn" onClick={() => d.pause(!p.paused)}>{p.paused ? "Reanudar" : "Pausar"}</button>
        <button className="btn" title="Se guarda el avance: podrás continuar más tarde, también después de cerrar LFind" onClick={d.suspend}>Parar y continuar luego</button>
        <button className="btn" title="Descarta este análisis. Los resultados anteriores se conservan" onClick={d.cancel}>Cancelar</button>
      </div>
    </section>
  );
}

function Resumable({ p }: { p: DupPending }) {
  const d = useDuplicates();
  return (
    <section className="dup-card" role="status">
      <h3>Hay un análisis sin terminar</h3>
      <p className="dup-hint">{p.roots.join(", ")} · {p.files.toLocaleString("es")} archivos · {p.step > 0 ? STEP[p.step] : "buscando archivos"} · guardado el {formatDate(p.savedAt)}</p>
      <div className="ap-row">
        <button className="btn primary" onClick={d.resume}>Reanudar</button>
        <button className="btn" onClick={d.discard}>Descartar</button>
      </div>
    </section>
  );
}

function compare(a: string, b: string) {
  const { toast } = useUi.getState();
  fs.dupCompare(a, b).then((r) => r && toast(r.identical ? "Comparados byte a byte: son idénticos." : `No son idénticos: difieren desde el byte ${r.firstDifference}.`, r.identical ? "success" : "error")).catch((e) => toast(errMsg(e), "error"));
}

function GroupCard({ g, selected, toggle }: { g: DupGroup; selected: Set<string>; toggle(g: DupGroup, path: string): void }) {
  const marked = g.members.filter((m) => selected.has(m.path));
  const pair = marked.length === 2 ? marked : g.members.slice(0, 2);
  return (
    <article className="dup-group">
      <header>
        <strong>{formatBytes(g.size)} × {g.members.length}</strong>
        <span className="dup-hint">se pueden liberar {formatBytes(g.reclaimable)}</span>
        <button className="ap-link" onClick={() => compare(pair[0].path, pair[1].path)}>Comparar{marked.length === 2 ? " las marcadas" : ""}</button>
      </header>
      {g.members.map((m) => {
        const warn = warningsOf(m.flags);
        const allowed = canSelect(g, selected, m.path);
        return (
          <div className="dup-row" key={m.path} data-warn={m.flags & DUP_FLAG.hardLink ? true : undefined}>
            <input type="checkbox" checked={selected.has(m.path)} disabled={!allowed} aria-label={`Marcar ${m.path}`} title={allowed ? undefined : "Debe quedar al menos una copia"} onChange={() => toggle(g, m.path)} />
            <span className="dup-path" title={m.path}>{m.path}</span>
            {warn.length > 0 && <span className="dup-warn" title={warn.join(" · ")}>{warn[0]}</span>}
            <span className="dup-date">{formatDate(m.modified)}</span>
            <button className="ap-link" onClick={() => { const dir = parent(m.path); if (dir) useNav.getState().openTab(dir); }}>Abrir ubicación</button>
          </div>
        );
      })}
    </article>
  );
}

function Results() {
  const d = useDuplicates();
  const s = d.summary;
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const bytes = useMemo(() => selectedBytes(d.groups, selected), [d.groups, selected]);
  const version = useOps((x) => x.version);
  const pending = useRef<string[]>([]);
  useEffect(() => {
    if (!pending.current.length) return;
    const list = pending.current;
    pending.current = [];
    void d.afterTrash(list);
    setSelected(new Set());
  }, [version]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!s) return null;
  const toggle = (g: DupGroup, path: string) => setSelected((cur) => {
    const next = new Set(cur);
    if (next.has(path)) next.delete(path);
    else if (canSelect(g, cur, path)) next.add(path);
    return next;
  });
  const remove = () => {
    const list = [...selected];
    pending.current = list;
    void d.trash(list);
  };
  return (
    <section className="dup-results">
      <header className="dup-summary">
        <div>
          <strong>{s.groups.toLocaleString("es")} grupos · {formatBytes(s.reclaimable)} recuperables</strong>
          <p className="dup-hint">{s.files.toLocaleString("es")} archivos en {s.roots.join(", ")} · {formatDate(s.finishedAt)}{s.skippedCloud ? ` · ${s.skippedCloud.toLocaleString("es")} en la nube sin analizar` : ""}{s.issues ? ` · ${s.issues.toLocaleString("es")} sin poder leer` : ""}</p>
        </div>
        <div className="ap-row">
          <button className="btn" disabled={!d.groups.length} onClick={() => setSelected(selectAllButNewest(d.groups))}>Marcar todas menos la más reciente</button>
          <button className="btn" disabled={!selected.size} onClick={() => setSelected(new Set())}>Quitar marcas</button>
          <button className="btn primary" disabled={!selected.size} onClick={remove}>Enviar a la Papelera ({selected.size}{selected.size ? ` · ${formatBytes(bytes)}` : ""})</button>
        </div>
      </header>
      {!d.groups.length && <p className="dup-hint dup-none">No se han encontrado duplicados.</p>}
      {d.groups.map((g) => <GroupCard key={g.id} g={g} selected={selected} toggle={toggle} />)}
      {d.hasMore && <button className="btn dup-go" onClick={() => void d.loadMore()}>Ver más grupos</button>}
    </section>
  );
}

/** Herramienta independiente: abrirla solo lee los resultados guardados. El análisis empieza únicamente con el botón. */
export function DuplicatesView() {
  const open = useDuplicates((s) => s.open);
  const progress = useDuplicates((s) => s.progress);
  const pending = useDuplicates((s) => s.pending);
  useEffect(open, [open]);
  return (
    <div className="dup">
      <h2>Detector de duplicados</h2>
      {progress ? <Running p={progress} /> : pending ? <Resumable p={pending} /> : <Setup />}
      <Results />
    </div>
  );
}
