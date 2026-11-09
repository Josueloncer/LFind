import { memo, useCallback, useEffect, useMemo, useRef } from "react";
import { chooseBackground } from "../stores/backgroundPick";
import { useCustomize } from "../stores/customize";
import type { KeyboardEvent, MouseEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { pinLabel } from "../lib/contextMenu";
import { kindLabel } from "../lib/fileKinds";
import { formatBytes, formatDate } from "../lib/format";
import { errMsg, fs } from "../lib/fs";
import { isPinned } from "../lib/locations";
import type { SearchHit } from "../lib/fs";
import { parent } from "../lib/path";
import { activeFilterCount, describeIndex, hasCriteria, hitToEntry, invalidSizes } from "../lib/searchQuery";
import type { DatePreset, SearchFilters } from "../lib/searchQuery";
import { useCloudHits } from "../hooks/useCloudHits";
import { copyText } from "../stores/clipboard";
import { useIndex } from "../stores/indexStatus";
import { useNav } from "../stores/nav";
import { useSearch } from "../stores/search";
import { usePrefs } from "../stores/prefs";
import { useUi } from "../stores/ui";
import { useViewer } from "../stores/viewer";
import type { MenuItem } from "../stores/ui";
import { EmptyState } from "./EmptyState";
import { EntryIcon } from "./EntryIcon";
import { Cloud, Database, SlidersHorizontal } from "./icons";

export const ROW_H = 48;
const nf = new Intl.NumberFormat();

const CATEGORIES: [string, string][] = [
  ["", "Todos los tipos"], ["image", "Imágenes"], ["video", "Vídeos"], ["audio", "Audio"], ["archive", "Comprimidos"], ["code", "Código"],
  ["doc", "Documentos"], ["sheet", "Hojas de cálculo"], ["slides", "Presentaciones"], ["pdf", "PDF"], ["design", "Diseño"], ["exec", "Aplicaciones"],
];
const PRESETS: [DatePreset, string][] = [["any", "Cualquier fecha"], ["today", "Hoy"], ["7d", "Últimos 7 días"], ["30d", "Último mes"], ["365d", "Último año"], ["custom", "Personalizado…"]];

/** Abre un resultado: las carpetas se abren dentro de LFind (y se sale de la búsqueda); los archivos con su programa. */
function openHit(h: SearchHit) {
  if (h.isDir) {
    useNav.getState().navigate(h.path);
    useSearch.getState().leave();
  } else if (useCustomize.getState().picking) void chooseBackground(h.path);
  else fs.openPath(h.path).catch((e) => useUi.getState().toast(errMsg(e), "error"));
}

/** «Abrir ubicación»: LFind abre la carpeta contenedora y selecciona el elemento. */
export function openLocation(h: SearchHit) {
  const dir = parent(h.path);
  if (!dir) return;
  useNav.getState().navigate(dir);
  useSearch.getState().setReveal(h.path);
  useSearch.getState().leave();
}

export function hitMenu(h: SearchHit): MenuItem[] {
  const toast = useUi.getState().toast;
  const fail = (e: unknown) => toast(errMsg(e), "error");
  return [
    { kind: "item", label: h.isDir ? "Abrir carpeta" : "Abrir", icon: "open", run: () => openHit(h), shortcut: "Intro" },
    { kind: "item", label: "Abrir ubicación", icon: "folder", run: () => openLocation(h), disabled: !parent(h.path), shortcut: "Ctrl+Intro" },
    { kind: "item", label: "Mostrar en el Explorador de Windows", icon: "explorer", run: () => void fs.revealPath(h.path).catch(fail) },
    ...(h.isDir ? [{ kind: "item", label: pinLabel(isPinned(usePrefs.getState().favorites, h.path)), icon: "pin", run: () => usePrefs.getState().toggleFavorite(h.path, h.name) } as MenuItem] : []),
    { kind: "sep" },
    { kind: "item", label: "Copiar ruta completa", icon: "path", run: () => void copyText(h.path).then((ok) => toast(ok ? "Ruta copiada" : "No se pudo copiar al portapapeles", ok ? "success" : "error")) },
    { kind: "item", label: "Propiedades", icon: "props", run: () => useUi.getState().openDialog({ type: "properties", paths: [h.path] }) },
  ];
}

interface RowProps {
  hit: SearchHit;
  top: number;
  selected: boolean;
  /** El contenido solo está en la nube. */
  cloud: boolean;
  onSelect(h: SearchHit): void;
  onContext(ev: MouseEvent, h: SearchHit): void;
}

const Row = memo(function Row({ hit, top, selected, cloud, onSelect, onContext }: RowProps) {
  const entry = useMemo(() => hitToEntry(hit), [hit]);
  const dir = parent(hit.path) ?? "";
  return (
    <div
      className="sv-row" role="option" aria-selected={selected} data-sel={selected || undefined} style={{ transform: `translateY(${top}px)`, height: ROW_H }}
      onClick={() => onSelect(hit)} onDoubleClick={() => openHit(hit)} onContextMenu={(ev) => (ev.preventDefault(), onSelect(hit), onContext(ev, hit))}
    >
      <div className="sv-name">
        <span className="sv-ico"><EntryIcon entry={entry} basePath={dir} size={24} /></span>
        <span className="sv-text">
          <span className="sv-title-line">
            <span className="sv-title" title={hit.name}>{hit.name}</span>
            {cloud && <Cloud className="link-badge" aria-label="Solo en la nube"><title>Solo en la nube</title></Cloud>}
          </span>
          <span className="sv-path" title={hit.path}>{dir || hit.path}</span>
        </span>
      </div>
      <span className="sv-cell">{kindLabel(entry)}</span>
      <span className="sv-cell sv-num">{hit.isDir ? "—" : formatBytes(hit.size)}</span>
      <span className="sv-cell">{formatDate(hit.modified)}</span>
      <button className="ibtn sv-loc" title="Abrir ubicación (Ctrl+Intro)" aria-label={`Abrir la ubicación de ${hit.name}`} tabIndex={-1} onClick={(ev) => (ev.stopPropagation(), openLocation(hit))}><FolderIcon /></button>
    </div>
  );
});

function FolderIcon() {
  // Icono «abrir ubicación»: carpeta con flecha (trazo propio, sin depender de otro icono del paquete).
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      <path d="M10 13h5m0 0-2-2m2 2-2 2" />
    </svg>
  );
}

