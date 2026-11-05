import { categoryOf } from "../lib/fileKinds";
import type { Category } from "../lib/fileKinds";
import type { CSSProperties } from "react";
import type { Entry } from "../lib/fs";

/** Gradientes compartidos: se declaran una vez y los glifos los referencian por id. */
export function GlyphDefs() {
  return (
    <svg width="0" height="0" style={{ position: "absolute" }} aria-hidden>
      <defs>
        <linearGradient id="nv-fb" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--fold-back-a)" />
          <stop offset="1" stopColor="var(--fold-back-b)" />
        </linearGradient>
        <linearGradient id="nv-ff" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--fold-front-a)" />
          <stop offset="1" stopColor="var(--fold-front-b)" />
        </linearGradient>
        <linearGradient id="nv-page" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--page-a)" />
          <stop offset="1" stopColor="var(--page-b)" />
        </linearGradient>
        <linearGradient id="nv-mark" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="var(--mark-a)" />
          <stop offset="1" stopColor="var(--mark-b)" />
        </linearGradient>
      </defs>
    </svg>
  );
}

export function FolderGlyph({ className = "glyph", style }: { className?: string; style?: CSSProperties }) {
  return (
    <svg className={className} style={style} viewBox="0 0 64 64" aria-hidden>
      <path d="M5 15a5 5 0 0 1 5-5h13a5 5 0 0 1 3.6 1.5l2.9 3a3 3 0 0 0 2.2.9H54a5 5 0 0 1 5 5v7H5z" fill="url(#nv-fb)" />
      <path d="M5 25.5a4.5 4.5 0 0 1 4.5-4.5h45a4.5 4.5 0 0 1 4.5 4.5V49a5 5 0 0 1-5 5H10a5 5 0 0 1-5-5z" fill="url(#nv-ff)" />
      <path d="M9.5 21.6h45" stroke="var(--glyph-shine)" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}

/** Colores por categoría: salen de los tokens del tema (`--cat-*` en themes.css). */
const CAT_COLOR: Record<Category, string> = {
  folder: "var(--cat-folder)", image: "var(--cat-image)", video: "var(--cat-video)", audio: "var(--cat-audio)", archive: "var(--cat-archive)",
  code: "var(--cat-code)", doc: "var(--cat-doc)", sheet: "var(--cat-sheet)", slides: "var(--cat-slides)", pdf: "var(--cat-pdf)",
  design: "var(--cat-design)", exec: "var(--cat-exec)", other: "var(--cat-other)",
};

export function FileGlyph({ entry, className = "glyph", showExt = false, style }: { entry: Pick<Entry, "kind" | "ext">; className?: string; showExt?: boolean; style?: CSSProperties }) {
  const color = CAT_COLOR[categoryOf(entry)];
  const label = (entry.ext ?? "").slice(0, 4).toUpperCase();
  return (
    <svg className={className} style={style} viewBox="0 0 56 64" aria-hidden>
      <path d="M11 4h23l13 13v39a5 5 0 0 1-5 5H11a5 5 0 0 1-5-5V9a5 5 0 0 1 5-5z" fill="url(#nv-page)" />
      <path d="M34 4v9a4 4 0 0 0 4 4h9z" fill="var(--glyph-shade)" />
      <rect x="6" y={showExt ? 38 : 44} width="41" height={showExt ? 15 : 8} rx="3.5" fill={color} transform="translate(0 0)" />
      {showExt && label && (
        <text x="26.5" y="49.4" textAnchor="middle" fontSize="9" fontWeight="700" fill="var(--glyph-label)" fontFamily="var(--font)">{label}</text>
      )}
    </svg>
  );
}

/** Icono por tipo de archivo/carpeta al tamaño pedido (px). Con ≥ 56 px muestra la extensión. */
export function EntryGlyph({ entry, size = 22 }: { entry: Entry; size?: number }) {
  const st = { width: size, height: size };
  return entry.kind === "dir" ? <FolderGlyph style={st} /> : <FileGlyph entry={entry} showExt={size >= 56} style={st} />;
}

export function BrandMark() {
  return (
    <svg viewBox="0 0 24 24" aria-label="LFind">
      <path d="M12 1.5l2.1 8.4 8.4 2.1-8.4 2.1L12 22.5l-2.1-8.4L1.5 12l8.4-2.1z" fill="url(#nv-mark)" />
    </svg>
  );
}
