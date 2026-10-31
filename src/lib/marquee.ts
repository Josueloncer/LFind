import type { Entry, ViewMode } from "./fs";
import type { Layout, Row } from "./layout";
import { CONTENT_H, DETAILS_H, LIST, PAD_X, TILE } from "./viewGeometry";
import { iconGeometry } from "./view";

// ───────── Selección por rectángulo (la lógica; FileView solo la conecta al ratón) ─────────

export interface Point {
  x: number;
  y: number;
}
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Movimiento mínimo (px de pantalla) para que un clic se considere arrastre. */
export const DRAG_THRESHOLD = 4;

/**
 * Una pulsación sobre una zona vacía. Hasta que el puntero se aleja `DRAG_THRESHOLD`, es un clic:
 * no aparece rectángulo ni se toca la selección (un temblor de la mano no cuenta).
 */
export class MarqueeSession {
  active = false;
  constructor(readonly startClient: Point) {}

  /** Devuelve `true` cuando ya es un arrastre. Una vez activo, sigue activo. */
  update(client: Point): boolean {
    if (!this.active && Math.hypot(client.x - this.startClient.x, client.y - this.startClient.y) >= DRAG_THRESHOLD) this.active = true;
    return this.active;
  }
}

/** Rectángulo normalizado entre dos puntos, recortado a los límites del contenido. */
export function rectFrom(a: Point, b: Point, bounds: { w: number; h: number }): Rect {
  const cl = (v: number, max: number) => Math.max(0, Math.min(max, v));
  const [ax, ay, bx, by] = [cl(a.x, bounds.w), cl(a.y, bounds.h), cl(b.x, bounds.w), cl(b.y, bounds.h)];
  return { x: Math.min(ax, bx), y: Math.min(ay, by), w: Math.abs(ax - bx), h: Math.abs(ay - by) };
}

/**
 * Resultado de seleccionar `hit` con el rectángulo, según las teclas pulsadas al empezar:
 *  - sin teclas: solo lo del rectángulo;
 *  - Mayús: se añade a lo ya seleccionado;
 *  - Ctrl: se invierte (lo del rectángulo que estaba seleccionado se deselecciona, el resto se añade).
 */
export function combineSelection(base: ReadonlySet<string>, hit: ReadonlySet<string>, mods: { ctrl: boolean; shift: boolean }): Set<string> {
  if (mods.ctrl) {
    const out = new Set(base);
    for (const n of hit) out.has(n) ? out.delete(n) : out.add(n);
    return out;
  }
  if (mods.shift) return new Set([...base, ...hit]);
  return new Set(hit);
}

export function sameSet(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a.size !== b.size) return false;
  for (const x of a) if (!b.has(x)) return false;
  return true;
}

/** Píxeles a desplazar por fotograma si el puntero está cerca de un borde (negativo = hacia arriba/izquierda). */
export function edgeScroll(pos: number, size: number, margin = 28, max = 22): number {
  if (pos < margin) return -Math.round(max * Math.min(1, (margin - pos) / margin + 0.15));
  if (pos > size - margin) return Math.round(max * Math.min(1, (pos - (size - margin)) / margin + 0.15));
  return 0;
}

// ───────── Qué elementos toca un rectángulo (sin leer el DOM: funciona con listas virtualizadas) ─────────

export interface Geometry {
  mode: ViewMode;
  /** Ancho útil del contenido. */
  width: number;
  count: number;
  rows: Row[];
  /** Posición superior de cada fila (suma de alturas). */
  tops: number[];
  /** Ancho de celda y ancho/margen reales del elemento dentro de ella (modos de cuadrícula). */
  cellW: number;
  boxW: number;
  insetX: number;
  insetY: number;
  /** Filas por columna (modo Lista). */
  rpc: number;
}

export function rowTops(rows: Row[]): number[] {
  const t = new Array<number>(rows.length);
  let y = 0;
  for (let i = 0; i < rows.length; i++) {
    t[i] = y;
    y += rows[i].h;
  }
  return t;
}

export function makeGeometry(a: { mode: ViewMode; iconSize: number; width: number; layout: Layout; rpc: number }): Geometry {
  const base = { mode: a.mode, width: a.width, count: a.layout.flat.length, rows: a.layout.rows, tops: rowTops(a.layout.rows), rpc: a.rpc };
  switch (a.mode) {
    case "tiles":
      return { ...base, cellW: TILE.w, boxW: TILE.w - 6, insetX: 3, insetY: 3 };
    case "icons": {
      const g = iconGeometry(a.iconSize);
      return g.inline ? { ...base, cellW: g.cellW, boxW: 214, insetX: 2, insetY: 2 } : { ...base, cellW: g.cellW, boxW: g.cellW - 6, insetX: 3, insetY: 4 };
    }
    case "list":
      return { ...base, cellW: LIST.w, boxW: LIST.w - 8, insetX: 2, insetY: 1 };
    default: // details | content: el elemento ocupa todo el ancho
      return { ...base, cellW: a.width, boxW: a.width - 24, insetX: 12, insetY: 2 };
  }
}

const hits = (r: Rect, x0: number, y0: number, x1: number, y1: number) => r.x < x1 && r.x + r.w > x0 && r.y < y1 && r.y + r.h > y0;

/** Índices (de `layout.flat`) de los elementos que el rectángulo toca. */
export function hitTest(r: Rect, g: Geometry): number[] {
  const out: number[] = [];
  if (!g.count || r.w < 0 || r.h < 0) return out;

  if (g.mode === "list") {
    const c0 = Math.max(0, Math.floor((r.x - PAD_X) / LIST.w));
    const c1 = Math.floor((r.x + r.w - PAD_X) / LIST.w);
    const r0 = Math.max(0, Math.floor((r.y - LIST.top) / LIST.h));
    const r1 = Math.min(g.rpc - 1, Math.floor((r.y + r.h - LIST.top) / LIST.h));
    for (let c = c0; c <= c1; c++) {
      for (let row = r0; row <= r1; row++) {
        const i = c * g.rpc + row;
        if (i >= g.count) break;
        const x0 = PAD_X + c * LIST.w + g.insetX;
        const y0 = LIST.top + row * LIST.h + g.insetY;
        if (hits(r, x0, y0, x0 + g.boxW, y0 + LIST.h - 2 * g.insetY)) out.push(i);
      }
    }
    return out;
  }

  // Búsqueda binaria de la primera fila que termina por debajo del borde superior del rectángulo.
  let lo = 0;
  let hi = g.rows.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (g.tops[mid] + g.rows[mid].h <= r.y) lo = mid + 1;
    else hi = mid;
  }
  const bottom = r.y + r.h;
  for (let i = lo; i < g.rows.length && g.tops[i] < bottom; i++) {
    const row = g.rows[i];
    if (row.kind !== "items") continue; // las cabeceras de grupo no son elementos
    for (let k = row.from; k < row.to; k++) {
      const col = k - row.from;
      const x0 = g.mode === "details" || g.mode === "content" ? g.insetX : PAD_X + col * g.cellW + g.insetX;
      if (hits(r, x0, g.tops[i] + g.insetY, x0 + g.boxW, g.tops[i] + row.h - g.insetY)) out.push(k);
    }
  }
  return out;
}

export const rowHeightOf = (mode: ViewMode): number | null => (mode === "details" ? DETAILS_H : mode === "content" ? CONTENT_H : null);
export const namesOf = (flat: Entry[], idx: number[]) => new Set(idx.map((i) => flat[i].name));
