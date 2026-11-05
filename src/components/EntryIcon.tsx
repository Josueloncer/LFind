import type { Entry } from "../lib/fs";
import { join } from "../lib/path";
import { useSystemIcon } from "../hooks/useSystemIcon";
import { usePrefs } from "../stores/prefs";
import { EntryGlyph } from "./Glyphs";

/**
 * Icono de un elemento: el glifo propio de LFind al instante (sin esperar a nada) y, en cuanto llega, el icono del
 * sistema (el del Explorador: EXE, accesos directos, carpetas personalizadas…). Si el sistema no tiene icono, o la opción
 * «Usar iconos del sistema» está desactivada, se queda el glifo. No son miniaturas: eso lo hace `Thumb` por encima.
 */
export function EntryIcon({ entry, basePath, size }: { entry: Entry; basePath: string; size: number }) {
  const enabled = usePrefs((s) => s.systemIcons);
  const url = useSystemIcon(entry, join(basePath, entry.name), size, enabled);
  if (!url) return <EntryGlyph entry={entry} size={size} />;
  return <img className="sysicon" src={url} alt="" width={size} height={size} draggable={false} decoding="async" style={{ width: size, height: size }} />;
}
