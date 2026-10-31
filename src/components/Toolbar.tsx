import { useEffect, useRef, useState } from "react";
import type { MouseEvent } from "react";
import { errMsg, fs } from "../lib/fs";
import { formatBytes } from "../lib/format";
import { thumbService } from "../lib/thumbService";
import { newMenu, sortMenu, viewMenu } from "../lib/menus";
import { basename, parent, samePath, segments } from "../lib/path";
import { VIEW_LABEL } from "../lib/view";
import type { EffectiveView } from "../lib/viewState";
import { useDropTarget } from "../hooks/useDropTarget";
import type { Column } from "../lib/fs";
import { selectActiveTab, useNav } from "../stores/nav";
import { usePrefs } from "../stores/prefs";
import { useUi } from "../stores/ui";
import type { MenuItem } from "../stores/ui";
import {
  ArrowLeft, ArrowRight, ArrowUp, ArrowUpDown, ChevronRight, Eye, EyeOff, Grid2x2, LayoutGrid, List, PanelLeft, PanelRight, Plus, RotateCw, Rows3, Search, Star, Table2,
} from "./icons";

const MODE_ICON = { details: Table2, list: List, tiles: LayoutGrid, content: Rows3, icons: Grid2x2 } as const;

interface Props {
  filter: string;
  onFilter(v: string): void;
  onReload(): void;
  view: EffectiveView;
  columns: Column[];
  setView(patch: Partial<EffectiveView>): void;
  setColumns(c: Column[]): void;
  applyAll(): void;
  newFolder(): void;
  newFile(): void;
}

