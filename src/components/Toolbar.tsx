import { useEffect, useRef, useState } from "react";
import type { MouseEvent } from "react";
import { errMsg, fs } from "../lib/fs";
import { formatBytes } from "../lib/format";
import { iconService } from "../lib/iconService";
import { thumbService } from "../lib/thumbService";
import { newMenu, sortMenu, viewMenu } from "../lib/menus";
import { crumbsFor, outcomeFor } from "../lib/addressBar";
import type { Crumb as CrumbModel } from "../lib/addressBar";
import { basename, parent, samePath } from "../lib/path";
import { VIEW_LABEL } from "../lib/view";
import type { EffectiveView } from "../lib/viewState";
import { useDropTarget } from "../hooks/useDropTarget";
import type { Column } from "../lib/fs";
import { useApp } from "../stores/app";
import { selectActiveTab, useNav } from "../stores/nav";
import { usePrefs } from "../stores/prefs";
import { useSearch } from "../stores/search";
import { useUi } from "../stores/ui";
import type { MenuItem } from "../stores/ui";
import {
  ArrowLeft, ArrowRight, ArrowUp, ArrowUpDown, ChevronRight, Eye, EyeOff, Globe, Grid2x2, LayoutGrid, List, PanelLeft, PanelRight, Plus, RotateCw, Rows3, Search, Star, Table2,
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
  pinned: boolean;
  togglePin(): void;
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

function Crumb({ label, path, last, onGo, onMenu }: { label: string; path: string | null; last: boolean; onGo(p: string): void; onMenu(ev: MouseEvent, p: string | null): void }) {
  const { over, bind } = useDropTarget(path ?? "");
  return (
    <>
      <button
        className={last ? "crumb crumb--last" : "crumb"} data-drop={(path && over) || undefined}
        onClick={(ev) => (path ? onGo(path) : onMenu(ev, null))} {...(path ? bind : {})}
      >{label}</button>
      <button className="crumb-more" aria-label={path ? `Subcarpetas de ${label}` : "Unidades"} title={path ? "Ver subcarpetas" : "Ver unidades"} onClick={(ev) => onMenu(ev, path)}><ChevronRight className="crumb-sep" /></button>
    </>
  );
}

export function Toolbar({ filter, onFilter, onReload, view, columns, setView, setColumns, applyAll, pinned, togglePin, newFolder, newFile }: Props) {
  const tab = useNav(selectActiveTab);
  const { navigate, goBack, goForward, goUp, jump } = useNav();
  const showHidden = usePrefs((s) => s.showHidden);
  const favorites = usePrefs((s) => s.favorites);
  const sidebarCollapsed = usePrefs((s) => s.sidebarCollapsed);
  const thumbnails = usePrefs((s) => s.thumbnails);
  const systemIcons = usePrefs((s) => s.systemIcons);
  const previewPane = usePrefs((s) => s.previewPane);
  const toast = useUi((s) => s.toast);
  const update = usePrefs((s) => s.update);
  const toggleFavorite = usePrefs((s) => s.toggleFavorite);
  const openMenu = useUi((s) => s.openMenu);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [invalid, setInvalid] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const check = useRef(0); // identifica la validación en curso: Escape o una nueva anulan la anterior
  const drives = useApp((s) => s.drives);
  const usage = useApp((s) => s.usage);
  const scope = useSearch((s) => s.scope);
  const searchText = useSearch((s) => s.text);
  const setScope = useSearch((s) => s.setScope);
  const openGlobal = useSearch((s) => s.openGlobal);
  const focusTick = useSearch((s) => s.focusTick);
  const setSearchText = useSearch((s) => s.setText);
  const global = scope === "global";

  const path = tab?.path ?? "";
  const crumbs: CrumbModel[] = path ? crumbsFor(path, usage) : [];
  const startEdit = () => {
    if (!path) return;
    setDraft(path);
    setInvalid(false);
    setEditing(true);
  };
  const stopEdit = () => {
    check.current++; // descarta cualquier validación pendiente
    setBusy(false);
    setInvalid(false);
    setEditing(false);
  };
  useEffect(() => {
    window.addEventListener("nova:edit-path", startEdit);
    return () => window.removeEventListener("nova:edit-path", startEdit);
  });
  useEffect(() => {
    if (editing) input.current?.select();
  }, [editing]);

  const isFav = !!path && favorites.some((f) => samePath(f.path, path));
  /**
   * Enter: la ruta se valida en Rust (existe, es accesible; con tiempo límite para rutas de red) SIN bloquear la interfaz.
   * Si no es válida se conserva la ubicación anterior y la barra sigue abierta con el mensaje. Si es válida se navega con
   * el mismo `navigate` de siempre (historial y migas se actualizan solos) o, si es la carpeta actual, solo se recarga.
   */
  const commit = () => {
    const text = draft.trim();
    if (!text) return stopEdit();
    const id = ++check.current;
    setBusy(true);
    setInvalid(false);
    fs.resolvePath(text)
      .then((r) => {
        if (id !== check.current) return;
        const o = outcomeFor(path, r);
        stopEdit();
        if (o.action === "reload") onReload();
        else (navigate(o.path), o.note && toast(o.note, "info"));
      })
      .catch((e) => {
        if (id !== check.current) return;
        setBusy(false);
        setInvalid(true);
        toast(errMsg(e), "error");
        input.current?.focus();
      });
  };
  const below = (ev: MouseEvent, items: MenuItem[]) => {
    const r = (ev.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu(r.left, r.bottom + 4, items);
  };

  const subfolderMenu = async (ev: MouseEvent, p: string | null) => {
    const btn = ev.currentTarget as HTMLElement;
    const r = btn.getBoundingClientRect();
    if (p === null) {
      // «Este equipo»: no hay una vista propia; se ofrece la lista de unidades (datos ya en memoria, sin consultar nada).
      openMenu(r.left, r.bottom + 4, drives.length
        ? drives.map((d): MenuItem => ({ kind: "item", label: crumbsFor(d.path, usage)[1]?.label ?? d.label, icon: "folder", run: () => navigate(d.path) }))
        : [{ kind: "header", label: "Sin unidades" }]);
      return;
    }
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


  // Cualquier petición de abrir la búsqueda global (botón o Ctrl+Mayús+F) lleva el cursor al campo y selecciona su texto.
  useEffect(() => {
    if (focusTick === 0) return;
    const el = document.getElementById("nova-filter") as HTMLInputElement | null;
    el?.focus();
    el?.select();
  }, [focusTick]);
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
            ref={input} className="path-input" value={draft} spellCheck={false} autoFocus aria-label="Ruta" aria-invalid={invalid || undefined}
            data-invalid={invalid || undefined} data-busy={busy || undefined} title={busy ? "Comprobando la ruta…" : undefined}
            onChange={(e) => (setDraft(e.target.value), setInvalid(false))} onBlur={() => !busy && stopEdit()}
            onKeyDown={(e) => {
              if (e.key === "Enter") return e.preventDefault(), busy || commit();
              if (e.key === "Escape") return e.preventDefault(), e.stopPropagation(), stopEdit();
            }}
          />
        ) : (
          <>
            <div className="crumb-track" onClick={(e) => e.target === e.currentTarget && startEdit()} title={path ? "Clic en el espacio vacío para escribir una ruta (Ctrl+L)" : undefined}>
              {crumbs.map((c, i) => <Crumb key={c.path ?? "pc"} label={c.label} path={c.path} last={i === crumbs.length - 1} onGo={navigate} onMenu={subfolderMenu} />)}
              {path && <span className="crumb-fill" onClick={startEdit} />}
            </div>
            <button className="ibtn" disabled={!path} title="Actualizar (F5)" aria-label="Actualizar" onClick={onReload}><RotateCw /></button>
          </>
        )}
      </div>

      <div className="tb-group">
        <button className="tbtn" disabled={!path} title="Crear una carpeta o un archivo nuevo" onClick={(e) => below(e, newMenu({ newFolder, newFile }))}><Plus /><span className="tb-label">Nuevo</span></button>
        <button className="tbtn" disabled={!path} title="Ordenar y agrupar" onClick={(e) => below(e, sortMenu(view, { setView }))}><ArrowUpDown /><span className="tb-label">Ordenar</span></button>
        <button className="tbtn" disabled={!path} title={`Vista: ${VIEW_LABEL[view.viewMode]} · Ctrl+rueda cambia el tamaño`} onClick={(e) => below(e, viewMenu(view, columns, { setView, setColumns, applyAll, pinned, togglePin }, {
          thumbnails,
          toggleThumbnails: () => update({ thumbnails: !thumbnails }),
          systemIcons,
          toggleSystemIcons: () => update({ systemIcons: !systemIcons }),
          clearThumbCache: () => void fs.clearThumbCache()
            .then((b) => (thumbService.clear(), iconService.clear(), toast(`Caché de miniaturas e iconos vaciada (${formatBytes(b)}).`, "success")))
            .catch((e) => toast(errMsg(e), "error")),
        }))}><ModeIcon /><span className="tb-label">Ver</span></button>
        <span className="tb-sep tb-hide-narrow" />
        <button className="ibtn tb-hide-narrow" aria-pressed={showHidden} title={showHidden ? "Ocultar elementos ocultos" : "Mostrar elementos ocultos"} aria-label="Elementos ocultos" onClick={() => update({ showHidden: !showHidden })}>{showHidden ? <Eye /> : <EyeOff />}</button>
        <button className="ibtn tb-hide-narrow" disabled={!path} aria-pressed={isFav} title={isFav ? "Quitar de favoritos" : "Añadir carpeta a favoritos"} aria-label="Favorito" onClick={() => toggleFavorite(path, basename(path))}><Star fill={isFav ? "currentColor" : "none"} /></button>
        <button className="ibtn tb-hide-narrow" aria-pressed={previewPane} title="Panel de vista previa (Alt+P)" aria-label="Panel de vista previa" onClick={() => update({ previewPane: !previewPane })}><PanelRight /></button>
        <span className="tb-sep" />
        <button
          type="button" className="tbtn tb-global" aria-pressed={global} aria-label="Buscar en todos los archivos"
          title="Buscar en todos los archivos (todas las unidades indexadas) · Ctrl+Mayús+F" onClick={openGlobal}
        ><Search /><span className="tb-label">Buscar en todos los archivos</span></button>
        <label className="filter" data-global={global || undefined}>
          <Search />
          <input
            id="nova-filter" value={global ? searchText : filter} spellCheck={false}
            placeholder={global ? "Buscar en todas las unidades" : "Filtrar en esta carpeta"}
            aria-label={global ? "Buscar en todas las unidades" : "Filtrar en esta carpeta"}
            onChange={(e) => (global ? setSearchText(e.target.value) : onFilter(e.target.value))}
            onKeyDown={(e) => {
              if (e.key !== "Escape") return;
              e.preventDefault();
              if (global) return searchText ? setSearchText("") : (setScope("folder"), e.currentTarget.blur());
              onFilter("");
              e.currentTarget.blur();
            }}
          />
          <button
            type="button" className="filter-scope" aria-pressed={global} aria-label="Buscar en todas las unidades"
            title={global ? "Buscando en todas las unidades (clic para volver a filtrar la carpeta) · Ctrl+Mayús+F" : "Buscar en todas las unidades con el índice · Ctrl+Mayús+F"}
            onClick={() => (global ? (setScope("folder"), document.getElementById("nova-filter")?.focus()) : openGlobal())}
          ><Globe /></button>
        </label>
      </div>
    </div>
  );
}
