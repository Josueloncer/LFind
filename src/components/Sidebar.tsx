import { ChartPie, CircleArrowUp, Clock, Cloud, Download, FileText, Film, HardDrive, Home, ImageIcon, Monitor, Music, Palette, Pin, SlidersHorizontal, TriangleAlert, X } from "./icons";
import { useEffect, useRef, useState } from "react";
import type { ComponentType, MouseEvent, ReactNode } from "react";
import type { DriveUsage, Favorite, PathState, Place } from "../lib/fs";
import { placeMenu } from "../lib/contextMenu";
import type { PlaceMenuActions } from "../lib/contextMenu";
import { THEMES } from "../lib/themes";
import { useUi } from "../stores/ui";
import type { MenuItem } from "../stores/ui";
import { driveLabel } from "../lib/addressBar";
import { formatBytes } from "../lib/format";
import { useDropTarget } from "../hooks/useDropTarget";
import { useReorder } from "../hooks/useReorder";
import { VISIBLE_RECENTS, isPinned, usedPercent, visibleRecents } from "../lib/locations";
import { applyOrder, moveKey } from "../lib/reorder";
import { locationOrder, sectionOrder } from "../lib/uiLayout";
import { COMPUTER, STORAGE, basename, canonicalKey, isStorage, samePath } from "../lib/path";
import { useApp } from "../stores/app";
import { copyText } from "../stores/clipboard";
import { useCustomize } from "../stores/customize";
import { selectActiveTab, useNav } from "../stores/nav";
import { usePathHealth } from "../stores/pathHealth";
import { usePrefs } from "../stores/prefs";
import { useStorage } from "../stores/storage";
import { useUpdate } from "../stores/update";
import { driveIcon, openDriveMenu } from "./ComputerView";

const ICONS: Record<Place["kind"], ComponentType<{ className?: string }>> = {
  home: Home, desktop: Monitor, documents: FileText, downloads: Download,
  pictures: ImageIcon, music: Music, videos: Film, drive: HardDrive, cloud: Cloud,
};

const IGNORE_IN_ROW = ".side-unpin,input";

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

