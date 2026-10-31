import type { Entry } from "./fs";

// Extensiones para las que merece la pena PEDIR una miniatura. Deben coincidir con
// crates/nova-core/src/thumbs.rs (una prueba de Rust lo comprueba). Pedirla para otras
// extensiones solo gastaría una llamada: el backend respondería «no soportado».

/** Las genera LFind con la crate `image` (Rust puro). */
export const RASTER = ["jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "tif", "tiff", "webp", "ico"];
/** Se muestran con <img>, donde no se ejecutan scripts. */
export const SVG = ["svg"];
/** Solo las resuelve Windows, y solo si el sistema tiene un proveedor instalado (Edge/Acrobat, Photoshop/Illustrator, códecs…). */
export const SHELL = [
  "pdf", "psd", "psb", "ai", "eps", "heic", "heif", "avif", "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2", "raf", "mp4", "m4v", "mov", "mkv", "avi", "wmv",
  "webm", "mpg", "mpeg", "flv", "3gp", "mp3", "flac", "m4a", "wma", "aac", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "vsdx", "xps",
];

const ALL = new Set([...RASTER, ...SVG, ...SHELL]);

/** Archivos visibles con extensión conocida. Las carpetas y los archivos ocultos no generan miniatura. */
export function isThumbEligible(e: Pick<Entry, "kind" | "ext" | "hidden" | "size">): boolean {
  return e.kind === "file" && !e.hidden && e.size > 0 && !!e.ext && ALL.has(e.ext);
}

/** Texto/código con vista previa textual en el panel de vista previa (coincide con nova-core::preview). */
export const TEXT = [
  "txt", "md", "markdown", "log", "json", "jsonc", "xml", "yml", "yaml", "toml", "ini", "cfg", "conf", "csv", "tsv", "js", "mjs", "ts", "tsx", "jsx", "css",
  "scss", "html", "htm", "rs", "py", "java", "c", "h", "cpp", "hpp", "cs", "go", "sh", "ps1", "bat", "cmd", "sql", "lua", "rb", "php", "kt", "swift", "vue",
  "svelte", "gitignore", "editorconfig", "lock",
];
export const isTextPreviewable = (e: Pick<Entry, "kind" | "ext" | "name">) =>
  e.kind === "file" && (TEXT.includes(e.ext ?? "") || TEXT.includes(e.name.replace(/^\./, "").toLowerCase()));

/**
 * Tamaño de miniatura que se pide al backend para un icono mostrado a `cssPx` píxeles.
 * Se redondea hacia arriba a un escalón para reutilizar la caché entre tamaños parecidos y
 * nunca se piden imágenes enormes para iconos pequeños.
 */
export function thumbBucket(cssPx: number, dpr = 1): number {
  const need = Math.ceil(cssPx * Math.max(1, Math.min(dpr, 2)));
  return [64, 128, 256, 512].find((b) => b >= need) ?? 512;
}
