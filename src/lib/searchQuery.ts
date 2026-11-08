// Lógica pura de la búsqueda global: de lo que el usuario escribe y marca en la interfaz a la consulta que entiende el índice (Rust).
// Sin React ni IPC: se prueba aislada (`searchQuery.test.ts`).

import type { Entry, IndexStatus, SearchHit, SearchQuery } from "./fs/types";

export type DateField = "modified" | "created" | "accessed";
export type DatePreset = "any" | "today" | "7d" | "30d" | "365d" | "custom";

/** Estado de los filtros tal como lo editan los campos de la interfaz (texto sin interpretar). */
export interface SearchFilters {
  exact: boolean;
  /** Extensiones separadas por coma, espacio o punto y coma ("psd, png" · "*.pdf"). */
  exts: string;
  /** "" = cualquiera; si no, una categoría de `fileKinds` ("image", "design", "pdf"…). */
  category: string;
  kind: "any" | "file" | "dir";
  /** "1 GB", "500k", "1,5 mb"… ("" = sin límite). */
  minSize: string;
  maxSize: string;
  dateField: DateField;
  datePreset: DatePreset;
  /** `yyyy-mm-dd` (solo con `datePreset === "custom"`). */
  dateFrom: string;
  dateTo: string;
  /** Raíz indexada ("C:\\"); "" = todas. */
  drive: string;
  folder: string;
  sort: "relevance" | "name" | "size" | "modified";
  desc: boolean;
}

export const DEFAULT_FILTERS: SearchFilters = {
  exact: false,
  exts: "",
  category: "",
  kind: "any",
  minSize: "",
  maxSize: "",
  dateField: "modified",
  datePreset: "any",
  dateFrom: "",
  dateTo: "",
  drive: "",
  folder: "",
  sort: "relevance",
  desc: false,
};

export const PAGE_SIZE = 200;

const DAY = 86_400_000;

const UNIT: Record<string, number> = { "": 1, b: 1, k: 1024, kb: 1024, m: 1024 ** 2, mb: 1024 ** 2, g: 1024 ** 3, gb: 1024 ** 3, t: 1024 ** 4, tb: 1024 ** 4 };

export type SizeParse = { ok: true; bytes: number | null } | { ok: false };

/** "1 GB" → 1073741824. Vacío → sin límite. Acepta coma decimal. Unidades binarias (1 KB = 1024 B), como el resto de LFind. */
export function parseSize(text: string): SizeParse {
  const t = text.trim().toLowerCase().replace(/\s+/g, "");
  if (!t) return { ok: true, bytes: null };
  const m = /^(\d+(?:[.,]\d+)?)([a-z]*)$/.exec(t);
  if (!m) return { ok: false };
  const mult = UNIT[m[2]];
  if (mult === undefined) return { ok: false };
  const v = Number(m[1].replace(",", ".")) * mult;
  return Number.isFinite(v) && v <= Number.MAX_SAFE_INTEGER ? { ok: true, bytes: Math.floor(v) } : { ok: false };
}

/** Extensiones de un texto libre: sin punto ni comodín, en minúsculas, sin repetir. */
export function parseExts(text: string): string[] {
  const out = new Set<string>();
  for (const raw of text.split(/[\s,;]+/)) {
    const e = raw.trim().replace(/^\*?\./, "").toLowerCase();
    if (e) out.add(e);
  }
  return [...out];
}

/** Extrae del texto los operadores `ext:psd` y `*.psd`; lo demás sigue siendo texto del nombre. */
export function extractOperators(text: string): { text: string; exts: string[] } {
  const exts: string[] = [];
  const rest: string[] = [];
  for (const tok of text.split(/\s+/).filter(Boolean)) {
    const low = tok.toLowerCase();
    if (low.startsWith("ext:") && low.length > 4) exts.push(...parseExts(low.slice(4)));
    else if (low.startsWith("*.") && low.length > 2) exts.push(...parseExts(low));
    else rest.push(tok);
  }
  return { text: rest.join(" "), exts };
}

/** Inicio (00:00 local) del día de `ms`. */
export function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

function parseDay(s: string, end: boolean): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s.trim());
  if (!m) return null;
  const d = end ? new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), 23, 59, 59, 999) : new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), 0, 0, 0, 0);
  const t = d.getTime();
  return Number.isNaN(t) ? null : t;
}

