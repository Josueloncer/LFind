import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Select } from "./Select";
import type { AvailStat, Availability } from "../lib/fs";
import type { ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { driveLabel } from "../lib/addressBar";
import { kindLabel } from "../lib/fileKinds";
import { formatBytes, formatDate } from "../lib/format";
import { errMsg, fs } from "../lib/fs";
import type { AnalyzeProgress, CatStat, DirHit, DriveUsage, ExtStat, FileHit, Issue, Page, Prefs, ReportInfo, SearchHit, TreeNode } from "../lib/fs";
import { usedPercent } from "../lib/locations";
import { basename, isDriveRoot, relativeTo, rootOf, storageRoute } from "../lib/path";
import { parseExts, parseSize } from "../lib/searchQuery";
import { useApp } from "../stores/app";
import { useNav } from "../stores/nav";
import { usePrefs } from "../stores/prefs";
import { useStorage } from "../stores/storage";
import type { StorageJob } from "../stores/storage";
import { useUi } from "../stores/ui";
import { EmptyState } from "./EmptyState";
import { EntryIcon } from "./EntryIcon";
import { driveIcon } from "./ComputerView";
import { hitMenu, openLocation } from "./SearchView";
import { StorageTreemap } from "./StorageTreemap";
import type { TreeBlock } from "./StorageTreemap";
import { ChartPie, Cloud, FolderOpen, Pause, Play, RefreshCw, RotateCw, Square, Trash2, TriangleAlert } from "./icons";

const nf = new Intl.NumberFormat();
const PAGE = 200;
const ROW_H = 44;
/** A partir de esta antigüedad se avisa de que el informe puede no reflejar el disco. */
const OLD_REPORT_MS = 7 * 86_400_000;

export const CAT_LABEL: Record<string, string> = {
  image: "Imágenes", video: "Vídeos", audio: "Audio", archive: "Comprimidos", code: "Código", doc: "Documentos",
  sheet: "Hojas de cálculo", slides: "Presentaciones", pdf: "PDF", design: "Diseño", exec: "Aplicaciones", other: "Otros",
};

type Tab = "overview" | "folders" | "files" | "types";
const TABS: [Tab, string][] = [["overview", "Resumen"], ["folders", "Carpetas"], ["files", "Archivos"], ["types", "Tipos"]];

interface FileFilters { cat: string; exts: string; min: string; max: string; avail: string }
const NO_FILTERS: FileFilters = { cat: "", exts: "", min: "", max: "", avail: "" };
const AVAIL_LABEL: Record<Availability, string> = { local: "Solo en este equipo", cloud: "Solo en la nube", synced: "En este equipo y sincronizados", unknown: "Estado desconocido" };
const AVAIL_NOTE: Record<Availability, string> = { local: "", cloud: "No ocupan espacio en este equipo", synced: "Borrarlos aquí también los borra en la nube", unknown: "Informe anterior: vuelve a analizar para clasificarlos" };
const AVAIL_OPTIONS: readonly (readonly [string, string])[] = [["", "Todos"], ...(Object.entries(AVAIL_LABEL) as [string, string][]), ["noncloud", "Sin archivos solo en la nube"]];

const percent = (v: number, total: number) => (total > 0 ? `${((v / total) * 100).toFixed(1)} %` : "—");
const fail = (e: unknown) => useUi.getState().toast(errMsg(e), "error");
const toHit = (f: FileHit): SearchHit => ({ path: f.path, name: f.name, ext: f.ext, isDir: false, size: f.size, modified: f.modified, created: null });
const dirToHit = (d: DirHit): SearchHit => ({ path: d.path, name: d.name, ext: "", isDir: true, size: d.bytes, modified: d.modified, created: null });

/** Lista por páginas: solo hay en memoria lo que se ha ido viendo. Una consulta nueva descarta la anterior. */
function usePaged<T>(fetchPage: (offset: number) => Promise<Page<T>>, key: string) {
  const [state, setState] = useState<{ items: T[]; more: boolean; loading: boolean; error: string | null }>({ items: [], more: false, loading: true, error: null });
  const token = useRef(0);
  const busy = useRef(false);
  const latest = useRef({ fetchPage, state });
  latest.current = { fetchPage, state };

  useEffect(() => {
    const t = ++token.current;
    busy.current = true;
    setState({ items: [], more: false, loading: true, error: null });
    latest.current.fetchPage(0)
      .then((p) => t === token.current && (busy.current = false, setState({ items: p.items, more: p.more, loading: false, error: null })))
      .catch((e) => t === token.current && (busy.current = false, setState({ items: [], more: false, loading: false, error: errMsg(e) })));
  }, [key]);

  const loadMore = useCallback(() => {
    const { fetchPage: f, state: s } = latest.current;
    if (busy.current || !s.more) return;
    busy.current = true;
    const t = token.current;
    f(s.items.length)
      .then((p) => t === token.current && (busy.current = false, setState((o) => ({ ...o, items: [...o.items, ...p.items], more: p.more }))))
      .catch((e) => t === token.current && (busy.current = false, fail(e)));
  }, []);
  return { ...state, loadMore };
}

/** Respuesta de una consulta que depende de `key`; lo que llega tarde se descarta. */
function useQuery<T>(load: () => Promise<T>, key: string): T | null {
  const [v, setV] = useState<T | null>(null);
  const ref = useRef(load);
  ref.current = load;
  useEffect(() => {
    let live = true;
    ref.current().then((x) => live && setV(x)).catch(fail);
    return () => { live = false; };
  }, [key]);
  return v;
}

function Bar({ value, max }: { value: number; max: number }) {
  return <span className="an-bar"><i style={{ width: `${max > 0 ? Math.max(0.6, (value / max) * 100) : 0}%` }} /></span>;
}

function CategoryRows({ cats, total, onPick, limit }: { cats: CatStat[]; total: number; onPick?(c: string): void; limit?: number }) {
  const list = limit ? cats.slice(0, limit) : cats;
  const max = list[0]?.bytes ?? 0;
  return (
    <div className="an-rows">
      {list.map((c) => (
        <button key={c.cat} className="an-row" disabled={!onPick} onClick={() => onPick?.(c.cat)} title={onPick ? `Ver archivos: ${CAT_LABEL[c.cat]}` : undefined}>
          <span className="an-row-name">{CAT_LABEL[c.cat] ?? c.cat}</span>
          <Bar value={c.bytes} max={max} />
          <span className="an-num">{formatBytes(c.bytes)}</span>
          <span className="an-num an-dim">{percent(c.bytes, total)}</span>
          <span className="an-num an-dim">{nf.format(c.files)}</span>
        </button>
      ))}
    </div>
  );
}

function Stat({ label, value, hint }: { label: string; value: ReactNode; hint?: string }) {
  return <div className="an-stat" title={hint}><span className="an-stat-label">{label}</span><span className="an-stat-value">{value}</span></div>;
}

/** Uso real de la unidad frente a lo que el análisis pudo sumar: son datos distintos y se presentan como tales. */
function Capacity({ root, analyzed }: { root: string; analyzed: number }) {
  const version = useStorage((s) => s.version);
  const drive = rootOf(root);
  const hasDrive = /^[a-z]:$/.test(drive);
  const usage = useQuery<DriveUsage | null>(() => (hasDrive ? fs.getDriveUsage(`${drive.toUpperCase()}\\`) : Promise.resolve(null)), `${drive}|${version}`);
  const pct = usedPercent(usage ?? undefined);
  if (!usage || pct === null) return null;
  const shown = Math.min(analyzed, usage.used);
  const other = usage.used - shown;
  const seg = (v: number) => `${(v / usage.total) * 100}%`;
  return (
    <section className="an-card">
      <h2 className="an-h">{driveLabel(drive[0], usage)}</h2>
      <div className="an-stats">
        <Stat label="Capacidad total" value={formatBytes(usage.total)} />
        <Stat label="Espacio usado" value={formatBytes(usage.used)} />
        <Stat label="Espacio disponible" value={formatBytes(usage.free)} />
        <Stat label="Ocupado" value={`${pct.toFixed(1)} %`} />
      </div>
      <div className="an-cap" role="img" aria-label={`${formatBytes(analyzed)} de archivos analizados, ${formatBytes(other)} de otro espacio usado, ${formatBytes(usage.free)} libres`}>
        <i className="an-cap-files" style={{ width: seg(shown) }} />
        <i className="an-cap-other" style={{ width: seg(other) }} />
      </div>
      <div className="an-legend">
        <span><i className="an-cap-files" />Archivos analizados en este equipo · {formatBytes(analyzed)}</span>
        {other > 0 && <span><i className="an-cap-other" />Resto del espacio usado · {formatBytes(other)}</span>}
        <span><i className="an-cap-free" />Libre · {formatBytes(usage.free)}</span>
      </div>
      {analyzed > usage.used && <p className="an-note">Los archivos suman más que el espacio usado: Windows cuenta una sola vez los enlaces duros y los archivos comprimidos ocupan menos en disco.</p>}
    </section>
  );
}

function Toolbar({ children }: { children: ReactNode }) {
  return <div className="an-filters">{children}</div>;
}

function SizeField({ label, value, onChange }: { label: string; value: string; onChange(v: string): void }) {
  return (
    <label className="sv-field"><span>{label}</span>
      <input value={value} spellCheck={false} placeholder="1 GB" aria-invalid={!parseSize(value).ok || undefined} onChange={(e) => onChange(e.target.value)} />
    </label>
  );
}

const sizeOf = (t: string) => { const p = parseSize(t); return p.ok && p.bytes !== null ? p.bytes : undefined; };

/** Fila virtualizada común a las listas del analizador. */
function VirtualRows<T>({ items, more, loadMore, render, label }: { items: T[]; more: boolean; loadMore(): void; render(item: T, top: number): ReactNode; label: string }) {
  const scroller = useRef<HTMLDivElement>(null);
  const vv = useVirtualizer({ count: items.length, getScrollElement: () => scroller.current, estimateSize: () => ROW_H, overscan: 10 });
  const rows = vv.getVirtualItems();
  const last = rows.length ? rows[rows.length - 1].index : -1;
  useEffect(() => {
    if (more && last >= items.length - 30) loadMore();
  }, [last, items.length, more, loadMore]);
  return (
    <div className="an-list" ref={scroller} role="list" aria-label={label}>
      <div style={{ height: vv.getTotalSize(), position: "relative" }}>{rows.map((r) => (items[r.index] ? render(items[r.index], r.start) : null))}</div>
    </div>
  );
}

const FileRow = memo(function FileRow({ f, top }: { f: FileHit; top: number }) {
  const hit = useMemo(() => toHit(f), [f]);
  const dir = f.path.slice(0, Math.max(0, f.path.length - f.name.length - 1));
  const entry = useMemo(() => ({ name: f.name, kind: "file" as const, size: f.size, modified: f.modified, created: null, hidden: false, ext: f.ext || null, link: false, cloud: f.cloud }), [f]);
  return (
    <div
      className="an-tr an-tr-files" role="listitem" style={{ transform: `translateY(${top}px)`, height: ROW_H }}
      onDoubleClick={() => openLocation(hit)} onContextMenu={(ev) => (ev.preventDefault(), useUi.getState().openMenu(ev.clientX, ev.clientY, hitMenu(hit)))}
    >
      <span className="an-name">
        <span className="an-ico"><EntryIcon entry={entry} basePath={dir} size={22} /></span>
        <span className="an-text"><span className="an-title" title={f.name}>{f.name}{f.cloud && <Cloud className="link-badge" aria-label="Solo en la nube"><title>Solo en la nube</title></Cloud>}</span><span className="an-path" title={f.path}>{dir}</span></span>
      </span>
      <span className="an-cell">{kindLabel(entry)}</span>
      <span className="an-num">{formatBytes(f.size)}</span>
      <span className="an-cell">{formatDate(f.modified)}</span>
      <button className="ibtn" title="Abrir ubicación en LFind" aria-label={`Abrir la ubicación de ${f.name}`} onClick={() => openLocation(hit)}><FolderOpen /></button>
    </div>
  );
});

function FilesTab({ root, dir, version, filters, setFilters }: { root: string; dir: number; version: number; filters: FileFilters; setFilters(f: FileFilters): void }) {
  const q = { dir, cat: filters.cat || undefined, exts: parseExts(filters.exts), min: sizeOf(filters.min), max: sizeOf(filters.max), avail: filters.avail || undefined, limit: PAGE };
  const key = JSON.stringify([root, version, q]);
  const page = usePaged((offset) => fs.analyzerFiles(root, { ...q, offset }), key);
  const set = (p: Partial<FileFilters>) => setFilters({ ...filters, ...p });
  return (
    <div className="an-tabbody">
      <Toolbar>
        <label className="sv-field"><span>Tipo</span>
          <Select label="Tipo" value={filters.cat} options={[["", "Todos"], ...Object.entries(CAT_LABEL)]} onChange={(cat) => set({ cat })} />
        </label>
        <label className="sv-field"><span>Disponibilidad</span>
          <Select label="Disponibilidad" value={filters.avail} options={AVAIL_OPTIONS} onChange={(avail) => set({ avail })} />
        </label>
        <label className="sv-field"><span>Extensión</span><input value={filters.exts} spellCheck={false} placeholder="mp4, iso" onChange={(e) => set({ exts: e.target.value })} /></label>
        <SizeField label="Tamaño mínimo" value={filters.min} onChange={(min) => set({ min })} />
        <SizeField label="Tamaño máximo" value={filters.max} onChange={(max) => set({ max })} />
      </Toolbar>
      <div className="an-tr an-tr-files an-th" role="presentation"><span>Nombre y ruta</span><span>Tipo</span><span className="an-num">Tamaño ↓</span><span>Modificado</span><span /></div>
      {page.error ? <EmptyState title="No se pudo consultar">{page.error}</EmptyState>
        : !page.loading && !page.items.length ? <EmptyState title="Sin archivos">Ningún archivo coincide con los filtros.</EmptyState>
        : <VirtualRows items={page.items} more={page.more} loadMore={page.loadMore} label="Archivos más grandes" render={(f, top) => <FileRow key={f.path} f={f} top={top} />} />}
    </div>
  );
}

type DirSort = "size" | "name" | "files";

function FoldersTab({ root, node, version, total, enter }: { root: string; node: DirHit; version: number; total: number; enter(d: DirHit): void }) {
  const [sort, setSort] = useState<DirSort>("size");
  const [desc, setDesc] = useState(true);
  const [tree, setTree] = useState(false);
  const [min, setMin] = useState("");
  const [max, setMax] = useState("");
  const q = { dir: node.id, tree, sort, desc, min: sizeOf(min), max: sizeOf(max), limit: PAGE };
  const page = usePaged((offset) => fs.analyzerDirs(root, { ...q, offset }), JSON.stringify([root, version, q]));
  const head = (k: DirSort, label: string, cls = "") => (
    <button className={cls} aria-sort={sort === k ? (desc ? "descending" : "ascending") : undefined} onClick={() => (sort === k ? setDesc(!desc) : (setSort(k), setDesc(k !== "name")))}>
      {label}{sort === k ? (desc ? " ↓" : " ↑") : ""}
    </button>
  );
  return (
    <div className="an-tabbody">
      <Toolbar>
        <div className="sv-field"><span>Mostrar</span>
          <div className="sv-seg" role="group" aria-label="Alcance">
            <button aria-pressed={!tree} onClick={() => setTree(false)}>Esta carpeta</button>
            <button aria-pressed={tree} onClick={() => setTree(true)}>Todas las subcarpetas</button>
          </div>
        </div>
        <SizeField label="Tamaño mínimo" value={min} onChange={setMin} />
        <SizeField label="Tamaño máximo" value={max} onChange={setMax} />
      </Toolbar>
      <div className="an-tr an-tr-dirs an-th" role="presentation">
        {head("name", "Nombre y ruta")}{head("size", "Tamaño", "an-num")}<span className="an-num">% del total</span>{head("files", "Archivos", "an-num")}<span className="an-num">Subcarpetas</span><span />
      </div>
      {page.error ? <EmptyState title="No se pudo consultar">{page.error}</EmptyState>
        : !page.loading && !page.items.length ? <EmptyState title="Sin carpetas">No hay subcarpetas que coincidan.</EmptyState>
        : <VirtualRows items={page.items} more={page.more} loadMore={page.loadMore} label="Carpetas por tamaño" render={(d, top) => (
          <div
            key={d.id} className="an-tr an-tr-dirs an-click" role="listitem" tabIndex={0} style={{ transform: `translateY(${top}px)`, height: ROW_H }}
            onClick={() => enter(d)} onKeyDown={(e) => e.key === "Enter" && enter(d)}
            onContextMenu={(ev) => (ev.preventDefault(), useUi.getState().openMenu(ev.clientX, ev.clientY, hitMenu(dirToHit(d))))}
          >
            <span className="an-name"><span className="an-text"><span className="an-title" title={d.name}>{d.name}{d.cloudPending && <Cloud className="link-badge" aria-label="Contenido en la nube, sin explorar"><title>Contenido en la nube, sin explorar</title></Cloud>}{d.unreadable && <TriangleAlert className="link-badge" aria-label="Sin acceso"><title>Sin acceso</title></TriangleAlert>}</span><span className="an-path" title={d.path}>{d.path}</span></span></span>
            <span className="an-num">{formatBytes(d.bytes)}</span>
            <span className="an-num an-dim">{percent(d.bytes, total)}</span>
            <span className="an-num">{nf.format(d.files)}</span>
            <span className="an-num">{nf.format(d.dirs)}</span>
            <button className="ibtn" title="Abrir en LFind" aria-label={`Abrir ${d.name} en LFind`} onClick={(ev) => (ev.stopPropagation(), useNav.getState().navigate(d.path))}><FolderOpen /></button>
          </div>
        )} />}
    </div>
  );
}

/** Reparte los archivos según dónde están. Se calcula con lo guardado en el informe: no toca los archivos. */
function AvailabilityCard({ root, dir, version, value, onChange }: { root: string; dir: number; version: number; value: string; onChange(v: string): void }) {
  const stats = useQuery<AvailStat[]>(() => fs.analyzerAvailability(root, dir), `${root}|${dir}|${version}`);
  if (!stats) return null;
  const onDisk = stats.reduce((n, s) => n + (s.kind === "cloud" ? 0 : s.bytes), 0);
  const logical = stats.reduce((n, s) => n + s.bytes, 0);
  return (
    <section className="an-card">
      <h2 className="an-h">Dónde están los archivos</h2>
      <div className="an-rows-head an-avail"><span>Estado</span><span className="an-num">Archivos</span><span className="an-num">Tamaño</span><span className="an-num">En este equipo</span></div>
      <div className="an-rows">
        {stats.map((s) => (
          <button key={s.kind} className="an-row an-avail" aria-pressed={value === s.kind} title={AVAIL_NOTE[s.kind] || undefined} onClick={() => onChange(value === s.kind ? "" : s.kind)}>
            <span className="an-row-name">{AVAIL_LABEL[s.kind]}{AVAIL_NOTE[s.kind] && <em className="an-avail-note">{AVAIL_NOTE[s.kind]}</em>}</span>
            <span className="an-num">{s.files.toLocaleString("es")}</span>
            <span className="an-num">{formatBytes(s.bytes)}</span>
            <span className="an-num">{formatBytes(s.kind === "cloud" ? 0 : s.bytes)}</span>
          </button>
        ))}
      </div>
      <p className="an-note">Total {formatBytes(logical)} de tamaño, {formatBytes(onDisk)} en este equipo.</p>
    </section>
  );
}

function TypesTab({ root, dir, version, cats: allCats, total: allTotal, pickCat, pickExt, avail, setAvail }: { root: string; dir: number; version: number; cats: CatStat[] | null; total: number; avail: string; setAvail(v: string): void; pickCat(c: string): void; pickExt(e: ExtStat): void }) {
  const page = usePaged((offset) => fs.analyzerExts(root, dir, offset, PAGE, avail || undefined), `${root}|${dir}|${version}|${avail}`);
  const filtered = useQuery<CatStat[]>(() => (avail ? fs.analyzerCategories(root, dir, avail) : Promise.resolve([])), `${root}|${dir}|${version}|${avail}`);
  const cats = avail ? filtered : allCats;
  const total = avail ? (filtered ?? []).reduce((n, c) => n + c.bytes, 0) : allTotal;
  const max = page.items[0]?.bytes ?? 0;
  return (
    <div className="an-scroll">
      <AvailabilityCard root={root} dir={dir} version={version} value={avail} onChange={setAvail} />
      <section className="an-card">
        <h2 className="an-h">Por tipo de archivo{avail && <span className="an-note"> · {AVAIL_OPTIONS.find(([v]) => v === avail)?.[1]}</span>}</h2>
        <div className="an-rows-head"><span>Tipo</span><span /><span className="an-num">Tamaño</span><span className="an-num">%</span><span className="an-num">Archivos</span></div>
        {cats ? <CategoryRows cats={cats} total={total} onPick={pickCat} /> : <p className="an-note">Calculando…</p>}
      </section>
      <section className="an-card">
        <h2 className="an-h">Por extensión</h2>
        <div className="an-rows-head"><span>Extensión</span><span /><span className="an-num">Tamaño</span><span className="an-num">%</span><span className="an-num">Archivos</span></div>
        <div className="an-rows">
          {page.items.map((e) => {
            const name = e.ext === "" ? "Sin extensión" : e.ext === "*" ? "Otras" : `.${e.ext}`;
            const can = e.ext !== "" && e.ext !== "*";
            return (
              <button key={e.ext} className="an-row" disabled={!can} onClick={() => pickExt(e)}>
                <span className="an-row-name">{name}</span><Bar value={e.bytes} max={max} /><span className="an-num">{formatBytes(e.bytes)}</span><span className="an-num an-dim">{percent(e.bytes, total)}</span><span className="an-num an-dim">{nf.format(e.files)}</span>
              </button>
            );
          })}
        </div>
        {page.more && <button className="btn an-more" onClick={page.loadMore}>Mostrar más</button>}
      </section>
    </div>
  );
}

function Overview({ root, at, node, version, cats, total, enter, openTab, pickCat }: { root: string; at: string; node: DirHit; version: number; cats: CatStat[] | null; total: number; enter(rel: string): void; openTab(t: Tab): void; pickCat(c: string): void }) {
  const key = `${root}|${node.id}|${version}`;
  const tree = useQuery<TreeNode[]>(() => fs.analyzerTreemap(root, node.id), key);
  const files = useQuery(() => fs.analyzerFiles(root, { dir: node.id, limit: 6 }), key);
  const blocks = useMemo<TreeBlock[]>(() => {
    if (!tree) return [];
    const out: TreeBlock[] = tree.map((t) => ({ key: `d${t.id}`, label: t.name, value: t.bytes, folder: t.name, kids: t.kids }));
    if (node.ownBytes > 0) out.push({ key: "own", label: "Archivos de esta carpeta", value: node.ownBytes, folder: null, files: true });
    const shown = out.reduce((a, b) => a + b.value, 0);
    if (node.bytes - shown > 0) out.push({ key: "rest", label: "Carpetas menores", value: node.bytes - shown, folder: null });
    return out;
  }, [tree, node]);
  const rel = (name: string) => (at ? `${at}/${name}` : name);
  return (
    <div className="an-scroll">
      <section className="an-card">
        <h2 className="an-h">Dónde está el espacio</h2>
        {tree && !blocks.length ? <p className="an-note">Esta carpeta no contiene datos.</p> : <StorageTreemap blocks={blocks} total={node.bytes} onFolder={(r) => enter(rel(r))} onFiles={() => openTab("files")} />}
      </section>
      <div className="an-two">
        <section className="an-card">
          <h2 className="an-h">Por tipo de archivo<button className="an-link" onClick={() => openTab("types")}>Ver todo</button></h2>
          {cats ? <CategoryRows cats={cats} total={total} onPick={pickCat} limit={8} /> : <p className="an-note">Calculando…</p>}
        </section>
        <section className="an-card">
          <h2 className="an-h">Archivos más grandes<button className="an-link" onClick={() => openTab("files")}>Ver todos</button></h2>
          <div className="an-rows">
            {files?.items.map((f) => (
              <button key={f.path} className="an-row an-row-file" title={f.path} onClick={() => openLocation(toHit(f))} onContextMenu={(ev) => (ev.preventDefault(), useUi.getState().openMenu(ev.clientX, ev.clientY, hitMenu(toHit(f))))}>
                <span className="an-row-name">{f.name}</span><span className="an-num">{formatBytes(f.size)}</span>
              </button>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}

const REFRESH_MODES: [Prefs["analyzerRefresh"], string][] = [["auto", "Automática"], ["manual", "Manual"], ["off", "Desactivada"]];

function Actions({ job, onStart, root }: { job: StorageJob | null; onStart(): void; root: string }) {
  const mine = job?.root === root;
  const updating = useStorage((s) => s.updating);
  const mode = usePrefs((s) => s.analyzerRefresh);
  const busy = !!job || !!updating;
  return (
    <div className="an-actions">
      <label className="an-mode" title="Automática: mientras LFind está abierto, actualiza en segundo plano lo que cambia. Manual: solo con el botón. Desactivada: el informe no cambia hasta un nuevo análisis.">
        <span>Actualización</span>
        <Select label="Actualización" value={mode} options={REFRESH_MODES} onChange={(analyzerRefresh) => (usePrefs.getState().update({ analyzerRefresh }), useStorage.getState().syncWatch(root))} />
      </label>
      {mode !== "off" && <button className="btn" disabled={busy} title="Relee solo las carpetas que han cambiado" onClick={() => useStorage.getState().update(root)}><RotateCw className="an-btn-ico" />{updating ? "Actualizando…" : "Actualizar cambios"}</button>}
      <button className="btn" disabled={busy} title={job && !mine ? `Hay un análisis en curso de ${job.root}` : "Vuelve a recorrer toda la ubicación"} onClick={onStart}><RefreshCw className="an-btn-ico" />Analizar de nuevo</button>
    </div>
  );
}

function Issues({ root, count }: { root: string; count: number }) {
  const [list, setList] = useState<Issue[] | null>(null);
  return (
    <details className="an-issues" onToggle={(e) => { if (e.currentTarget.open && !list) fs.analyzerIssues(root).then(setList).catch(fail); }}>
      <summary><TriangleAlert className="an-btn-ico" />{nf.format(count)} {count === 1 ? "ubicación no se pudo leer" : "ubicaciones no se pudieron leer"} (sin permisos o protegidas); su contenido no está sumado</summary>
      <ul>{list?.map((i) => <li key={i.path} title={i.message}>{i.path}</li>)}</ul>
    </details>
  );
}

function ReportBody({ root, at, report, version }: { root: string; at: string; report: ReportInfo; version: number }) {
  const navigate = useNav((s) => s.navigate);
  const [tab, setTab] = useState<Tab>("overview");
  const [filters, setFilters] = useState<FileFilters>(NO_FILTERS);
  const start = useStorage((s) => s.start);
  const job = useStorage((s) => s.job);
  const node = useQuery<DirHit | null>(() => fs.analyzerNode(root, at), `${root}|${at}|${version}`);
  const cats = useQuery<CatStat[]>(() => (node ? fs.analyzerCategories(root, node.id) : Promise.resolve([])), `${root}|${node?.id}|${version}`);
  const enter = useCallback((rel: string) => navigate(storageRoute(root, rel)), [navigate, root]);
  const enterDir = useCallback((d: DirHit) => enter(relativeTo(root, d.path)), [enter, root]);

  if (node === null) return <EmptyState title="Cargando el informe…"> </EmptyState>;
  const old = Date.now() - report.finishedAt > OLD_REPORT_MS;
  const stale = report.state === "missing" ? "Esta ubicación ya no existe: el informe muestra cómo estaba al analizarla." : report.state === "otherVolume" ? "Ahora hay otro disco en esta letra de unidad: este informe corresponde al disco anterior." : old ? "Este informe tiene más de una semana y puede no reflejar el estado actual." : null;
  const pickCat = (cat: string) => (setFilters({ ...NO_FILTERS, cat, avail: filters.avail }), setTab("files"));
  const pickExt = (e: ExtStat) => (setFilters({ ...NO_FILTERS, exts: e.ext, avail: filters.avail }), setTab("files"));
  return (
    <div className="an" data-flow={tab === "overview" || tab === "types" || undefined}>
      <div className="an-top">
        <div className="an-heading">
          <h1 className="an-title-main" title={node.path}>{isDriveRoot(node.path) ? node.path : basename(node.path)}</h1>
          <p className="an-sub-main" title={node.path}>{report.updatedDirs > 0 ? `Actualizado: ${formatDate(report.finishedAt)} · ${nf.format(report.updatedDirs)} carpetas releídas · Análisis completo: ${formatDate(report.fullScanAt)}` : `Último análisis: ${formatDate(report.finishedAt)} · ${formatDuration(report.durationMs)}`}{report.checkedAt > report.finishedAt ? ` · Comprobado: ${formatDate(report.checkedAt)}` : ""}</p>
        </div>
        <Actions job={job} root={report.root} onStart={() => start(report.root)} />
      </div>
      {stale && <div className="an-warn" role="status"><TriangleAlert className="an-btn-ico" />{stale}</div>}
      {report.partial && <div className="an-partial" role="status" title="Un archivo que cambia de tamaño sin que cambie la fecha de su carpeta solo se detecta con la vigilancia automática o con un análisis completo.">Resultados parciales: actualizados según las fechas de las carpetas. «Analizar de nuevo» los deja exactos.</div>}
      {tab === "overview" && (
        <div className="an-summary">
          <Capacity root={report.root} analyzed={report.bytes - report.cloudBytes} />
          <section className="an-card">
            <h2 className="an-h">{at ? "Esta carpeta" : "Lo analizado"}</h2>
            <div className="an-stats">
              <Stat label="Archivos" value={nf.format(node.files)} />
              <Stat label="Carpetas" value={nf.format(node.dirs)} />
              <Stat label="Tamaño analizado" value={formatBytes(node.bytes)} hint="Suma de los tamaños de los archivos, incluidos los que solo están en la nube." />
              {at ? <Stat label="Del total analizado" value={percent(node.bytes, report.bytes)} /> : <Stat label="Solo en la nube" value={formatBytes(report.cloudBytes)} hint="Forman parte del tamaño analizado pero no ocupan disco en este equipo." />}
            </div>
            <p className="an-note">El tamaño analizado es la suma de los tamaños de los archivos a los que se tuvo acceso. Puede no coincidir con el espacio que Windows da por usado: la compresión, los enlaces duros, las carpetas protegidas y los archivos del sistema influyen.{report.links ? ` ${nf.format(report.links)} enlaces no se siguieron.` : ""}{report.cloudDirs ? ` ${nf.format(report.cloudDirs)} carpetas en la nube no se exploraron para no descargar nada.` : ""}</p>
          </section>
        </div>
      )}
      {report.errors > 0 && tab === "overview" && <Issues root={report.root} count={report.errors} />}
      <div className="sv-seg an-tabs" role="tablist" aria-label="Secciones del informe">
        {TABS.map(([k, l]) => <button key={k} role="tab" aria-selected={tab === k} aria-pressed={tab === k} onClick={() => setTab(k)}>{l}</button>)}
      </div>
      {tab === "overview" && <Overview root={root} at={at} node={node} version={version} cats={cats} total={node.bytes} enter={enter} openTab={setTab} pickCat={pickCat} />}
      {tab === "folders" && <FoldersTab root={root} node={node} version={version} total={report.bytes} enter={enterDir} />}
      {tab === "files" && <FilesTab root={root} dir={node.id} version={version} filters={filters} setFilters={setFilters} />}
      {tab === "types" && <TypesTab root={root} dir={node.id} version={version} cats={cats} total={node.bytes} pickCat={pickCat} pickExt={pickExt} avail={filters.avail} setAvail={(avail) => setFilters({ ...filters, avail })} />}
    </div>
  );
}

function formatDuration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${Math.max(s, 1)} s`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m} min ${s % 60} s` : `${Math.floor(m / 60)} h ${m % 60} min`;
}

/** Análisis en curso: todo lo de aquí llega ya resumido desde Rust, con un tamaño fijo sea cual sea el disco. */
function Live({ job }: { job: StorageJob }) {
  const { pause, resume, cancel } = useStorage.getState();
  const p: AnalyzeProgress | null = job.progress;
  const usage = useApp((s) => (isDriveRoot(job.root) ? s.usage[job.root] : undefined));
  const local = p ? p.bytes - p.cloudBytes : 0;
  const pct = usage && usage.used > 0 ? Math.min(99, (local / usage.used) * 100) : null;
  const finishing = job.phase === "finishing";
  const status = job.phase === "paused" ? "En pausa" : job.phase === "cancelling" ? "Cancelando…" : finishing ? "Guardando el informe…" : "Analizando…";
  const dirMax = p?.topDirs[0]?.bytes ?? 0;
  const cats = useMemo(() => [...(p?.categories ?? [])].sort((a, b) => b.bytes - a.bytes), [p?.categories]);
  return (
    <div className="an" data-flow>
      <div className="an-top">
        <div className="an-heading"><h1 className="an-title-main" title={job.root}>{isDriveRoot(job.root) ? job.root : basename(job.root)}</h1><p className="an-sub-main">{status}{p ? ` · ${formatDuration(p.elapsedMs)}` : ""}</p></div>
        <div className="an-actions">
          {job.phase === "paused" ? <button className="btn" onClick={resume}><Play className="an-btn-ico" />Reanudar</button> : <button className="btn" disabled={finishing || job.phase === "cancelling"} onClick={pause}><Pause className="an-btn-ico" />Pausar</button>}
          <button className="btn" disabled={job.phase === "cancelling"} onClick={cancel}><Square className="an-btn-ico" />Cancelar</button>
        </div>
      </div>
      <div className="an-scroll">
        <section className="an-card">
          <div className="an-progress" role="progressbar" aria-label="Progreso del análisis" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct === null ? undefined : Math.round(pct)} data-indet={pct === null && job.phase === "scanning" ? true : undefined}><i style={pct === null ? undefined : { width: `${pct}%` }} /></div>
          <div className="an-stats">
            <Stat label="Archivos" value={nf.format(p?.files ?? 0)} />
            <Stat label="Carpetas" value={nf.format(p?.dirs ?? 0)} />
            <Stat label="Tamaño hasta ahora" value={formatBytes(p?.bytes ?? 0)} />
            <Stat label="Sin acceso" value={nf.format(p?.errors ?? 0)} />
          </div>
          <p className="an-current" title={p?.current}>{p?.current ?? "Preparando…"}</p>
          <p className="an-note">Resultados parciales: cambiarán hasta que termine. Solo se leen metadatos, con prioridad baja; ningún archivo se abre, se mueve ni se descarga.</p>
        </section>
        {p && (
          <div className="an-two">
            <section className="an-card"><h2 className="an-h">Carpetas de primer nivel</h2>
              <div className="an-rows">{p.topDirs.slice(0, 12).map((d) => <div key={d.name} className="an-row"><span className="an-row-name">{d.name}</span><Bar value={d.bytes} max={dirMax} /><span className="an-num">{formatBytes(d.bytes)}</span><span className="an-num an-dim">{percent(d.bytes, p.bytes)}</span><span className="an-num an-dim">{nf.format(d.files)}</span></div>)}</div>
            </section>
            <section className="an-card"><h2 className="an-h">Por tipo de archivo</h2><CategoryRows cats={cats} total={p.bytes} /></section>
            <section className="an-card an-wide"><h2 className="an-h">Archivos más grandes hasta ahora</h2>
              <div className="an-rows">{p.topFiles.slice(0, 10).map((f) => <div key={f.path} className="an-row an-row-file" title={f.path}><span className="an-row-name">{f.path}</span><span className="an-num">{formatBytes(f.size)}</span></div>)}</div>
            </section>
          </div>
        )}
      </div>
    </div>
  );
}

function Report({ root, at }: { root: string; at: string }) {
  const job = useStorage((s) => s.job);
  const version = useStorage((s) => s.version);
  const start = useStorage((s) => s.start);
  const [report, setReport] = useState<ReportInfo | null | undefined>(undefined);
  // Al cambiar de ubicación se vacía; una actualización del mismo informe no interrumpe lo que se está viendo.
  useEffect(() => setReport(undefined), [root]);
  useEffect(() => {
    let live = true;
    fs.analyzerReport(root).then((r) => live && setReport(r)).catch((e) => live && (setReport(null), fail(e)));
    return () => { live = false; };
  }, [root, version]);
  const hasReport = !!report;
  useEffect(() => {
    if (hasReport) useStorage.getState().syncWatch(root);
    return () => useStorage.getState().syncWatch(null);
  }, [root, hasReport]);
  const running = job && relativeTo(job.root, root) === "" && relativeTo(root, job.root) === "";
  if (running) return <Live job={job} />;
  if (report === undefined) return null;
  if (report === null) {
    return (
      <EmptyState title="Sin análisis de esta ubicación">
        <p>{root}</p>
        <button className="btn primary" disabled={!!job} onClick={() => start(root)}>Analizar ahora</button>
      </EmptyState>
    );
  }
  return <ReportBody key={`${report.key}|${at}`} root={root} at={at} report={report} version={version} />;
}

function DriveTile({ root, label, kind, usage, last, onPick }: { root: string; label: string; kind: string | null | undefined; usage?: DriveUsage; last?: ReportInfo; onPick(): void }) {
  const Icon = driveIcon(kind);
  const pct = usedPercent(usage);
  return (
    <button className="drive-card" title={root} onClick={onPick}>
      <Icon className="drive-card-ico" />
      <span className="drive-card-main">
        <span className="drive-card-name">{label}</span>
        <span className="drive-card-kind">{last ? `Analizada: ${formatDate(last.finishedAt)}` : "Sin analizar"}</span>
        {pct !== null && <span className="drive-bar" data-low={pct > 90 || undefined}><i style={{ width: `${pct}%` }} /></span>}
        {usage && <span className="drive-text">{formatBytes(usage.used)} usados de {formatBytes(usage.total)}</span>}
      </span>
    </button>
  );
}

function Home() {
  const drives = useApp((s) => s.drives);
  const usage = useApp((s) => s.usage);
  const reports = useStorage((s) => s.reports);
  const job = useStorage((s) => s.job);
  const { analyze, forget } = useStorage.getState();
  const navigate = useNav((s) => s.navigate);
  const toast = useUi((s) => s.toast);
  const [folder, setFolder] = useState("");
  const byRoot = useMemo(() => new Map(reports.map((r) => [r.root.toLowerCase(), r])), [reports]);
  useEffect(() => useStorage.getState().loadReports(), []);
  const submit = () => {
    const t = folder.trim();
    if (t) fs.resolvePath(t).then((r) => (r.type === "dir" ? analyze(r.path) : toast("Escribe la ruta de una carpeta.", "error"))).catch(fail);
  };
  return (
    <div className="an an-page">
      <header className="an-top"><div className="an-heading"><h1 className="an-title-main"><ChartPie className="an-title-ico" />Analizador de almacenamiento</h1><p className="an-sub-main">Descubre qué ocupa el espacio. El análisis solo lee metadatos y nunca modifica tus archivos.</p></div></header>
      {job && <button className="an-warn an-go" onClick={() => navigate(storageRoute(job.root))}>Análisis en curso de {job.root} · {nf.format(job.progress?.files ?? 0)} archivos. Ver progreso</button>}
      <section aria-label="Unidades">
        <h2 className="computer-title">Unidades</h2>
        <div className="drive-grid">
          {drives.map((d) => {
            const u = usage[d.path];
            const kind = u?.kind ?? d.driveKind;
            return <DriveTile key={d.id} root={d.path} kind={kind} usage={u} last={byRoot.get(d.path.toLowerCase())} label={driveLabel(d.path[0] ?? "", u ?? { label: null, kind: kind ?? "unknown" })} onPick={() => analyze(d.path)} />;
          })}
        </div>
      </section>
      <section aria-label="Analizar una carpeta">
        <h2 className="computer-title">Una carpeta</h2>
        <form className="an-folder" onSubmit={(e) => (e.preventDefault(), submit())}>
          <input value={folder} spellCheck={false} placeholder="C:\Users\…" aria-label="Ruta de la carpeta" onChange={(e) => setFolder(e.target.value)} />
          <button className="btn" type="submit" disabled={!folder.trim()}>Analizar</button>
        </form>
        <p className="an-note">También puedes hacer clic derecho en cualquier carpeta o en el fondo de una carpeta y elegir «Analizar almacenamiento».</p>
      </section>
      {reports.length > 0 && (
        <section aria-label="Informes guardados">
          <h2 className="computer-title">Informes guardados</h2>
          <div className="an-rows">
            {reports.map((r) => (
              <div key={r.key} className="an-saved">
                <button className="an-saved-open" title={r.root} onClick={() => navigate(storageRoute(r.root))}><span className="an-row-name">{r.root}</span><span className="an-dim">{formatDate(r.finishedAt)}</span><span className="an-num">{formatBytes(r.bytes)}</span><span className="an-num an-dim">{nf.format(r.files)} archivos</span></button>
                <button className="ibtn" title={`Quitar este informe (${formatBytes(r.dbBytes)}). Tus archivos no se tocan.`} aria-label={`Quitar el informe de ${r.root}`} onClick={() => forget(r.root)}><Trash2 /></button>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

/** Pantalla del analizador. La ruta virtual decide qué se muestra, así funcionan Atrás, Adelante, Subir y las migas. */
export function StorageView({ root, at }: { root: string | null; at: string }) {
  if (!fs.available) return <EmptyState title="Analizador no disponible">El análisis usa el motor de Rust; abre LFind como aplicación de escritorio.</EmptyState>;
  return root ? <Report root={root} at={at} /> : <Home />;
}

/** Barra inferior del analizador. */
export function StorageStatusInfo() {
  const job = useStorage((s) => s.job);
  return (
    <span className="status-left">
      Analizador de almacenamiento
      {job && <span className="busy"><i className="dot" />{job.phase === "paused" ? "En pausa" : `${nf.format(job.progress?.files ?? 0)} archivos`}</span>}
    </span>
  );
}
