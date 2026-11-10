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

/**
 * Pinzar en el trackpad llega como Ctrl+rueda sintético: Chromium envía `deltaY = 100·ln(1/escala)`. Con esta ganancia el
 * tamaño sigue a los dedos (pinzar al doble duplica los iconos) en vez de avanzar 600 veces más despacio que una rueda.
 */
export const PINCH_GAIN = WHEEL_DOUBLING / (100 * Math.LN2);
/** Cambio máximo por fotograma: un evento brusco no puede dar un salto. */
export const MAX_FRAME_RATIO = 1.25;

/** Una rueda de ratón da saltos de 100 o más por punto; un trackpad, valores pequeños y casi siempre con decimales. */
export const isWheelNotch = (deltaY: number, deltaMode: number): boolean => deltaMode !== 0 || (Math.abs(deltaY) >= 50 && Number.isInteger(deltaY));

/** Unidades de zoom de un evento, en la escala de la rueda (600 duplican el tamaño). */
export function wheelUnits(deltaY: number, deltaMode: number, pinch: boolean): number {
  const px = deltaY * (deltaMode === 1 ? 33 : deltaMode === 2 ? 600 : 1);
  return pinch ? px * PINCH_GAIN : px;
}

/**
 * Tamaño exacto (con decimales) tras `units`. Se acumula sin redondear: así un gesto lento avanza en proporción a lo que se
 * mueven los dedos, no un píxel por fotograma. El redondeo solo se aplica al pintar.
 */
export function nextExactSize(exact: number, units: number): number {
  const ratio = Math.min(MAX_FRAME_RATIO, Math.max(1 / MAX_FRAME_RATIO, 2 ** (-units / WHEEL_DOUBLING)));
  return Math.max(MIN_ICON, Math.min(MAX_ICON, exact * ratio));
}

export const zoomTo = (size: number): ViewSpec => ({ viewMode: "icons", iconSize: clampIcon(size) });

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
