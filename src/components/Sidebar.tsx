import { CircleArrowUp, Clock, Cloud, Download, FileText, Film, HardDrive, Home, ImageIcon, Monitor, Music, Palette, Pin, TriangleAlert, X } from "./icons";
import { useEffect, useRef, useState } from "react";
import type { ComponentType, DragEvent, MouseEvent } from "react";
import type { DriveUsage, Favorite, PathState, Place } from "../lib/fs";
import { placeMenu } from "../lib/contextMenu";
import type { PlaceMenuActions } from "../lib/contextMenu";
import { THEMES } from "../lib/themes";
import { useUi } from "../stores/ui";
import type { MenuItem } from "../stores/ui";
import { driveLabel } from "../lib/addressBar";
import { formatBytes } from "../lib/format";
import { useDropTarget } from "../hooks/useDropTarget";
import { VISIBLE_RECENTS, canMovePin, isPinned, usedPercent, visibleRecents } from "../lib/locations";
import { COMPUTER, basename, canonicalKey, samePath } from "../lib/path";
import { useApp } from "../stores/app";
import { copyText } from "../stores/clipboard";
import { selectActiveTab, useNav } from "../stores/nav";
import { usePathHealth } from "../stores/pathHealth";
import { usePrefs } from "../stores/prefs";
import { useUpdate } from "../stores/update";
import { driveIcon, openDriveMenu } from "./ComputerView";

const ICONS: Record<Place["kind"], ComponentType<{ className?: string }>> = {
  home: Home, desktop: Monitor, documents: FileText, downloads: Download,
  pictures: ImageIcon, music: Music, videos: Film, drive: HardDrive, cloud: Cloud,
};

/** Tipo propio del arrastre para reordenar: no se confunde con arrastrar archivos. */
const PIN_MIME = "application/x-lfind-pin";

interface Reorder {
  /** Identifica el acceso: un índice dejaría de ser válido si la lista cambiara durante el arrastre. */
  id: string;
  index: number;
  count: number;
  /** Posición de inserción que se está señalando (índice «delante de»). */
  mark: number | null;
  dragging: boolean;
  start(index: number, id: string): void;
  /** Señala la posición; devuelve `false` si soltar ahí no cambiaría nada. */
  point(at: number): boolean;
  clearMark(): void;
  drop(id: string, at: number): void;
  end(): void;
}

/** Posición de inserción según la mitad de la fila sobre la que está el puntero. */
const insertionAt = (ev: DragEvent, index: number) => {
  const r = ev.currentTarget.getBoundingClientRect();
  return ev.clientY < r.top + r.height / 2 ? index : index + 1;
};

function LabelEditor({ value, onDone }: { value: string; onDone(v: string | null): void }) {
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  const finish = (v: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(v);
  };
  // En el siguiente fotograma: el menú que lo abrió ya se cerró y devolvió el foco.
  useEffect(() => {
    const id = requestAnimationFrame(() => (ref.current?.focus(), ref.current?.select()));
    return () => cancelAnimationFrame(id);
  }, []);
  return (
    <input
      ref={ref} className="rename" defaultValue={value} spellCheck={false} aria-label="Nombre en Ubicaciones"
      onBlur={(e) => finish(e.currentTarget.value)}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finish(e.currentTarget.value);
        else if (e.key === "Escape") finish(null);
      }}
    />
  );
}