function Item({ label, path, Icon, active, onOpen, onRemove, removeLabel, usage, state, onMenu, dnd, editing, onEdited, busy }: {
  label: string; path: string; Icon: ComponentType<{ className?: string }>; active: boolean;
  onOpen(p: string): void; onRemove?: () => void; removeLabel?: string; usage?: DriveUsage; state?: PathState;
  onMenu?(ev: MouseEvent): void; dnd?: { attrs: object; handle: object }; editing?: boolean; onEdited?(v: string | null): void; busy?: boolean;
}) {
  // Soltar archivos sobre una ubicación los mueve/copia allí (Ctrl copia, Mayús mueve).
  const { over, bind } = useDropTarget(path);
  const pct = usedPercent(usage);
  const note = state === "missing" ? "La carpeta ya no existe en esta ruta" : state === "unavailable" ? "La unidad no responde" : null;
  const title = usage ? `${label} — ${formatBytes(usage.free)} libres de ${formatBytes(usage.total)}` : `${label} — ${note ?? path}`;
  // Tras abrir la ubicación el cursor sigue encima: la acción de quitar no aparece hasta salir y volver a pasar.
  const [quiet, setQuiet] = useState(false);
  return (
    <div
      className="side-row" data-drop={over || undefined} data-quiet={quiet || undefined} data-movable={dnd && !editing ? true : undefined}
      {...dnd?.attrs} {...(editing ? undefined : dnd?.handle)}
      onContextMenu={onMenu} onPointerLeave={() => setQuiet(false)}
      // Soltar archivos: las filas siguen siendo destino del arrastre de archivos.
      onDragOver={(ev) => bind.onDragOver?.(ev)}
      onDragLeave={(ev) => {
        if (!ev.currentTarget.contains(ev.relatedTarget as Node | null)) bind.onDragLeave?.();
      }}
      onDrop={(ev) => bind.onDrop?.(ev)}
    >
      {editing && onEdited ? (
        <div className="side-item"><Icon className="side-ico" /><LabelEditor value={label} onDone={onEdited} /></div>
      ) : (
        <button className="side-item" data-active={active} data-tall={usage ? true : undefined} data-state={state && state !== "ok" ? state : undefined} title={title} onClick={() => (setQuiet(true), onOpen(path))}>
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
          {busy && <i className="side-dot" title="Análisis en curso" />}
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
  const layout = usePrefs((s) => s.layout);
  const { update, toggleFavorite, renameFavorite, moveLocation, moveRecent, setLayout, forgetRecent, clearRecents } = usePrefs.getState();
  const customizing = useCustomize((s) => s.active);
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
  const [allRecents, setAllRecents] = useState(false);
  const analyzing = useStorage((s) => !!s.job);

  // Selector de tema: menú con los seis; el cambio es inmediato y se guarda con las preferencias.
  const pickTheme = (e: MouseEvent<HTMLButtonElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    openMenu(r.left, r.top - (THEMES.length + 2) * 32, [
      { kind: "header", label: "Tema" },
      ...THEMES.map((t): MenuItem => ({ kind: "item", label: t.label, checked: t.id === theme, run: () => update({ theme: t.id }) })),
      { kind: "sep" },
      { kind: "item", label: "Personalización…", run: () => useCustomize.getState().openAppearance() },
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
      analyze: () => useStorage.getState().analyze(path),
      ...extra,
    }));
  };

  const pinMenu = (f: Favorite, i: number, ids: string[]) => menu(f.path, f.label, {
    rename() {
      // En modo raíl no hay sitio para el campo: se abre el panel.
      if (collapsed) update({ sidebarCollapsed: false });
      setEditing(canonicalKey(f.path));
    },
    moveUp: i > 0 ? () => moveLocation(ids, ids[i], ids[i - 1]) : undefined,
    moveDown: i < ids.length - 1 ? () => moveLocation(ids, ids[i], ids[i + 2] ?? null) : undefined,
  });

  const known = [...places, ...cloud].filter((p) => !isPinned(favorites, p.path));
  const history = visibleRecents(recents, [...favorites.map((f) => f.path), ...places.map((p) => p.path), ...cloud.map((p) => p.path)]);
  const shownRecents = allRecents ? history : history.slice(0, VISIBLE_RECENTS);
  const orderedDrives = applyOrder(drives, (d) => canonicalKey(d.path), layout.drives);

  // Carpetas fijadas y ubicaciones del sistema se ven como una sola lista y se ordenan como una sola lista.
  const pinByKey = new Map(favorites.map((f) => [canonicalKey(f.path), f]));
  const placeByKey = new Map(known.map((p) => [canonicalKey(p.path), p]));
  const locationIds = locationOrder([...pinByKey.keys()], [...placeByKey.keys()], layout.places);
  const locationDnd = useReorder({ group: "locations", ids: locationIds, ignore: IGNORE_IN_ROW, onMove: (id, before) => moveLocation(locationIds, id, before) });
  const recentIds = shownRecents.map((r) => canonicalKey(r.path));
  const allRecentIds = history.map((r) => canonicalKey(r.path));
  const recentDnd = useReorder({
    group: "recents", ids: recentIds, ignore: IGNORE_IN_ROW,
    // Soltar al final de lo visible deja el elemento delante de los que siguen ocultos tras «Mostrar más».
    onMove: (id, before) => moveRecent(id, before ?? allRecentIds.find((k) => !recentIds.includes(k)) ?? null),
  });
  const driveIds = orderedDrives.map((d) => canonicalKey(d.path));
  const driveDnd = useReorder({ group: "drives", ids: driveIds, ignore: IGNORE_IN_ROW, onMove: (id, before) => setLayout({ drives: moveKey(driveIds, id, before) }) });

  const sections: Record<string, { title: string; action?: ReactNode; body: ReactNode } | null> = {
    locations: {
      title: "Ubicaciones",
      body: (
        <>
          {favorites.length === 0 && <div className="side-hint">Haz clic derecho en una carpeta y elige «Fijar en Ubicaciones».</div>}
          {locationIds.map((key, i) => {
            const dnd = { attrs: locationDnd.attrs(key), handle: locationDnd.handle(key) };
            const f = pinByKey.get(key);
            if (f) {
              return (
                <Item
                  key={key} label={f.label} path={f.path} Icon={Pin} active={samePath(cur, f.path)} onOpen={navigate}
                  onRemove={() => toggleFavorite(f.path, f.label)} removeLabel="Quitar de Ubicaciones" state={health[key]} onMenu={pinMenu(f, i, locationIds)}
                  dnd={dnd} editing={editing === key}
                  onEdited={(v) => (setEditing(null), v !== null && renameFavorite(f.path, v))}
                />
              );
            }
            const p = placeByKey.get(key)!;
            return <Item key={key} label={p.label} path={p.path} Icon={ICONS[p.kind]} active={samePath(cur, p.path)} onOpen={navigate} onMenu={menu(p.path, p.label)} dnd={dnd} />;
          })}
        </>
      ),
    },
    recents: history.length === 0 ? null : {
      title: "Recientes",
      action: <button className="side-clear" title="Limpiar el historial" onClick={clearRecents}>Limpiar</button>,
      body: (
        <>
          {shownRecents.map((r) => {
            const label = basename(r.path) || r.path;
            const key = canonicalKey(r.path);
            return (
              <Item
                key={key} label={label} path={r.path} Icon={Clock} active={samePath(cur, r.path)} onOpen={navigate} state={health[key]}
                onRemove={() => forgetRecent(r.path)} removeLabel="Quitar del historial"
                onMenu={menu(r.path, label, { forget: () => forgetRecent(r.path), clearAll: clearRecents })}
                dnd={{ attrs: recentDnd.attrs(key), handle: recentDnd.handle(key) }}
              />
            );
          })}
          {history.length > VISIBLE_RECENTS && (
            <button className="side-more" onClick={() => setAllRecents(!allRecents)}>{allRecents ? "Mostrar menos" : `Mostrar ${history.length - VISIBLE_RECENTS} más`}</button>
          )}
        </>
      ),
    },
    computer: {
      title: "Mi equipo",
      body: (
        <>
          <Item label="Todas las unidades" path={COMPUTER} Icon={Monitor} active={cur === COMPUTER} onOpen={navigate} />
          {orderedDrives.map((p) => {
            const u = usage[p.path];
            const kind = u?.kind ?? p.driveKind;
            const label = driveLabel(p.path[0] ?? "", u ?? { label: null, kind: kind ?? "unknown" });
            const key = canonicalKey(p.path);
            return <Item key={p.id} label={label} path={p.path} Icon={driveIcon(kind)} active={samePath(cur, p.path)} onOpen={navigate} usage={u} onMenu={(ev) => openDriveMenu(ev, p, label)} dnd={{ attrs: driveDnd.attrs(key), handle: driveDnd.handle(key) }} />;
          })}
        </>
      ),
    },
    storage: { title: "Almacenamiento", body: <Item label="Analizador de almacenamiento" path={STORAGE} Icon={ChartPie} active={isStorage(cur)} onOpen={navigate} busy={analyzing} /> },
  };
  const allSections = sectionOrder(layout);
  const visibleSections = allSections.filter((id) => sections[id]);
  const sectionDnd = useReorder({ group: "sections", ids: visibleSections, ignore: "button", onMove: (id, before) => setLayout({ sections: moveKey(allSections, id, before) }) });

  return (
    <nav className="sidebar" aria-label="Ubicaciones" data-customizing={customizing || undefined}>
      <div className="side-scroll">
        {visibleSections.map((id) => {
          const sec = sections[id]!;
          return (
            <section key={id} className="side-section" aria-label={sec.title} {...sectionDnd.attrs(id)}>
              <div className="side-title" data-action={sec.action ? true : undefined} data-movable title="Arrastra para mover la sección" {...sectionDnd.handle(id)}>
                {sec.title}{sec.action}
              </div>
              {sec.body}
            </section>
          );
        })}
      </div>
      <div className="side-foot">
        <button className="side-theme" aria-pressed={customizing} title="Personalizar interfaz: reorganizar y ocultar elementos" aria-label="Personalizar interfaz" onClick={() => (customizing ? useCustomize.getState().exit() : useCustomize.getState().enter())}>
          <SlidersHorizontal />
          <span className="side-label">Personalizar interfaz</span>
        </button>
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
