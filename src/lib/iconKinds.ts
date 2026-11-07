import type { Entry } from "./fs";

// Réplica de la clasificación de `crates/nova-core/src/icons.rs`: decide, SIN pedir nada al backend, si el icono
// de un elemento es COMPARTIDO por tipo (una sola extracción para cientos de archivos) o PROPIO del archivo.
// Si cambia allí, hay que cambiarlo aquí (una prueba de Rust y otra de TypeScript fijan las mismas claves).

/** Iconos propios de cada archivo (se leen del archivo). */
export const PER_FILE_EXTS = ["exe", "scr", "lnk", "url", "ico", "cpl", "appref-ms"];

/** Tamaños de icono que se piden: 32, 48, 64, 128 o 256. Igual que `icons::bucket`. */
export function iconBucket(px: number): number {
  if (px <= 32) return 32;
  if (px <= 48) return 48;
  if (px <= 64) return 64;
  if (px <= 128) return 128;
  return 256;
}

/** Tamaño pedido para un icono mostrado a `cssPx` (se tiene en cuenta la densidad de píxeles, como en las miniaturas). */
export function iconPx(cssPx: number, dpr = 1): number {
  return iconBucket(Math.ceil(cssPx * Math.max(1, Math.min(dpr, 2))));
}

/** Extensión «normal» en minúsculas; algo raro se trata como archivo genérico (igual que `icons::ext_of`). */
export function extOf(name: string): string {
  const i = name.lastIndexOf(".");
  if (i <= 0) return ""; // sin punto o archivo «.oculto» (Rust: `Path::extension` tampoco lo considera extensión)
  const e = name.slice(i + 1).toLowerCase();
  return e.length > 0 && e.length <= 16 && /^[a-z0-9_-]+$/.test(e) ? e : "";
}

export function typeKey(ext: string, dir: boolean, px: number): string {
  return dir ? `d-${px}` : `t-${ext || "_"}-${px}`;
}

/**
 * Clave del icono si es COMPARTIDO por tipo (solo archivos): se puede resolver sin preguntar al backend cuando ya se tiene.
 * `null` = icono propio del archivo (EXE, LNK…) o carpeta: ahí las carpetas con icono personalizado (`desktop.ini`) solo las
 * conoce el backend, que contesta con la clave genérica `d-<px>` o con una propia (ruta + fecha).
 */
export function sharedKey(e: Pick<Entry, "kind" | "name">, px: number): string | null {
  if (e.kind === "dir") return null;
  const ext = extOf(e.name);
  return PER_FILE_EXTS.includes(ext) ? null : typeKey(ext, false, px);
}

/**
 * ¿Se pide al sistema el icono de este elemento? Las CARPETAS nunca: siempre llevan el icono propio de LFind (`FolderGlyph`,
 * vectorial y coloreado por los tokens del tema), no el de carpeta de Windows. Los archivos (EXE, PDF, PSD, accesos
 * directos…) siguen usando el icono del sistema, y las miniaturas de imagen no cambian.
 */
export function usesSystemIcon(e: Pick<Entry, "kind">): boolean {
  return e.kind !== "dir";
}