function Item({ label, path, Icon, active, onOpen, onRemove, removeLabel, usage, state, onMenu, reorder, editing, onEdited }: {
  label: string; path: string; Icon: ComponentType<{ className?: string }>; active: boolean;
  onOpen(p: string): void; onRemove?: () => void; removeLabel?: string; usage?: DriveUsage; state?: PathState;
  onMenu?(ev: MouseEvent): void; reorder?: Reorder; editing?: boolean; onEdited?(v: string | null): void;
}) {
  // Soltar archivos sobre una ubicación los mueve/copia allí (Ctrl copia, Mayús mueve).
  const { over, bind } = useDropTarget(path);
  const pct = usedPercent(usage);
  const note = state === "missing" ? "La carpeta ya no existe en esta ruta" : state === "unavailable" ? "La unidad no responde" : null;
  const title = usage ? `${label} — ${formatBytes(usage.free)} libres de ${formatBytes(usage.total)}` : `${label} — ${note ?? path}`;
  const isPinDrag = (ev: DragEvent) => !!reorder && Array.from(ev.dataTransfer.types).includes(PIN_MIME);
  const mark = !reorder || reorder.mark === null ? undefined : reorder.mark === reorder.index ? "before" : reorder.mark === reorder.count && reorder.index === reorder.count - 1 ? "after" : undefined;
  return (
    <div
      className="side-row" data-drop={over || undefined} data-mark={mark} data-dragging={reorder?.dragging || undefined} draggable={!!reorder && !editing}
      onContextMenu={onMenu}
      onDragStart={reorder && ((ev) => {
        ev.dataTransfer.setData(PIN_MIME, reorder.id);
        ev.dataTransfer.effectAllowed = "move";
        reorder.start(reorder.index, reorder.id);
      })}
      onDragEnd={reorder && (() => reorder.end())}
      onDragOver={(ev) => {
        if (!isPinDrag(ev)) return bind.onDragOver?.(ev);
        // Sin `preventDefault`, el sistema muestra el cursor de «no permitido» y no habrá `drop`.
        if (!reorder?.point(insertionAt(ev, reorder.index))) return;
        ev.preventDefault();
        ev.dataTransfer.dropEffect = "move";
      }}
      onDragLeave={(ev) => {
        // Pasar de la fila a un elemento suyo también dispara `dragleave`: no debe parpadear el indicador.
        if (ev.currentTarget.contains(ev.relatedTarget as Node | null)) return;
        reorder?.clearMark();
        bind.onDragLeave?.();
      }}
      onDrop={(ev) => {
        if (!isPinDrag(ev)) return bind.onDrop?.(ev);
        ev.preventDefault();
        if (reorder) reorder.drop(ev.dataTransfer.getData(PIN_MIME), insertionAt(ev, reorder.index));
      }}
    >
      {editing && onEdited ? (
        <div className="side-item"><Icon className="side-ico" /><LabelEditor value={label} onDone={onEdited} /></div>
      ) : (
        <button className="side-item" data-active={active} data-tall={usage ? true : undefined} data-state={state && state !== "ok" ? state : undefined} title={title} onClick={() => onOpen(path)}>
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
          {state === "missing" && <TriangleAlert className="side-warn" aria-label={note ?? undefined} />}
        </button>
      )}
      {onRemove && !editing && (
        <button className="side-unpin" aria-label={`${removeLabel}: ${label}`} title={removeLabel} onClick={onRemove}><X /></button>
      )}
    </div>
  );
}