/** Rango de fechas del filtro (en milisegundos epoch), o `null` si no hay filtro de fecha. */
export function dateRange(f: Pick<SearchFilters, "datePreset" | "dateFrom" | "dateTo">, now: number): { after: number | null; before: number | null } | null {
  switch (f.datePreset) {
    case "any": return null;
    case "today": return { after: startOfDay(now), before: null };
    case "7d": return { after: now - 7 * DAY, before: null };
    case "30d": return { after: now - 30 * DAY, before: null };
    case "365d": return { after: now - 365 * DAY, before: null };
    case "custom": {
      const after = parseDay(f.dateFrom, false);
      const before = parseDay(f.dateTo, true);
      return after === null && before === null ? null : { after, before };
    }
  }
}

/** ¿Algún campo de tamaño tiene un valor que no se entiende? (la interfaz lo marca y no lo aplica) */
export function invalidSizes(f: Pick<SearchFilters, "minSize" | "maxSize">): { min: boolean; max: boolean } {
  return { min: !parseSize(f.minSize).ok, max: !parseSize(f.maxSize).ok };
}

const NO_RANGE = { after: null, before: null } as const;

export interface BuildCtx {
  now: number;
  limit?: number;
  offset?: number;
}

/** Construye la consulta para el índice. Los filtros inválidos (tamaño ilegible, fecha incompleta) simplemente no se aplican. */
export function buildQuery(text: string, f: SearchFilters, ctx: BuildCtx): SearchQuery {
  const ops = extractOperators(text);
  const exts = [...new Set([...parseExts(f.exts), ...ops.exts])];
  const min = parseSize(f.minSize);
  const max = parseSize(f.maxSize);
  let minSize = min.ok ? min.bytes : null;
  let maxSize = max.ok ? max.bytes : null;
  if (minSize !== null && maxSize !== null && minSize > maxSize) [minSize, maxSize] = [maxSize, minSize];
  const range = dateRange(f, ctx.now);
  const folder = f.folder.trim().replace(/^"(.*)"$/, "$1").trim();
  return {
    text: ops.text,
    exact: f.exact,
    exts,
    category: f.category || null,
    kind: f.kind,
    minSize,
    maxSize,
    modified: f.dateField === "modified" && range ? range : { ...NO_RANGE },
    created: f.dateField === "created" && range ? range : { ...NO_RANGE },
    accessed: f.dateField === "accessed" && range ? range : { ...NO_RANGE },
    drives: f.drive ? [f.drive] : [],
    folder: folder || null,
    sort: f.sort,
    desc: f.desc,
    limit: ctx.limit ?? PAGE_SIZE,
    offset: ctx.offset ?? 0,
  };
}

/** Cantidad de filtros activos (sin contar el texto): para el distintivo del botón «Filtros». */
export function activeFilterCount(f: SearchFilters): number {
  let n = 0;
  if (f.exact) n++;
  if (parseExts(f.exts).length) n++;
  if (f.category) n++;
  if (f.kind !== "any") n++;
  if (parseSize(f.minSize).ok && f.minSize.trim()) n++;
  if (parseSize(f.maxSize).ok && f.maxSize.trim()) n++;
  if (dateRange(f, 0)) n++;
  if (f.drive) n++;
  if (f.folder.trim()) n++;
  return n;
}

/** ¿Hay algo que buscar? Sin texto ni filtros no se consulta (se muestra el estado del índice). */
export function hasCriteria(text: string, f: SearchFilters): boolean {
  return text.trim().length > 0 || activeFilterCount(f) > 0;
}

/** Resultado del índice como `Entry`, para reutilizar iconos y vista previa. */
export function hitToEntry(h: SearchHit): Entry {
  return { name: h.name, kind: h.isDir ? "dir" : "file", size: h.size, modified: h.modified, created: h.created, hidden: false, ext: h.ext || null, link: false };
}

/** Texto del estado del índice para la barra inferior. */
export function describeIndex(s: IndexStatus | null, fmt: (n: number) => string = (n) => String(n)): string {
  if (!s) return "Índice no disponible";
  const total = fmt(s.totalFiles + s.totalDirs);
  if (s.phase === "scanning") {
    const pct = s.progress === null ? "" : ` · ${Math.round(s.progress * 100)} %`;
    return `Indexando… ${fmt(s.scanned)} entradas vistas${pct} · ${total} en el índice`;
  }
  if (s.phase === "paused") return `Indexado en pausa · ${total} en el índice`;
  const pending = s.roots.some((r) => r.online && r.state !== "complete");
  return pending ? `Índice incompleto · ${total} entradas` : `Índice al día · ${total} entradas`;
}

/** Añade una página de resultados a la lista actual sin repetir rutas (el índice puede cambiar entre páginas mientras se indexa). */
export function appendUnique(current: SearchHit[], page: SearchHit[]): SearchHit[] {
  const seen = new Set(current.map((h) => h.path));
  const out = current.slice();
  for (const h of page) if (!seen.has(h.path)) (seen.add(h.path), out.push(h));
  return out;
}
