// Cálculos puros del visor (sin React ni IPC): navegación, zoom y disposición de páginas. Ver `viewerMath.test.ts`.

/** Índice del archivo vecino, saltando lo que no sea archivo. `null` si no hay más. */
export function neighborIndex<T>(items: readonly T[], isFile: (t: T) => boolean, current: number, dir: 1 | -1): number | null {
  for (let i = current + dir; i >= 0 && i < items.length; i += dir) if (isFile(items[i])) return i;
  return null;
}

/** Posición (desde 1) del elemento actual contando solo archivos. */
export function filePosition<T>(items: readonly T[], isFile: (t: T) => boolean, current: number): { index: number; total: number } | null {
  if (current < 0 || current >= items.length || !isFile(items[current])) return null;
  let index = 0;
  let total = 0;
  items.forEach((t, i) => {
    if (isFile(t)) {
      total++;
      if (i <= current) index++;
    }
  });
  return { index, total };
}

export const MIN_ZOOM = 0.02;
export const MAX_ZOOM = 16;
export const ZOOM_STEP = 1.25;
export const clampZoom = (z: number): number => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

export interface ImageView {
  /** `true`: la escala se recalcula para que quepa; al hacer zoom pasa a `false`. */
  fit: boolean;
  scale: number;
  x: number;
  y: number;
  rotation: 0 | 90 | 180 | 270;
}

export interface Box {
  w: number;
  h: number;
}

export const rotatedBox = (img: Box, rotation: number): Box => (rotation % 180 === 0 ? img : { w: img.h, h: img.w });

/** Escala para que la imagen quepa en el área. No amplía lo pequeño: una imagen de 16 px no debe verse borrosa a pantalla completa. */
export function fitScale(img: Box, area: Box, rotation: number): number {
  const b = rotatedBox(img, rotation);
  if (b.w <= 0 || b.h <= 0 || area.w <= 0 || area.h <= 0) return 1;
  return Math.min(1, area.w / b.w, area.h / b.h);
}

/** Lleva el desplazamiento a un rango en el que la imagen no se pierde fuera del área. */
export function clampPan(v: ImageView, img: Box, area: Box): ImageView {
  const b = rotatedBox(img, v.rotation);
  const limX = Math.max(0, (b.w * v.scale - area.w) / 2);
  const limY = Math.max(0, (b.h * v.scale - area.h) / 2);
  const limit = (n: number, lim: number) => Math.min(lim, Math.max(-lim, n)) || 0;
  return { ...v, x: limit(v.x, limX), y: limit(v.y, limY) };
}

/** Zoom manteniendo fijo el punto `(px, py)` (relativo al centro del área). */
export function zoomAt(v: ImageView, nextScale: number, px: number, py: number, img: Box, area: Box): ImageView {
  const scale = clampZoom(nextScale);
  const k = scale / v.scale;
  return clampPan({ ...v, fit: false, scale, x: px - (px - v.x) * k, y: py - (py - v.y) * k }, img, area);
}

export const rotateCw = (r: ImageView["rotation"]): ImageView["rotation"] => ((r + 90) % 360) as ImageView["rotation"];

// ───────── PDF ─────────

export type PdfZoom = { mode: "width" } | { mode: "page" } | { mode: "custom"; scale: number };

export const PAGE_GAP = 12;
/** Tope de píxeles por página: limita la memoria aunque se amplíe mucho. */
export const MAX_CANVAS_PIXELS = 16_000_000;

/** Escala en CSS de una página según el modo. `pad` es el margen total alrededor. */
export function pdfScale(zoom: PdfZoom, page: Box, area: Box, pad = 24): number {
  if (zoom.mode === "custom") return zoom.scale;
  if (page.w <= 0 || page.h <= 0) return 1;
  const w = Math.max(40, area.w - pad) / page.w;
  if (zoom.mode === "width") return clampZoom(w);
  return clampZoom(Math.min(w, Math.max(40, area.h - pad) / page.h));
}

/** Multiplicador de resolución del lienzo: nitidez según la pantalla, sin pasar del tope. */
export function canvasScale(cssW: number, cssH: number, dpr: number): number {
  const want = Math.max(1, Math.min(dpr, 2));
  const pixels = cssW * cssH * want * want;
  return pixels > MAX_CANVAS_PIXELS ? want * Math.sqrt(MAX_CANVAS_PIXELS / pixels) : want;
}

/** Página (desde 1) en el centro de la zona visible, a partir de los inicios de cada página. */
export function pageAtCenter(starts: readonly number[], scrollTop: number, viewH: number): number {
  const c = scrollTop + viewH / 2;
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (starts[mid] <= c) lo = mid;
    else hi = mid - 1;
  }
  return lo + 1;
}

/** Duración legible: `1:05` o `1:02:03`. `null` si no se conoce. */
export function formatDuration(s: number): string | null {
  if (!Number.isFinite(s) || s < 0) return null;
  const t = Math.round(s);
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const ss = String(t % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** Fecha de un PDF (`D:20240131120000+01'00'`) a texto, o `null` si no se entiende. */
export function pdfDate(v: unknown): string | null {
  if (typeof v !== "string") return null;
  const m = /^D:(\d{4})(\d{2})?(\d{2})?/.exec(v);
  if (!m) return null;
  return [m[3], m[2], m[1]].filter(Boolean).join("/");
}

/** Fichas sin valores vacíos ni etiquetas repetidas (manda la primera). */
export function mergeFacts<T extends { label: string; value: string }>(...lists: T[][]): T[] {
  const seen = new Set<string>();
  const out: T[] = [];
  for (const f of lists.flat()) {
    if (f.value.trim() && !seen.has(f.label)) (seen.add(f.label), out.push(f));
  }
  return out;
}