export function Sidebar() {
  const places = useApp((s) => s.places);
  const drives = useApp((s) => s.drives);
  const cloud = useApp((s) => s.cloud);
  const usage = useApp((s) => s.usage);
  const recents = usePrefs((s) => s.recentFolders);
  const favorites = usePrefs((s) => s.favorites);
  const theme = usePrefs((s) => s.theme);
  const collapsed = usePrefs((s) => s.sidebarCollapsed);
  const { update, toggleFavorite, renameFavorite, moveFavorite, forgetRecent, clearRecents } = usePrefs.getState();
  const health = usePathHealth((s) => s.states);
  const tab = useNav(selectActiveTab);
  const navigate = useNav((s) => s.navigate);
  const openTab = useNav((s) => s.openTab);
  const cur = tab?.path ?? "";
  const th = THEMES.find((t) => t.id === theme) ?? THEMES[0];
  const openMenu = useUi((s) => s.openMenu);
  const openDialog = useUi((s) => s.openDialog);
  const toast = useUi((s) => s.toast);
  const updateReady = useUpdate((s) => s.state.phase === "available" || s.state.phase === "ready");
  const [editing, setEditing] = useState<string | null>(null);
  const [mark, setMark] = useState<number | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const dragFrom = useRef<number | null>(null);
  const dragTimer = useRef<number | undefined>(undefined);
  const [allRecents, setAllRecents] = useState(false);

  const endDrag = () => {
    window.clearTimeout(dragTimer.current);
    dragFrom.current = null;
    setMark(null);
    setDragging(null);
  };
  const reorderBase = {
    count: favorites.length,
    mark,
    start(index: number, id: string) {
      dragFrom.current = index;
      // Después del `dragstart`: cambiar la fila arrastrada durante ese evento puede cancelar el arrastre.
      dragTimer.current = window.setTimeout(() => setDragging(id), 0);
    },
    point(at: number) {
      const ok = dragFrom.current !== null && canMovePin(favorites.length, dragFrom.current, at);
      setMark(ok ? at : null);
      return ok;
    },
    clearMark: () => setMark(null),
    drop(id: string, at: number) {
      const from = favorites.findIndex((f) => canonicalKey(f.path) === id);
      if (canMovePin(favorites.length, from, at)) moveFavorite(from, at);
      endDrag();
    },
    end: endDrag,
  };

  // Selector de tema: menú con los seis; el cambio es inmediato y se guarda con las preferencias.
  const pickTheme = (e: MouseEvent<HTMLButtonElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    openMenu(r.left, r.top - (THEMES.length + 1) * 32, [
      { kind: "header", label: "Tema" },
      ...THEMES.map((t): MenuItem => ({ kind: "item", label: t.label, checked: t.id === theme, run: () => update({ theme: t.id }) })),
    ]);
  };

  const menu = (path: string, label: string, extra: Partial<PlaceMenuActions> = {}) => (ev: MouseEvent) => {
    ev.preventDefault();
    openMenu(ev.clientX, ev.clientY, placeMenu({
      open: () => navigate(path),
      openInNewTab: () => openTab(path),
      pinned: isPinned(usePrefs.getState().favorites, path),
      togglePin: () => toggleFavorite(path, label),
      copyPath: () => void copyText(path).then((ok) => toast(ok ? "Ruta copiada" : "No se pudo copiar al portapapeles", ok ? "success" : "error")),
      properties: () => openDialog({ type: "properties", paths: [path] }),
      ...extra,
    }));
  };

  const pinMenu = (f: Favorite, i: number) => menu(f.path, f.label, {
    rename() {
      // En modo raíl no hay sitio para el campo: se abre el panel.
      if (collapsed) update({ sidebarCollapsed: false });
      setEditing(canonicalKey(f.path));
    },
    moveUp: i > 0 ? () => moveFavorite(i, i - 1) : undefined,
    moveDown: i < favorites.length - 1 ? () => moveFavorite(i, i + 2) : undefined,
  });

  const known = [...places, ...cloud].filter((p) => !isPinned(favorites, p.path));
  const history = visibleRecents(recents, [...favorites.map((f) => f.path), ...places.map((p) => p.path), ...cloud.map((p) => p.path)]);
  const shownRecents = allRecents ? history : history.slice(0, VISIBLE_RECENTS);

  return (
    <nav className="sidebar" aria-label="Ubicaciones">
      <div className="side-scroll">
        <div className="side-title">Ubicaciones</div>
        {favorites.length === 0 && <div className="side-hint">Haz clic derecho en una carpeta y elige «Fijar en Ubicaciones».</div>}
        {favorites.map((f, i) => {
          const key = canonicalKey(f.path);
          return (
            <Item
              key={key} label={f.label} path={f.path} Icon={Pin} active={samePath(cur, f.path)} onOpen={navigate}
              onRemove={() => toggleFavorite(f.path, f.label)} removeLabel="Quitar de Ubicaciones" state={health[key]} onMenu={pinMenu(f, i)}
              reorder={{ ...reorderBase, id: key, index: i, dragging: dragging === key }} editing={editing === key}
              onEdited={(v) => (setEditing(null), v !== null && renameFavorite(f.path, v))}
            />
          );
        })}
        {known.map((p) => (
          <Item key={p.id} label={p.label} path={p.path} Icon={ICONS[p.kind]} active={samePath(cur, p.path)} onOpen={navigate} onMenu={menu(p.path, p.label)} />
        ))}

        {history.length > 0 && (
          <div className="side-title" data-action>
            Recientes
            <button className="side-clear" title="Limpiar el historial" onClick={clearRecents}>Limpiar</button>
          </div>
        )}
        {shownRecents.map((r) => {
          const label = basename(r.path) || r.path;
          return (
            <Item
              key={canonicalKey(r.path)} label={label} path={r.path} Icon={Clock} active={samePath(cur, r.path)} onOpen={navigate} state={health[canonicalKey(r.path)]}
              onRemove={() => forgetRecent(r.path)} removeLabel="Quitar del historial"
              onMenu={menu(r.path, label, { forget: () => forgetRecent(r.path), clearAll: clearRecents })}
            />
          );
        })}
        {history.length > VISIBLE_RECENTS && (
          <button className="side-more" onClick={() => setAllRecents(!allRecents)}>{allRecents ? "Mostrar menos" : `Mostrar ${history.length - VISIBLE_RECENTS} más`}</button>
        )}

        <div className="side-title">Mi equipo</div>
        <Item label="Todas las unidades" path={COMPUTER} Icon={Monitor} active={cur === COMPUTER} onOpen={navigate} />
        {drives.map((p) => {
          const u = usage[p.path];
          const kind = u?.kind ?? p.driveKind;
          const label = driveLabel(p.path[0] ?? "", u ?? { label: null, kind: kind ?? "unknown" });
          return <Item key={p.id} label={label} path={p.path} Icon={driveIcon(kind)} active={samePath(cur, p.path)} onOpen={navigate} usage={u} onMenu={(ev) => openDriveMenu(ev, p, label)} />;
        })}
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
