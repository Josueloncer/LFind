import { thumbBucket } from "../lib/thumbKinds";
import type { Entry } from "../lib/fs";
import { join } from "../lib/path";
import { useThumb } from "../hooks/useThumb";
import { EntryGlyph } from "./Glyphs";

/**
 * Icono del archivo que se sustituye por su miniatura real cuando está lista.
 * Muestra siempre primero el icono del tipo de archivo: la carpeta se ve al instante y las
 * miniaturas aparecen después, conforme llegan.
 */
export function Thumb({ entry, basePath, size }: { entry: Entry; basePath: string; size: number }) {
  const px = thumbBucket(size, window.devicePixelRatio || 1);
  const path = join(basePath, entry.name);
  // La fecha y el tamaño forman parte de la clave: si el archivo cambia, se vuelve a pedir.
  const { url } = useThumb(`${path}|${px}|${entry.modified ?? 0}|${entry.size}`, path, px);
  if (!url) return <EntryGlyph entry={entry} size={size} />;
  return (
    <span className="thumb" style={{ width: size, height: size }}>
      <img src={url} alt="" draggable={false} decoding="async" />
    </span>
  );
}
