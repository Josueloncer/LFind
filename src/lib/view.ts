import type { ViewMode } from "./fs";

/** Nombres de los tamaños clásicos del Explorador de Windows. */
export const ICON_PRESETS = [
  { label: "Iconos extragrandes", size: 256 },
  { label: "Iconos grandes", size: 96 },
  { label: "Iconos medianos", size: 64 },
  { label: "Iconos pequeños", size: 32 },
] as const;

export const MIN_ICON = 32;
export const MAX_ICON = 256;
/** El tamaño que se muestra como 100 %. */
export const ICON_100 = 96;

export const iconPercent = (size: number): number => Math.round((size * 100) / ICON_100);
const clampIcon = (size: number) => Math.max(MIN_ICON, Math.min(MAX_ICON, Math.round(size)));

export interface ViewSpec {
  viewMode: ViewMode;
  iconSize: number;
}

/**
 * Posición en el control de zoom, de menor a mayor: 0 detalles, 1 lista y, de 2 a `LADDER_MAX`, los iconos de forma continua.
 * La escala de los iconos es logarítmica: cada tramo del control cambia el tamaño en la misma proporción.
 */
export const LADDER_MAX = 10;
const ICON_SPAN = LADDER_MAX - 2;
const RANGE = Math.log(MAX_ICON / MIN_ICON);

export const ladderOfIcon = (size: number): number => 2 + (ICON_SPAN * Math.log(clampIcon(size) / MIN_ICON)) / RANGE;

export function ladderPos(cur: ViewSpec): number {
  return cur.viewMode === "details" || cur.viewMode === "content" ? 0
    : cur.viewMode === "list" ? 1
    : cur.viewMode === "tiles" ? ladderOfIcon(64)
    : ladderOfIcon(cur.iconSize);
}

export function ladderAt(pos: number, keepSize: number): ViewSpec {
  const n = Math.max(0, Math.min(LADDER_MAX, pos));
  if (n < 0.5) return { viewMode: "details", iconSize: keepSize };
  if (n < 1.5) return { viewMode: "list", iconSize: keepSize };
  return { viewMode: "icons", iconSize: clampIcon(MIN_ICON * Math.exp((Math.max(0, n - 2) / ICON_SPAN) * RANGE)) };
}

const WHEEL_DOUBLING = 600; // unidades de rueda para duplicar el tamaño: un punto de la rueda (100) cambia un 12 %
const STEP_FACTOR = 2 ** (100 / WHEEL_DOUBLING);

/** Ajuste continuo con Ctrl+rueda. Siempre cambia al menos un píxel, para que un giro lento también avance. */
export function zoomBy(cur: ViewSpec, deltaY: number): ViewSpec {
  const next = cur.iconSize * 2 ** (-deltaY / WHEEL_DOUBLING);
  const moved = Math.round(next) === cur.iconSize ? cur.iconSize + Math.sign(-deltaY) : next;
  return { viewMode: "icons", iconSize: clampIcon(moved) };
}

/** Un paso entero: pasa de una vista a otra (detalles, lista, iconos) y, dentro de los iconos, equivale a un punto de la rueda. */
export function zoomStep(cur: ViewSpec, dir: 1 | -1): ViewSpec {
  if (cur.viewMode === "icons") {
    if (dir < 0 && cur.iconSize <= MIN_ICON) return { viewMode: "list", iconSize: cur.iconSize };
    return { viewMode: "icons", iconSize: clampIcon(dir > 0 ? cur.iconSize * STEP_FACTOR : cur.iconSize / STEP_FACTOR) };
  }
  if (cur.viewMode === "tiles") return dir > 0 ? { viewMode: "icons", iconSize: 64 } : { viewMode: "list", iconSize: cur.iconSize };
  const pos = cur.viewMode === "list" ? 1 : 0;
  return ladderAt(pos + dir, cur.iconSize);
}

export interface IconGeometry {
  cellW: number;
  cellH: number;
  /** Tamaños pequeños: icono a la izquierda y nombre a la derecha. */
  inline: boolean;
}

/** `fontScale`: la altura se reserva justa para dos líneas de nombre con la letra elegida. */
export function iconGeometry(size: number, fontScale = 100): IconGeometry {
  if (size <= MIN_ICON) return { cellW: 220, cellH: 26, inline: true };
  return { cellW: Math.max(76, size + 14), cellH: size + 11 + Math.ceil((31.2 * fontScale) / 100), inline: false };
}

/** Margen lateral de la lista de elementos. */
const SIDE_PAD = 16;
/** Una celda puede crecer hasta esto para repartir el espacio sobrante. */
const MAX_STRETCH = 1.3;

/** Reparte las celdas por el ancho disponible: lo que sobra se reparte entre ellas y no se queda a la derecha. */
export function iconCells(size: number, width: number): { perRow: number; cellW: number } {
  const g = iconGeometry(size);
  const free = Math.max(0, width - SIDE_PAD);
  const perRow = Math.max(1, Math.floor(free / g.cellW));
  if (g.inline) return { perRow, cellW: g.cellW };
  return { perRow, cellW: Math.min(Math.floor(g.cellW * MAX_STRETCH), Math.max(g.cellW, Math.floor(free / perRow))) };
}

export const VIEW_LABEL: Record<ViewMode, string> = {
  details: "Detalles",
  list: "Lista",
  tiles: "Mosaicos",
  content: "Contenido",
  icons: "Iconos",
};

/** Anchura mínima del nombre en la vista de detalles. */
export const NAME_MIN_W = 180;

/** Columnas que caben dejando sitio al nombre; se retiran las últimas. Sin anchura conocida, todas. */
export function fitColumns<T>(cols: readonly T[], widthOf: (c: T) => number, available: number): T[] {
  const out = [...cols];
  if (available <= 0) return out;
  while (out.length && available - out.reduce((a, c) => a + widthOf(c), 0) < NAME_MIN_W) out.pop();
  return out;
}
