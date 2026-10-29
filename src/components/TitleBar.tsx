import { useEffect, useState } from "react";
import { Plus, X } from "./icons";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "../lib/fs";
import { basename } from "../lib/path";
import { useDropTarget } from "../hooks/useDropTarget";
import type { Tab } from "../stores/nav";
import { useNav } from "../stores/nav";
import { FolderGlyph, NovaMark } from "./Glyphs";

function WindowControls() {
  const [max, setMax] = useState(false);
  useEffect(() => {
    const w = getCurrentWindow();
    let off: (() => void) | undefined;
    let dead = false;
    let t: ReturnType<typeof setTimeout> | undefined;
    const sync = () => void w.isMaximized().then((m) => !dead && setMax(m));
    // Al arrastrar el borde llegan decenas de eventos por segundo: se agrupan (una consulta IPC al terminar).
    const onResize = () => {
      clearTimeout(t);
      t = setTimeout(sync, 120);
    };
    sync();
    void w.onResized(onResize).then((u) => (dead ? u() : (off = u)));
    return () => {
      dead = true;
      clearTimeout(t);
      off?.();
    };
  }, []);
  const w = getCurrentWindow();
  return (
    <div className="winctl">
      <button aria-label="Minimizar" onClick={() => void w.minimize()}>
        <svg viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" strokeWidth="1" /></svg>
      </button>
      <button aria-label={max ? "Restaurar" : "Maximizar"} onClick={() => void w.toggleMaximize()}>
        {max ? (
          <svg viewBox="0 0 10 10" fill="none" stroke="currentColor"><path d="M2.5 2.5V.5h7v7h-2" /><rect x=".5" y="2.5" width="7" height="7" /></svg>
        ) : (
          <svg viewBox="0 0 10 10" fill="none" stroke="currentColor"><rect x=".5" y=".5" width="9" height="9" /></svg>
        )}
      </button>
      <button className="close" aria-label="Cerrar" onClick={() => void w.close()}>
        <svg viewBox="0 0 10 10" stroke="currentColor"><path d="M0 0l10 10M10 0L0 10" /></svg>
      </button>
    </div>
  );
}

function TabItem({ t, active, many, onSelect, onClose }: { t: Tab; active: boolean; many: boolean; onSelect(): void; onClose(): void }) {
  // Soltar archivos sobre una pestaña los lleva a la carpeta que muestra.
  const { over, bind } = useDropTarget(t.path);
  return (
    <div
      role="tab" tabIndex={0} aria-selected={active} className="tab" data-drop={over || undefined} title={t.path}
      onClick={onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onSelect()}
      onAuxClick={(e) => e.button === 1 && onClose()}
      {...bind}
    >
      <FolderGlyph className="tab-icon" />
      <span className="tab-label">{basename(t.path)}</span>
      {many && (
        <button className="tab-close" aria-label="Cerrar pestaña" onClick={(e) => (e.stopPropagation(), onClose())}><X /></button>
      )}
    </div>
  );
}

export function TitleBar() {
  const { tabs, activeId, setActive, closeTab, openTab } = useNav();
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region><NovaMark /></div>
      <div className="tabs" role="tablist" data-tauri-drag-region>
        {tabs.map((t) => (
          <TabItem key={t.id} t={t} active={t.id === activeId} many={tabs.length > 1} onSelect={() => setActive(t.id)} onClose={() => closeTab(t.id)} />
        ))}
        {tabs.length > 0 && (
          <button className="ibtn tab-new" title="Nueva pestaña (Ctrl+T)" aria-label="Nueva pestaña" onClick={() => openTab()}>
            <Plus />
          </button>
        )}
      </div>
      <div className="drag-fill" data-tauri-drag-region />
      {isTauri && <WindowControls />}
    </header>
  );
}
