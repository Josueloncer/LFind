import type { Entry } from "./fs";

export type Category =
  | "folder" | "image" | "video" | "audio" | "archive" | "code"
  | "doc" | "sheet" | "slides" | "pdf" | "design" | "exec" | "other";

const MAP: Record<Exclude<Category, "folder" | "other">, string[]> = {
  image: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "heic", "avif", "tif", "tiff", "raw"],
  video: ["mp4", "mkv", "mov", "avi", "webm", "wmv", "flv", "m4v"],
  audio: ["mp3", "wav", "flac", "ogg", "m4a", "aac", "wma", "opus"],
  archive: ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso"],
  code: ["js", "ts", "tsx", "jsx", "json", "rs", "py", "html", "css", "md", "toml", "yml", "yaml", "c", "cpp", "h", "cs", "java", "go", "sh", "ps1", "bat", "sql", "xml"],
  doc: ["doc", "docx", "txt", "rtf", "odt", "log"],
  sheet: ["xls", "xlsx", "csv", "ods", "tsv"],
  slides: ["ppt", "pptx", "odp", "key"],
  pdf: ["pdf"],
  design: ["psd", "psb", "ai", "eps", "indd", "xcf", "sketch", "fig", "cdr"],
  exec: ["exe", "msi", "dll", "lnk"],
};

const BY_EXT = new Map<string, Category>();
for (const [cat, exts] of Object.entries(MAP)) for (const e of exts) BY_EXT.set(e, cat as Category);

export function categoryOf(e: Pick<Entry, "kind" | "ext">): Category {
  if (e.kind === "dir") return "folder";
  return (e.ext && BY_EXT.get(e.ext)) || "other";
}

const LABEL: Record<Category, string> = {
  folder: "Carpeta", image: "Imagen", video: "Vídeo", audio: "Audio", archive: "Archivo comprimido",
  code: "Código", doc: "Documento", sheet: "Hoja de cálculo", slides: "Presentación", pdf: "PDF", design: "Diseño",
  exec: "Aplicación", other: "Archivo",
};

const EXT_LABEL: Record<string, string> = {
  psd: "Documento de Photoshop", psb: "Documento grande de Photoshop", ai: "Documento de Illustrator",
  eps: "PostScript encapsulado", indd: "Documento de InDesign", svg: "Imagen SVG", webp: "Imagen WebP", heic: "Imagen HEIC",
};

export function kindLabel(e: Pick<Entry, "kind" | "ext">): string {
  if (e.kind === "file" && e.ext && EXT_LABEL[e.ext]) return EXT_LABEL[e.ext];
  const c = categoryOf(e);
  if (c === "other" && e.ext) return `Archivo ${e.ext.toUpperCase()}`;
  return LABEL[c];
}