/** Subcarpetas de una ruta (para el menú de las migas de pan). */
function listSubfolders(path: string): Promise<string[]> {
  return new Promise((resolve) => {
    const out: string[] = [];
    fs.openDir(path, (e) => {
      if (e.type === "batch") for (const x of e.entries) x.kind === "dir" && out.length < 400 && out.push(x.name);
      else resolve(out.sort((a, b) => a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" })));
    });
  });
}

function Crumb({ label, path, last, onGo, onMenu }: { label: string; path: string; last: boolean; onGo(p: string): void; onMenu(ev: MouseEvent, p: string): void }) {
  const { over, bind } = useDropTarget(path);
  return (
    <>
      <button className={last ? "crumb crumb--last" : "crumb"} data-drop={over || undefined} onClick={() => onGo(path)} {...bind}>{label}</button>
      <button className="crumb-more" aria-label={`Subcarpetas de ${label}`} title="Ver subcarpetas" onClick={(ev) => onMenu(ev, path)}><ChevronRight className="crumb-sep" /></button>
    </>
  );
}

export function Toolbar({ filter, onFilter, onReload, view, columns, setView, setColumns, applyAll, newFolder, newFile }: Props) {
  const tab = useNav(selectActiveTab);
  const { navigate, goBack, goForward, goUp, jump } = useNav();
  const showHidden = usePrefs((s) => s.showHidden);
  const favorites = usePrefs((s) => s.favorites);
  const sidebarCollapsed = usePrefs((s) => s.sidebarCollapsed);
  const thumbnails = usePrefs((s) => s.thumbnails);
  const previewPane = usePrefs((s) => s.previewPane);
  const toast = useUi((s) => s.toast);
  const update = usePrefs((s) => s.update);
  const toggleFavorite = usePrefs((s) => s.toggleFavorite);
  const openMenu = useUi((s) => s.openMenu);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const input = useRef<HTMLInputElement>(null);

  const path = tab?.path ?? "";
  const segs = path ? segments(path) : [];
  const startEdit = () => {
    if (!path) return;
    setDraft(path);
    setEditing(true);
  };
  useEffect(() => {
    window.addEventListener("nova:edit-path", startEdit);
    return () => window.removeEventListener("nova:edit-path", startEdit);
  });
  useEffect(() => {
    if (editing) input.current?.select();
  }, [editing]);

  const isFav = !!path && favorites.some((f) => samePath(f.path, path));
  const commit = () => {
    const p = draft.trim();
    setEditing(false);
    if (p) navigate(p);
  };

  const below = (ev: MouseEvent, items: MenuItem[]) => {
    const r = (ev.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu(r.left, r.bottom + 4, items);
  };

  const subfolderMenu = async (ev: MouseEvent, p: string) => {
    const btn = ev.currentTarget as HTMLElement;
    const r = btn.getBoundingClientRect();
    const names = await listSubfolders(p);
    const sep = p.includes("\\") ? "\\" : "/";
    openMenu(r.left, r.bottom + 4, names.length
      ? names.slice(0, 200).map((n): MenuItem => ({ kind: "item", label: n, icon: "folder", run: () => navigate(p.endsWith(sep) ? p + n : p + sep + n) }))
      : [{ kind: "header", label: "Sin subcarpetas" }]);
  };

  const history = (ev: MouseEvent, dir: "back" | "forward") => {
    ev.preventDefault();
    if (!tab) return;
    const list = dir === "back" ? [...tab.back].reverse() : tab.forward;
    if (!list.length) return;
    openMenu(ev.clientX, ev.clientY, list.slice(0, 15).map((p, i): MenuItem => ({ kind: "item", label: basename(p) || p, icon: "folder", run: () => jump(dir === "back" ? -(i + 1) : i + 1) })));
  };

  const ModeIcon = MODE_ICON[view.viewMode];

  return (
    <div className="toolbar">
      <div className="tb-group">
        <button className="ibtn" aria-pressed={!sidebarCollapsed} title="Mostrar u ocultar el panel lateral (Ctrl+B)" aria-label="Panel lateral" onClick={() => update({ sidebarCollapsed: !sidebarCollapsed })}><PanelLeft /></button>
        <span className="tb-sep" />
        <button className="ibtn" disabled={!tab?.back.length} title="Atrás (Alt+←) · clic derecho: historial" aria-label="Atrás" onClick={goBack} onContextMenu={(e) => history(e, "back")}><ArrowLeft /></button>
        <button className="ibtn" disabled={!tab?.forward.length} title="Adelante (Alt+→) · clic derecho: historial" aria-label="Adelante" onClick={goForward} onContextMenu={(e) => history(e, "forward")}><ArrowRight /></button>
        <button className="ibtn" disabled={!path || !parent(path)} title="Subir un nivel (Alt+↑)" aria-label="Subir un nivel" onClick={goUp}><ArrowUp /></button>
      </div>

      <div className="crumbs">
        {editing ? (
          <input
            ref={input} className="path-input" value={draft} spellCheck={false}
            onChange={(e) => setDraft(e.target.value)} onBlur={() => setEditing(false)}
            onKeyDown={(e) => (e.key === "Enter" ? commit() : e.key === "Escape" && setEditing(false))}
          />
        ) : (
          <>
            <div className="crumb-track" onClick={(e) => e.target === e.currentTarget && startEdit()} title={path ? "Clic en el espacio vacío para escribir una ruta (Ctrl+L)" : undefined}>
              {segs.map((s, i) => <Crumb key={s.path} label={s.label} path={s.path} last={i === segs.length - 1} onGo={navigate} onMenu={subfolderMenu} />)}
              {path && <span className="crumb-fill" onClick={startEdit} />}
            </div>
            <button className="ibtn" disabled={!path} title="Actualizar (F5)" aria-label="Actualizar" onClick={onReload}><RotateCw /></button>
          </>
        )}
      </div>

      <div className="tb-group">
        <button className="tbtn" disabled={!path} title="Crear una carpeta o un archivo nuevo" onClick={(e) => below(e, newMenu({ newFolder, newFile }))}><Plus /><span className="tb-label">Nuevo</span></button>
        <button className="tbtn" disabled={!path} title="Ordenar y agrupar" onClick={(e) => below(e, sortMenu(view, { setView }))}><ArrowUpDown /><span className="tb-label">Ordenar</span></button>
        <button className="tbtn" disabled={!path} title={`Vista: ${VIEW_LABEL[view.viewMode]} · Ctrl+rueda cambia el tamaño`} onClick={(e) => below(e, viewMenu(view, columns, { setView, setColumns, applyAll }, {
          thumbnails,
          toggleThumbnails: () => update({ thumbnails: !thumbnails }),
          clearThumbCache: () => void fs.clearThumbCache()
            .then((b) => (thumbService.clear(), toast(`Caché de miniaturas vaciada (${formatBytes(b)}).`, "success")))
            .catch((e) => toast(errMsg(e), "error")),
        }))}><ModeIcon /><span className="tb-label">Ver</span></button>
        <span className="tb-sep tb-hide-narrow" />
        <button className="ibtn tb-hide-narrow" aria-pressed={showHidden} title={showHidden ? "Ocultar elementos ocultos" : "Mostrar elementos ocultos"} aria-label="Elementos ocultos" onClick={() => update({ showHidden: !showHidden })}>{showHidden ? <Eye /> : <EyeOff />}</button>
        <button className="ibtn tb-hide-narrow" disabled={!path} aria-pressed={isFav} title={isFav ? "Quitar de favoritos" : "Añadir carpeta a favoritos"} aria-label="Favorito" onClick={() => toggleFavorite(path, basename(path))}><Star fill={isFav ? "currentColor" : "none"} /></button>
        <button className="ibtn tb-hide-narrow" aria-pressed={previewPane} title="Panel de vista previa (Alt+P)" aria-label="Panel de vista previa" onClick={() => update({ previewPane: !previewPane })}><PanelRight /></button>
        <span className="tb-sep" />
        <label className="filter">
          <Search />
          <input id="nova-filter" value={filter} placeholder="Filtrar en esta carpeta" spellCheck={false} onChange={(e) => onFilter(e.target.value)} onKeyDown={(e) => e.key === "Escape" && (onFilter(""), e.currentTarget.blur())} />
        </label>
      </div>
    </div>
  );
}