function FiltersPanel({ basePath }: { basePath: string }) {
  const f = useSearch((s) => s.filters);
  const patch = useSearch((s) => s.patchFilters);
  const reset = useSearch((s) => s.resetFilters);
  const roots = useIndex((s) => s.status?.roots);
  const bad = invalidSizes(f);
  const drives = (roots ?? []).filter((r) => r.online).map((r) => r.path);
  const set = <K extends keyof SearchFilters>(k: K) => (v: SearchFilters[K]) => patch({ [k]: v } as Partial<SearchFilters>);
  return (
    <div className="sv-filters" role="group" aria-label="Filtros de búsqueda">
      <label className="sv-field"><span>Extensión</span><input value={f.exts} spellCheck={false} placeholder="psd, png, pdf" onChange={(e) => set("exts")(e.target.value)} /></label>
      <label className="sv-field"><span>Tipo</span>
        <select value={f.category} onChange={(e) => set("category")(e.target.value)}>{CATEGORIES.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select>
      </label>
      <label className="sv-field"><span>Tamaño mínimo</span><input value={f.minSize} spellCheck={false} placeholder="1 GB" aria-invalid={bad.min || undefined} onChange={(e) => set("minSize")(e.target.value)} /></label>
      <label className="sv-field"><span>Tamaño máximo</span><input value={f.maxSize} spellCheck={false} placeholder="500 MB" aria-invalid={bad.max || undefined} onChange={(e) => set("maxSize")(e.target.value)} /></label>
      <label className="sv-field"><span>Fecha de</span>
        <select value={f.dateField} onChange={(e) => set("dateField")(e.target.value as SearchFilters["dateField"])}>
          <option value="modified">Modificación</option><option value="created">Creación</option><option value="accessed">Último acceso</option>
        </select>
      </label>
      <label className="sv-field"><span>Periodo</span>
        <select value={f.datePreset} onChange={(e) => set("datePreset")(e.target.value as DatePreset)}>{PRESETS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select>
      </label>
      {f.datePreset === "custom" && (
        <>
          <label className="sv-field"><span>Desde</span><input type="date" value={f.dateFrom} onChange={(e) => set("dateFrom")(e.target.value)} /></label>
          <label className="sv-field"><span>Hasta</span><input type="date" value={f.dateTo} onChange={(e) => set("dateTo")(e.target.value)} /></label>
        </>
      )}
      <label className="sv-field"><span>Unidad</span>
        <select value={f.drive} onChange={(e) => set("drive")(e.target.value)}>
          <option value="">Todas</option>
          {f.drive && !drives.includes(f.drive) && <option value={f.drive}>{f.drive}</option>}
          {drives.map((d) => <option key={d} value={d}>{d}</option>)}
        </select>
      </label>
      <label className="sv-field sv-wide"><span>Dentro de la carpeta</span>
        <span className="sv-inline">
          <input value={f.folder} spellCheck={false} placeholder="C:\Users\…" onChange={(e) => set("folder")(e.target.value)} />
          <button className="btn" type="button" disabled={!basePath} title={basePath || undefined} onClick={() => set("folder")(basePath)}>Carpeta actual</button>
        </span>
      </label>
      <label className="sv-field"><span>Ordenar por</span>
        <select value={f.sort} onChange={(e) => patch({ sort: e.target.value as SearchFilters["sort"], desc: e.target.value === "size" || e.target.value === "modified" })}>
          <option value="relevance">Relevancia</option><option value="name">Nombre</option><option value="size">Tamaño</option><option value="modified">Fecha de modificación</option>
        </select>
      </label>
      <div className="sv-field sv-checks">
        <label className="check"><input type="checkbox" checked={f.exact} onChange={(e) => set("exact")(e.target.checked)} /> Nombre exacto</label>
        <button className="btn" type="button" onClick={reset}>Quitar filtros</button>
      </div>
    </div>
  );
}

/**
 * Resultados de la búsqueda global (índice persistente de todas las unidades). Reutiliza el cuadro de búsqueda de la barra
 * superior; esta vista solo pinta los filtros y los resultados. Lista virtualizada: en memoria hay como mucho unos miles de filas
 * (paginadas desde Rust) y en el DOM solo las visibles.
 */
export function SearchView({ basePath }: { basePath: string }) {
  const text = useSearch((s) => s.text);
  const filters = useSearch((s) => s.filters);
  const open = useSearch((s) => s.filtersOpen);
  const toggle = useSearch((s) => s.toggleFilters);
  const patch = useSearch((s) => s.patchFilters);
  const snap = useSearch((s) => s.snap);
  const selected = useSearch((s) => s.selected);
  const select = useSearch((s) => s.select);
  const loadMore = useSearch((s) => s.loadMore);
  const leave = useSearch((s) => s.leave);
  const status = useIndex((s) => s.status);
  const openMenu = useUi((s) => s.openMenu);
  const openDialog = useUi((s) => s.openDialog);
  const previewPane = usePrefs((s) => s.previewPane);
  const scroller = useRef<HTMLDivElement>(null);
  const hits = snap.hits;
  const nFilters = activeFilterCount(filters);
  const criteria = hasCriteria(text, filters);

  const vv = useVirtualizer({ count: hits.length, getScrollElement: () => scroller.current, estimateSize: () => ROW_H, overscan: 8 });
  const items = vv.getVirtualItems();
  const cloudHits = useCloudHits(items.flatMap((it) => hits[it.index] ?? []));
  const last = items.length ? items[items.length - 1].index : -1;

  // Al acercarse al final se pide la página siguiente.
  useEffect(() => {
    if (snap.more && last >= hits.length - 25) loadMore();
  }, [last, hits.length, snap.more, loadMore]);

  // Nueva búsqueda: la lista vuelve arriba.
  useEffect(() => vv.scrollToOffset(0), [text, filters, vv]);

  const index = useMemo(() => (selected ? hits.findIndex((h) => h.path === selected) : -1), [hits, selected]);
  // El visor cambia la selección sin pasar por la lista: al volver debe seguir a la vista.
  useEffect(() => {
    if (index >= 0) vv.scrollToIndex(index, { align: "auto" });
  }, [index, vv]);
  const move = useCallback((i: number) => {
    const j = Math.max(0, Math.min(hits.length - 1, i));
    if (hits[j]) (select(hits[j].path), vv.scrollToIndex(j, { align: "auto" }));
  }, [hits, select, vv]);

  const onKey = (e: KeyboardEvent) => {
    const page = Math.max(1, Math.floor((scroller.current?.clientHeight ?? ROW_H * 10) / ROW_H) - 1);
    switch (e.key) {
      case "ArrowDown": e.preventDefault(); return move(index < 0 ? 0 : index + 1);
      case "ArrowUp": e.preventDefault(); return move(index < 0 ? 0 : index - 1);
      case "ArrowRight": if (previewPane) (e.preventDefault(), useViewer.getState().nav?.step(1)); return;
      case "ArrowLeft": if (previewPane) (e.preventDefault(), useViewer.getState().nav?.step(-1)); return;
      case "PageDown": e.preventDefault(); return move(index + page);
      case "PageUp": e.preventDefault(); return move(index - page);
      case "Home": e.preventDefault(); return move(0);
      case "End": e.preventDefault(); return move(hits.length - 1);
      case "Enter": {
        const h = hits[index];
        if (!h) return;
        e.preventDefault();
        return e.ctrlKey || e.metaKey ? openLocation(h) : openHit(h);
      }
      case "Escape": e.preventDefault(); return leave();
    }
  };
  const onContext = useCallback((ev: MouseEvent, h: SearchHit) => openMenu(ev.clientX, ev.clientY, hitMenu(h)), [openMenu]);
  const onSelect = useCallback((h: SearchHit) => select(h.path), [select]);

  const sortHead = (key: "name" | "size" | "modified", label: string, cls = "") => (
    <button className={`sv-h ${cls}`} onClick={() => patch(filters.sort === key ? { desc: !filters.desc } : { sort: key, desc: key !== "name" })} aria-sort={filters.sort === key ? (filters.desc ? "descending" : "ascending") : undefined}>
      {label}{filters.sort === key ? (filters.desc ? " ↓" : " ↑") : ""}
    </button>
  );

  let body;
  if (!fs.available) {
    body = <EmptyState title="Búsqueda global no disponible">La búsqueda en todas las unidades usa el índice del motor de Rust; abre LFind como aplicación de escritorio (<code>npm run tauri dev</code>).</EmptyState>;
  } else if (snap.error && !hits.length) {
    body = <EmptyState title="No se pudo buscar">{snap.error}</EmptyState>;
  } else if (!criteria) {
    body = (
      <EmptyState title="Búsqueda en todo el equipo">
        Escribe un nombre en la barra superior o elige filtros (extensión, tipo, tamaño, fecha…). Atajos: <code>*.psd</code> o <code>ext:pdf</code> en el texto. {describeIndex(status, (n) => nf.format(n))}.
      </EmptyState>
    );
  } else if (!snap.loading && !hits.length) {
    const building = status && status.phase !== "idle";
    body = (
      <EmptyState title="Sin resultados">
        Nada coincide con la búsqueda{nFilters ? " y los filtros" : ""}.{building ? " El índice todavía se está construyendo: puede que aún no esté lo que buscas." : ""}
      </EmptyState>
    );
  } else {
    body = (
      <div className="sv-list" ref={scroller} tabIndex={0} role="listbox" aria-label="Resultados de la búsqueda" aria-multiselectable={false} onKeyDown={onKey}>
        <div style={{ height: vv.getTotalSize(), position: "relative" }}>
          {items.map((it) => {
            const h = hits[it.index];
            return h ? <Row key={h.path} hit={h} top={it.start} selected={h.path === selected} cloud={cloudHits.has(h.path)} onSelect={onSelect} onContext={onContext} /> : null;
          })}
        </div>
        {snap.loadingMore && <div className="sv-more">Cargando más resultados…</div>}
      </div>
    );
  }

  return (
    <div className="sv">
      <div className="sv-bar">
        <button className="tbtn" aria-pressed={open} aria-expanded={open} onClick={toggle} title="Mostrar u ocultar los filtros">
          <SlidersHorizontal /><span className="tb-label">Filtros{nFilters ? ` (${nFilters})` : ""}</span>
        </button>
        <div className="sv-seg" role="group" aria-label="Archivos o carpetas">
          {([["any", "Todo"], ["file", "Archivos"], ["dir", "Carpetas"]] as const).map(([k, l]) => (
            <button key={k} aria-pressed={filters.kind === k} onClick={() => patch({ kind: k })}>{l}</button>
          ))}
        </div>
        <span className="sv-spacer" />
        <button className="tbtn" onClick={() => openDialog({ type: "index" })} title="Estado del índice y configuración">
          <Database /><span className="tb-label">Índice</span>
        </button>
      </div>
      {open && <FiltersPanel basePath={basePath} />}
      {criteria && hits.length > 0 && (
        <div className="sv-head" role="presentation">
          {sortHead("name", "Nombre y ubicación")}
          <span className="sv-h sv-static">Tipo</span>
          {sortHead("size", "Tamaño", "sv-num")}
          {sortHead("modified", "Modificado")}
          <span />
        </div>
      )}
      {(snap.loading) && <div className="progress" />}
      <div className="sv-body">{body}</div>
    </div>
  );
}

/** Barra inferior en la búsqueda global: cantidad de resultados, tiempo de la consulta y estado del índice (abre su panel). */
export function SearchStatusInfo() {
  const snap = useSearch((s) => s.snap);
  const status = useIndex((s) => s.status);
  const openDialog = useUi((s) => s.openDialog);
  const n = snap.hits.length;
  const left = !snap.active ? "Búsqueda global" : `${nf.format(n)}${snap.more ? "+" : ""} ${n === 1 && !snap.more ? "resultado" : "resultados"}${snap.elapsedMs !== null ? ` · ${nf.format(snap.elapsedMs)} ms` : ""}`;
  return (
    <>
      <span className="status-left">
        {left}
        {(snap.loading || snap.loadingMore) && <span className="busy"><i className="dot" />Buscando…</span>}
      </span>
      <button className="status-index" onClick={() => openDialog({ type: "index" })} title="Estado del índice y configuración" data-busy={status?.phase === "scanning" || undefined}>
        {status?.phase === "scanning" && <i className="dot" />}{describeIndex(status, (v) => nf.format(v))}
      </button>
    </>
  );
}
