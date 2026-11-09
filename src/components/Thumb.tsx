import { thumbBucket, thumbKey } from "../lib/thumbKinds";
import type { Entry } from "../lib/fs";
import { join } from "../lib/path";
import { useThumb } from "../hooks/useThumb";
import { EntryIcon } from "./EntryIcon";

/**
 * Icono del archivo que se sustituye por su miniatura real cuando está lista.
 * Muestra siempre primero el icono (del sistema, o el glifo propio): la carpeta se ve al instante y las
 * miniaturas aparecen después, conforme llegan.
 */
export function Thumb({ entry, basePath, size }: { entry: Entry; basePath: string; size: number }) {
  const px = thumbBucket(size, window.devicePixelRatio || 1);
  const path = join(basePath, entry.name);
  // La fecha y el tamaño forman parte de la clave: si el archivo cambia, se vuelve a pedir.
  const { url } = useThumb(thumbKey(path, px, entry), path, px);
  if (!url) return <EntryIcon entry={entry} basePath={basePath} size={size} />;
  return (
    <span className="thumb" style={{ width: size, height: size }}>
      <img src={url} alt="" draggable={false} decoding="async" />
    </span>
  );
}
