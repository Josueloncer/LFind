import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import type { DragEvent, MouseEvent } from "react";
import { useShallow } from "zustand/react/shallow";
import { hitToEntry } from "../lib/searchQuery";
import { useDirectory } from "../hooks/useDirectory";
import { useFileActions } from "../hooks/useFileActions";
import { dropInto } from "../hooks/useDropTarget";
import { useSortedEntries } from "../hooks/useSortedEntries";
import { beginDrag, canDropInto, draggedPaths, dropOperation, endDrag } from "../lib/dnd";
import { fs } from "../lib/fs";
import type { Column, Entry, SearchHit, SortKey } from "../lib/fs";
import { buildContextMenu } from "../lib/contextMenu";
import type { ContextPoint } from "../lib/contextMenu";
import { columnsMenu } from "../lib/menus";
import { systemHasFiles, watchSystemClipboard } from "../lib/systemClipboard";
import { useShellData } from "../stores/shellData";
import { VISIT_DWELL_MS } from "../lib/locations";
import { COMPUTER, basename, isStorage, isVirtual, join, parent, parseStorageRoute, samePath } from "../lib/path";
import { ladderAt, ladderPos, zoomStep } from "../lib/view";
import { effectiveView, isPinned, nextSort } from "../lib/viewState";
import type { EffectiveView } from "../lib/viewState";
import { useApp } from "../stores/app";
import { useClipboard } from "../stores/clipboard";
import { selectActiveTab, useNav } from "../stores/nav";
import { pathKey, useOps } from "../stores/ops";
import { usePrefs } from "../stores/prefs";
import { useSearch } from "../stores/search";
import { useStorage } from "../stores/storage";
import { useUi } from "../stores/ui";
import { useViewer } from "../stores/viewer";
import { filePosition, neighborIndex } from "../lib/viewerMath";
import { ComputerView } from "./ComputerView";
import { EmptyState } from "./EmptyState";
import { ErrorBoundary } from "./ErrorBoundary";
import { FileView } from "./FileView";
import { PreviewPane } from "./PreviewPane";
import { SearchStatusInfo, SearchView } from "./SearchView";
import { StatusBar } from "./StatusBar";
import { StorageStatusInfo, StorageView } from "./StorageView";
import { Toolbar } from "./Toolbar";

