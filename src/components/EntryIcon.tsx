import type { Entry } from "../lib/fs";
import { join } from "../lib/path";
import { useSystemIcon } from "../hooks/useSystemIcon";
import { usesSystemIcon } from "../lib/iconKinds";
import { usePrefs } from "../stores/prefs";
import { EntryGlyph } from "./Glyphs";

/**
 * Icono de un elemento: el glifo propio de LFind al instante (sin esperar a nada) y, en cuanto llega, el icono del
 * sistema (el del Explorador: EXE, accesos directos, carpetas personalizadas…). Si el sistema no tiene icono, o la opción
 * «Usar iconos del sistema» está desactivada, se queda el glifo. Las carpetas NO usan el icono del sistema: llevan el `FolderGlyph` de LFind. No son miniaturas: eso lo hace `Thumb` por encima.
 */
export function EntryIcon({ entry, basePath, size }: { entry: Entry; basePath: string; size: number }) {
  // Carpetas: siempre el glifo propio (ni se pide su icono al sistema: se ahorra la consulta y la decodificación).
  const enabled = usePrefs((s) => s.systemIcons) && usesSystemIcon(entry);
  const url = useSystemIcon(entry, join(basePath, entry.name), size, enabled);
  if (!url) return <EntryGlyph entry={entry} size={size} />;
  // `srcSet` declara que cada píxel del icono es un píxel de pantalla; con `object-fit: scale-down` se reduce si sobra, pero nunca se amplía.
  return <img className="sysicon" src={url} srcSet={`${url} ${window.devicePixelRatio || 1}x`} alt="" width={size} height={size} draggable={false} decoding="async" style={{ width: size, height: size }} />;
}
