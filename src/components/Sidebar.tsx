import { CircleArrowUp, Clock, Download, FileText, Film, HardDrive, Home, ImageIcon, Monitor, Music, Palette, Star, X } from "./icons";
import type { ComponentType, MouseEvent } from "react";
import type { DriveUsage, Place } from "../lib/fs";
import { THEMES } from "../lib/themes";
import { useUi } from "../stores/ui";
import type { MenuItem } from "../stores/ui";
import { formatBytes } from "../lib/format";
import { useDropTarget } from "../hooks/useDropTarget";
import { basename, samePath } from "../lib/path";
import { useApp } from "../stores/app";
import { selectActiveTab, useNav } from "../stores/nav";
import { usePrefs } from "../stores/prefs";
import { useUpdate } from "../stores/update";

const ICONS: Record<Place["kind"], ComponentType<{ className?: string }>> = {
  home: Home, desktop: Monitor, documents: FileText, downloads: Download,
  pictures: ImageIcon, music: Music, videos: Film, drive: HardDrive,
};


function Item({ label, path, Icon, active, onOpen, onRemove, usage }: {
  label: string; path: string; Icon: ComponentType<{ className?: string }>; active: boolean;
  onOpen(p: string): void; onRemove?: () => void; usage?: DriveUsage;
}) {
  // Soltar archivos sobre una ubicación los mueve/copia allí (Ctrl copia, Mayús mueve).
  const { over, bind } = useDropTarget(path);
  const pct = usage && usage.total > 0 ? Math.min(100, (usage.used / usage.total) * 100) : null;
  return (
    <div className="side-row" data-drop={over || undefined} {...bind}>
      <button className="side-item" data-active={active} data-tall={usage ? true : undefined} title={usage ? `${label} — ${formatBytes(usage.free)} libres de ${formatBytes(usage.total)}` : `${label} — ${path}`} onClick={() => onOpen(path)}>
        <Icon className="side-ico" />
        <span className="side-main">
          <span className="side-label">{label}</span>
          {pct !== null && usage && (
            <>
              <span className="drive-bar" data-low={pct > 90 || undefined}><i style={{ width: `${pct}%` }} /></span>
              <span className="drive-text">{formatBytes(usage.free)} libres de {formatBytes(usage.total)}</span>
            </>
          )}
        </span>
      </button>
      {onRemove && (
        <button className="side-unpin" aria-label={`Quitar ${label} de favoritos`} title="Quitar de favoritos" onClick={onRemove}><X /></button>
      )}
    </div>
  );
}

export function Sidebar() {
  const { places, drives, usage } = useApp();
  const recents = usePrefs((s) => s.recents);
  const tab = useNav(selectActiveTab);
  const navigate = useNav((s) => s.navigate);
  const { favorites, theme, update, toggleFavorite } = usePrefs();
  const cur = tab?.path ?? "";
  const th = THEMES.find((t) => t.id === theme) ?? THEMES[0];
  const openMenu = useUi((s) => s.openMenu);
  const openDialog = useUi((s) => s.openDialog);
  const updateReady = useUpdate((s) => s.state.phase === "available" || s.state.phase === "ready");
  // Selector de tema: menú con los seis; el cambio es inmediato y se guarda con las preferencias.
  const pickTheme = (e: MouseEvent<HTMLButtonElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    openMenu(r.left, r.top - (THEMES.length + 1) * 32, [
      { kind: "header", label: "Tema" },
      ...THEMES.map((t): MenuItem => ({ kind: "item", label: t.label, checked: t.id === theme, run: () => update({ theme: t.id }) })),
    ]);
  };

  return (
    <nav className="sidebar" aria-label="Ubicaciones">
      <div className="side-scroll">
        <div className="side-title">Favoritos</div>
        {favorites.length === 0 && <div className="side-hint">Pulsa ☆ en la barra superior para fijar la carpeta actual.</div>}
        {favorites.map((f) => (
          <Item key={f.path} label={f.label} path={f.path} Icon={Star} active={samePath(cur, f.path)} onOpen={navigate} onRemove={() => toggleFavorite(f.path, f.label)} />
        ))}

        {recents.length > 0 && <div className="side-title">Recientes</div>}
        {recents.filter((r) => !favorites.some((f) => samePath(f.path, r))).slice(0, 6).map((r) => (
          <Item key={r} label={basename(r) || r} path={r} Icon={Clock} active={samePath(cur, r)} onOpen={navigate} />
        ))}

        {places.length > 0 && <div className="side-title">Ubicaciones</div>}
        {places.map((p) => (
          <Item key={p.id} label={p.label} path={p.path} Icon={ICONS[p.kind]} active={samePath(cur, p.path)} onOpen={navigate} />
        ))}

        {drives.length > 0 && <div className="side-title">Unidades</div>}
        {drives.map((p) => (
          <Item key={p.id} label={usage[p.path]?.label ? `${usage[p.path].label} (${p.path.replace(/[\\/]+$/, "")})` : p.label} path={p.path} Icon={ICONS.drive} active={samePath(cur, p.path)} onOpen={navigate} usage={usage[p.path]} />
        ))}
      </div>
      <div className="side-foot">
        <button className="side-theme" title={`Tema: ${th.label} (clic para cambiar)`} onClick={pickTheme}>
          <Palette />
          <span className="side-label">Tema: {th.label}</span>
        </button>
        <button className="side-theme" title={updateReady ? "Hay una nueva versión de LFind" : "Buscar actualizaciones"} aria-label="Buscar actualizaciones" onClick={() => openDialog({ type: "update" })}>
          <CircleArrowUp />
          <span className="side-label">Buscar actualizaciones</span>
          {updateReady && <i className="side-dot" />}
        </button>
      </div>
    </nav>
  );
}