export function Pane() {
  const tab = useNav(selectActiveTab);
  const p = usePrefs(useShallow((s) => ({
    viewMode: s.viewMode, iconSize: s.iconSize, sortKey: s.sortKey, sortDir: s.sortDir, groupBy: s.groupBy, folderViews: s.folderViews,
    showHidden: s.showHidden, columns: s.columns, loaded: s.loaded, thumbnails: s.thumbnails, previewPane: s.previewPane,
  })));
  const { setView: setPrefsView, setPinned, applyViewToAll, visitFolder, update } = usePrefs.getState();
  const appReady = useApp((s) => s.ready);
  const hasStart = useApp((s) => s.places.length > 0 || s.drives.length > 0);
  const openMenu = useUi((s) => s.openMenu);
  const clipMode = useClipboard((s) => s.mode);
  const clipPaths = useClipboard((s) => s.paths);
  const opsVersion = useOps((s) => s.version);
  const hidden = useOps((s) => s.hidden);
  const global = useSearch((s) => s.scope === "global");
  const pickedHit = useSearch((s) => (s.scope === "global" && s.selected ? s.snap.hits.find((h) => h.path === s.selected) ?? null : null));
  const revealReq = useSearch((s) => s.reveal);
  const viewerExpanded = useViewer((s) => s.expanded);
  const orderRef = useRef<Entry[]>([]); // orden real en pantalla (con agrupación): lo mantiene FileView

  const isComputer = tab?.path === COMPUTER;
  const driveCount = useApp((s) => s.drives.length);
  // «Mi equipo» no es una carpeta: sin ruta no se lee nada del disco y las acciones de archivo no hacen nada.
  const storage = !!tab && isStorage(tab.path);
  const path = !tab || isVirtual(tab.path) ? null : tab.path;
  const view = useMemo(() => effectiveView(p, path ?? ""), [p.viewMode, p.iconSize, p.sortKey, p.sortDir, p.groupBy, p.folderViews, path]); // eslint-disable-line react-hooks/exhaustive-deps

  const dir = useDirectory(path);
  const [filter, setFilter] = useState("");
  const deferredFilter = useDeferredValue(filter);
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [renaming, setRenaming] = useState<string | null>(null);
  const [reveal, setReveal] = useState<string | null>(null);
  const pending = useRef<{ name: string; rename: boolean } | null>(null);

  // Cambiar de carpeta o de pestaña limpia selección, filtro y renombrado.
  useEffect(() => {
    setSelected(new Set());
    setFilter("");
    setRenaming(null);
    setReveal(null);
    pending.current = null;
  }, [tab?.id, tab?.path]);

  // Cuando una recarga confirma qué hay en disco, lo eliminado deja de ocultarse (y lo que falló ya se restauró).
  useEffect(() => {
    if (dir.status !== "ready" || !path || !useOps.getState().hidden.size) return;
    useOps.getState().settle(new Set(dir.entries.map((e) => pathKey(join(path, e.name)))));
  }, [dir.entries, dir.status, path]);

  // Al terminar cualquier operación (copiar, mover, eliminar, deshacer…) se actualiza la carpeta abierta.
  useEffect(() => {
    if (opsVersion > 0) dir.reload();
  }, [opsVersion]); // eslint-disable-line react-hooks/exhaustive-deps

  const loading = dir.status === "loading";
  const { entries: ordered, sorting } = useSortedEntries(dir.entries, view.sortKey, view.sortDir, dir.status === "ready");

  const hasHidden = useMemo(() => dir.entries.some((e) => e.hidden), [dir.entries]);
  const visible = useMemo(() => {
    let l: Entry[] = ordered;
    if (!p.showHidden && hasHidden) l = l.filter((e) => !e.hidden);
    // Eliminación optimista: lo que se está eliminando desaparece al confirmar, sin esperar al sistema de archivos.
    if (hidden.size && path) l = l.filter((e) => !hidden.has(pathKey(join(path, e.name))));
    const q = deferredFilter.trim().toLowerCase();
    if (q) l = l.filter((e) => e.name.toLowerCase().includes(q));
    return l;
  }, [ordered, p.showHidden, hasHidden, deferredFilter, hidden, path]);

  // Elementos cortados que están en esta carpeta (se muestran atenuados).
  const cutNames = useMemo(() => {
    const s = new Set<string>();
    if (clipMode === "cut" && path) {
      const prefix = join(path, "");
      for (const c of clipPaths) if (c.toLowerCase().startsWith(prefix.toLowerCase())) s.add(c.slice(prefix.length));
    }
    return s;
  }, [clipMode, clipPaths, path]);

  const hiddenHere = useMemo(() => (hidden.size && path ? dir.entries.reduce((n, e) => n + (hidden.has(pathKey(join(path, e.name))) ? 1 : 0), 0) : 0), [hidden, path, dir.entries]);

  const { selCount, selectedBytes } = useMemo(() => {
    let n = 0;
    let bytes = 0;
    if (selected.size) for (const e of visible) if (selected.has(e.name)) (n++, (bytes += e.kind === "file" ? e.size : 0));
    return { selCount: n, selectedBytes: bytes };
  }, [selected, visible]);

  const afterCreate = useCallback((name: string) => {
    pending.current = { name, rename: true };
    dir.reload();
  }, [dir.reload]); // eslint-disable-line react-hooks/exhaustive-deps
  const afterRename = useCallback((name: string) => {
    pending.current = { name, rename: false };
    dir.reload();
  }, [dir.reload]); // eslint-disable-line react-hooks/exhaustive-deps
  const startRename = useCallback((name: string) => setRenaming(name), []);

  const A = useFileActions({ path, visible, selected, reload: dir.reload, afterCreate, afterRename, startRename });

  // Un elemento recién creado/renombrado: se selecciona, se muestra y (si es nuevo) pasa a modo renombrar.
  useEffect(() => {
    const pd = pending.current;
    if (!pd || dir.status !== "ready" || !visible.some((e) => e.name === pd.name)) return;
    pending.current = null;
    setSelected(new Set([pd.name]));
    setReveal(pd.name);
    if (pd.rename) setRenaming(pd.name);
  }, [visible, dir.status]);

  // «Abrir ubicación» desde la búsqueda global: al leerse la carpeta contenedora se selecciona el elemento y se muestra.
  useEffect(() => {
    if (!revealReq || dir.status !== "ready" || !path) return;
    if (Date.now() - revealReq.at > 15_000) return useSearch.getState().setReveal(null);
    const par = parent(revealReq.path);
    const name = basename(revealReq.path);
    if (!par || !samePath(par, path) || !visible.some((e) => e.name === name)) return;
    useSearch.getState().setReveal(null);
    setSelected(new Set([name]));
    setReveal(name);
  }, [revealReq, dir.status, path, visible]);

  // Recientes: carpetas que el usuario abrió y en las que se detuvo. No cuentan la inicial, las recargas ni las de paso.
  const visited = useRef(new Map<string, string>());
  useEffect(() => {
    if (dir.status !== "ready" || !path || !tab || tab.auto || visited.current.get(tab.id) === path) return;
    const id = tab.id;
    const t = setTimeout(() => {
      visited.current.set(id, path);
      visitFolder(path);
    }, VISIT_DWELL_MS);
    return () => clearTimeout(t);
  }, [dir.status, path, tab?.id, tab?.auto]); // eslint-disable-line react-hooks/exhaustive-deps

  const refreshComputer = useCallback(() => void useApp.getState().refreshDrives().catch(() => {}), []);
  const refreshStorage = useCallback(() => useStorage.getState().refresh(), []);
  const reload = isComputer ? refreshComputer : storage ? refreshStorage : dir.reload;

  // ───────── vista ─────────
  // Modo y tamaño de icono → vista global (se conserva al navegar, al cambiar de unidad y al reiniciar);
  // orden y agrupación → carpeta. Si la carpeta está «fijada», todo se guarda para ella.
  const setView = useCallback((patch: Partial<EffectiveView>) => path && setPrefsView(path, patch), [path, setPrefsView]);
  const pinned = useMemo(() => !!path && isPinned(p, path), [p.folderViews, path]); // eslint-disable-line react-hooks/exhaustive-deps
  const togglePin = useCallback(() => path && setPinned(path, !pinned), [path, pinned, setPinned]);
  const onSort = (k: SortKey) => setView(nextSort(view, k));
  const onZoom = useCallback((d: 1 | -1) => setView(zoomStep(view, d)), [setView, view]);
  const setColumns = useCallback((c: Column[]) => update({ columns: c }), [update]);

  // ───────── menús ─────────
  // Un único camino: menú propio de LFind, construido de forma síncrona con datos que ya están en memoria (selección,
  // portapapeles, «Enviar a», plantillas de «Nuevo»). Abrirlo no hace ninguna llamada al sistema.
  useEffect(() => {
    watchSystemClipboard();
    const w = window as unknown as { requestIdleCallback?: (f: () => void, o?: { timeout: number }) => number };
    if (w.requestIdleCallback) w.requestIdleCallback(() => void useShellData.getState().load(), { timeout: 2000 });
    else window.setTimeout(() => void useShellData.getState().load(), 300);
  }, []);
  const onContext = (ev: ContextPoint, e: Entry | null) => {
    if (!path) return;
    const list = e ? (selected.has(e.name) ? A.sel : [e]) : [];
    const sd = useShellData.getState();
    const items = buildContextMenu(
      { path, list, canPaste: (useClipboard.getState().paths.length > 0 && !!useClipboard.getState().mode) || systemHasFiles(), view, sendTo: sd.sendTo, templates: sd.templates, shift: ev.shiftKey },
      {
        ...A,
        paste: (t) => void A.paste(t ? join(path, t.name) : undefined),
        newFolder: () => void A.newFolder(),
        newFile: () => void A.newFile(),
        openFolderInExplorer: () => void A.openFolderInExplorer(),
        reload: dir.reload,
        setView,
        analyze: (t) => useStorage.getState().analyze(t ? join(path, t.name) : path),
      },
    );
    openMenu(ev.clientX, ev.clientY, items);
  };
  const onHeaderContext = (ev: MouseEvent) => openMenu(ev.clientX, ev.clientY, columnsMenu(p.columns, { setColumns }));

  // ───────── arrastrar y soltar ─────────
  const destOf = (t: Entry | null) => (t ? join(path ?? "", t.name) : path ?? "");
  const onDragStart = (ev: DragEvent, e: Entry) => beginDrag(ev, (selected.has(e.name) ? A.sel : [e]).map((x) => join(path ?? "", x.name)));
  const canDrop = (t: Entry | null) => {
    const d = draggedPaths();
    return !!d && !!path && canDropInto(d, destOf(t));
  };
  const dropEffect = (ev: DragEvent, t: Entry | null) => dropOperation(draggedPaths() ?? [], destOf(t), ev);
  const onDrop = (ev: DragEvent, t: Entry | null) => void dropInto(ev, destOf(t));

  // ───────── visor: anterior/siguiente en el contexto actual (carpeta o resultados) ─────────
  const viewerCtx = useRef({ A });
  viewerCtx.current = { A };
  useEffect(() => {
    const isFile = (e: Entry) => e.kind === "file";
    const hitIsFile = (h: SearchHit) => !h.isDir;
    const folderIndex = () => {
      const sel = viewerCtx.current.A.sel;
      return sel.length === 1 ? orderRef.current.findIndex((e) => e.name === sel[0].name) : -1;
    };
    const searchState = () => {
      const s = useSearch.getState();
      return { hits: s.snap.hits, at: s.selected ? s.snap.hits.findIndex((h) => h.path === s.selected) : -1 };
    };
    useViewer.getState().bindNav({
      hasTarget: () => (useSearch.getState().scope === "global" ? searchState().at >= 0 : viewerCtx.current.A.sel.length === 1),
      position() {
        if (useSearch.getState().scope === "global") {
          const { hits, at } = searchState();
          return filePosition(hits, hitIsFile, at);
        }
        return filePosition(orderRef.current, isFile, folderIndex());
      },
      step(d) {
        if (useSearch.getState().scope === "global") {
          const { hits, at } = searchState();
          const n = neighborIndex(hits, hitIsFile, at, d);
          if (n !== null) useSearch.getState().select(hits[n].path);
          return;
        }
        const list = orderRef.current;
        const n = neighborIndex(list, isFile, folderIndex(), d);
        if (n === null) return;
        setSelected(new Set([list[n].name]));
        setReveal(list[n].name);
      },
    });
    return () => useViewer.getState().bindNav(null);
  }, []);

  // ───────── atajos de archivo ─────────
  const keys = useRef({ A, selected, view, reload });
  keys.current = { A, selected, view, reload };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) return;
      const ui = useUi.getState();
      if (ui.dialog || ui.menu) return;
      // Espacio abre la vista ampliada (salvo que lo consuma la búsqueda por escritura de la lista o un control con foco).
      if (e.key === " " && !e.defaultPrevented && !e.ctrlKey && !e.altKey && !e.metaKey && !(t instanceof HTMLButtonElement || t instanceof HTMLSelectElement)) {
        e.preventDefault();
        return useViewer.getState().expand();
      }
      // Con la búsqueda global en pantalla, la selección de la carpeta de fondo no se toca (Supr, Cortar, Renombrar…).
      if (useSearch.getState().scope === "global" && e.key !== "F5") return;
      const { A, selected } = keys.current;
      const ctrl = e.ctrlKey || e.metaKey;
      const k = e.key.toLowerCase();
      if (e.key === "F5") return e.preventDefault(), keys.current.reload();
      if (ctrl && !e.shiftKey && k === "c") return e.preventDefault(), A.copy();
      if (ctrl && !e.shiftKey && k === "x") return e.preventDefault(), A.cut();
      if (ctrl && !e.shiftKey && k === "v") return e.preventDefault(), A.paste();
      if (ctrl && !e.shiftKey && k === "z") return e.preventDefault(), A.undo();
      if (ctrl && e.shiftKey && k === "n") return e.preventDefault(), void A.newFolder();
      if (e.key === "Delete" && selected.size) return e.preventDefault(), A.remove(A.sel, e.shiftKey);
      if (e.key === "F2" && A.sel.length === 1) return e.preventDefault(), A.startRename(A.sel[0].name);
      if (e.altKey && e.key === "Enter") return e.preventDefault(), A.properties(A.sel);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // ───────── contenido ─────────
  const starting = !appReady || !p.loaded || (!tab && hasStart);
  let body = null;
  if (starting) {
    body = null;
  } else if (!tab) {
    body = fs.available ? (
      <EmptyState title="Sin ubicación inicial">No se encontró ninguna carpeta de usuario ni unidad. Escribe una ruta en la barra superior.</EmptyState>
    ) : (
      <EmptyState title="Vista previa sin acceso al disco">
        Esta interfaz se está ejecutando en un navegador. Para ver tus archivos reales, ejecútala como aplicación de escritorio con <code>npm run tauri dev</code>.
      </EmptyState>
    );
  } else if (global) {
    body = <SearchView basePath={path ?? ""} />;
  } else if (isComputer) {
    body = <ComputerView />;
  } else if (storage) {
    const r = parseStorageRoute(tab.path);
    body = <StorageView root={r.root} at={r.at} />;
  } else if (dir.status === "error") {
    body = <EmptyState title="No se puede abrir la carpeta">{dir.error}</EmptyState>;
  } else if (!loading && !sorting && visible.length === 0) {
    body = <EmptyState title={filter ? "Sin coincidencias" : "Carpeta vacía"}>{filter ? `Ningún elemento contiene “${filter}”.` : "Haz clic derecho para crear una carpeta o un archivo, o pega elementos aquí."}</EmptyState>;
    // El menú contextual también debe funcionar en una carpeta vacía.
    body = <div className="empty-wrap" onContextMenu={(ev) => (ev.preventDefault(), onContext(ev, null))}>{body}</div>;
  } else {
    body = (
      <FileView
        key={tab.path}
        entries={visible} basePath={tab.path} thumbs={p.thumbnails} mode={view.viewMode} iconSize={view.iconSize} columns={p.columns} groupBy={view.groupBy}
        sortKey={view.sortKey} sortDir={view.sortDir} onSort={onSort}
        selected={selected} onSelect={setSelected} cut={cutNames}
        renaming={renaming} onRename={(o, n) => (setRenaming(null), A.rename(o, n))} onCancelRename={() => setRenaming(null)}
        onOpen={A.open} onContext={onContext} onHeaderContext={onHeaderContext} onZoom={onZoom}
        onDragStart={onDragStart} canDrop={canDrop} dropEffect={dropEffect} onDrop={onDrop} onDragEnd={endDrag}
        reveal={reveal} onRevealed={() => setReveal(null)} orderRef={orderRef} previewOpen={p.previewPane}
      />
    );
  }

  return (
    <main className="main">
      <Toolbar
        filter={filter} onFilter={setFilter} onReload={reload}
        view={view} columns={p.columns} setView={setView} setColumns={setColumns} applyAll={() => path && applyViewToAll(path)} pinned={pinned} togglePin={togglePin}
        newFolder={() => void A.newFolder()} newFile={() => void A.newFile()}
      />
      <div className="content">
        {(loading || sorting) && <div className="progress" />}
        <div className="content-row">
          <div className="content-main"><ErrorBoundary scope="view" resetKey={`${tab?.id}|${tab?.path}|${global}|${view.viewMode}`}>{body}</ErrorBoundary></div>
          <ErrorBoundary scope="panel" resetKey={global ? pickedHit?.path : `${tab?.path}|${A.sel.length === 1 ? A.sel[0].name : ""}`}>
            {(p.previewPane || viewerExpanded) && (global
              ? <PreviewPane entry={pickedHit ? hitToEntry(pickedHit) : null} basePath={pickedHit ? parent(pickedHit.path) ?? "" : ""} count={pickedHit ? 1 : 0} />
              : tab && path && <PreviewPane entry={A.sel.length === 1 ? A.sel[0] : null} basePath={tab.path} count={selCount} />)}
          </ErrorBoundary>
        </div>
      </div>
      <StatusBar
        total={isComputer ? driveCount : dir.entries.length - hiddenHere} shown={isComputer ? driveCount : visible.length} selected={selCount} selectedBytes={selectedBytes}
        loading={loading} sorting={sorting} filtered={filter.trim() !== "" || (!p.showHidden && hasHidden)}
        view={view} onLadder={(pos) => setView(ladderAt(pos, view.iconSize))} ladder={ladderPos(view)}
        info={global ? <SearchStatusInfo /> : storage ? <StorageStatusInfo /> : undefined}
      />
    </main>
  );
}
