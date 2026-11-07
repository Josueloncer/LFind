/** Modelo puro de la barra de dirección: migas de pan y decisión de qué hacer con lo escrito. Sin React ni Tauri. */
import type { DriveUsage } from "./fs/types";
import { join, samePath, segments } from "./path";

export interface Crumb {
  label: string;
  /** Ruta a la que lleva. `null` = «Este equipo»: no es una carpeta, abre la lista de unidades. */
  path: string | null;
}

/** «Disco local (C:)», como el Explorador: etiqueta del volumen (o el nombre por defecto de su tipo) + letra. */
export function driveLabel(letter: string, usage?: Pick<DriveUsage, "label" | "kind">): string {
  const name = usage?.label?.trim() || (usage?.kind === "removable" ? "Disco extraíble" : usage?.kind === "cdrom" ? "Unidad de DVD" : usage?.kind === "network" ? "Unidad de red" : "Disco local");
  return `${name} (${letter.toUpperCase()}:)`;
}

/**
 * Migas de una ruta. En Windows con unidad: `Este equipo › Disco local (C:) › Users › …`. En rutas de red `\\srv\share` el
 * primer segmento es `\\srv\share` (la raíz navegable), no `\\srv`. En POSIX se conservan las migas de siempre.
 */
export function crumbsFor(path: string, usage: Record<string, Pick<DriveUsage, "label" | "kind"> | undefined> = {}): Crumb[] {
  if (!path) return [];
  const segs = segments(path);
  if (/^[A-Za-z]:/.test(path)) {
    const [drive, ...rest] = segs;
    const u = usage[drive.path] ?? usage[drive.path.toUpperCase()];
    return [{ label: "Este equipo", path: null }, { label: driveLabel(drive.label[0], u), path: drive.path }, ...rest];
  }
  if (path.startsWith("\\\\") && segs.length >= 2) {
    const root = join(segs[0].path, segs[1].label);
    return [{ label: root, path: root }, ...segs.slice(2)];
  }
  return segs;
}

/** Qué hacer al confirmar con Enter, una vez que Rust ha validado la ruta. */
export type Outcome =
  | { action: "reload" }
  | { action: "navigate"; path: string; note?: string };

export function outcomeFor(current: string, r: { type: "dir"; path: string } | { type: "file"; dir: string; file: string }): Outcome {
  const target = r.type === "dir" ? r.path : r.dir;
  const note = r.type === "file" ? `«${r.file}» es un archivo: se abrió la carpeta que lo contiene.` : undefined;
  return samePath(current, target) && !note ? { action: "reload" } : { action: "navigate", path: target, note };
}
