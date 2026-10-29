import type { ViewMode } from "./fs";

/** Tamaños de icono (px) que recorre Ctrl+rueda en los modos de iconos. */
export const ICON_STEPS = [32, 48, 64, 80, 96, 128, 160, 192, 256] as const;

/** Nombres de los tamaños clásicos del Explorador de Windows. */
export const ICON_PRESETS = [
  { label: "Iconos extragrandes", size: 256 },
  { label: "Iconos grandes", size: 96 },
  { label: "Iconos medianos", size: 64 },
  { label: "Iconos pequeños", size: 32 },
] as const;

export const MIN_ICON = ICON_STEPS[0];
export const MAX_ICON = ICON_STEPS[ICON_STEPS.length - 1];

export interface ViewSpec {
  viewMode: ViewMode;
  iconSize: number;
}

const nearestIndex = (size: number) =>
  ICON_STEPS.reduce((best, s, i) => (Math.abs(s - size) < Math.abs(ICON_STEPS[best] - size) ? i : best), 0);

/**
 * Escalera de zoom, de menor a mayor: detalles → lista → iconos 32 … 256.
 * Ctrl+rueda arriba sube un peldaño; abajo, baja uno. «Mosaicos» y «Contenido» se sitúan
 * junto a su equivalente más cercano para que la rueda siempre tenga un siguiente paso.
 */
export const LADDER_MAX = ICON_STEPS.length + 1;

/** Posición de la vista en la escalera (0 = detalles … LADDER_MAX = iconos de 256 px). */
export function ladderPos(cur: ViewSpec): number {
  return cur.viewMode === "details" || cur.viewMode === "content" ? 0
    : cur.viewMode === "list" ? 1
    : cur.viewMode === "tiles" ? 2 + nearestIndex(64)
    : 2 + nearestIndex(cur.iconSize);
}

export function ladderAt(pos: number, keepSize: number): ViewSpec {
  const n = Math.max(0, Math.min(LADDER_MAX, Math.round(pos)));
  if (n === 0) return { viewMode: "details", iconSize: keepSize };
  if (n === 1) return { viewMode: "list", iconSize: keepSize };
  return { viewMode: "icons", iconSize: ICON_STEPS[n - 2] };
}

export function zoomStep(cur: ViewSpec, dir: 1 | -1): ViewSpec {
  return ladderAt(ladderPos(cur) + dir, cur.iconSize);
}

export interface IconGeometry {
  cellW: number;
  cellH: number;
  /** Tamaños pequeños: icono a la izquierda y nombre a la derecha. */
  inline: boolean;
}

export function iconGeometry(size: number): IconGeometry {
  if (size <= 32) return { cellW: 220, cellH: 32, inline: true };
  return { cellW: Math.max(96, size + 44), cellH: size + 62, inline: false };
}

export const VIEW_LABEL: Record<ViewMode, string> = {
  details: "Detalles",
  list: "Lista",
  tiles: "Mosaicos",
  content: "Contenido",
  icons: "Iconos",
};
