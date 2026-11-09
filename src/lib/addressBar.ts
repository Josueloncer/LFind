/** Modelo puro de la barra de dirección: migas de pan y decisión de qué hacer con lo escrito. Sin React ni Tauri. */
import type { DriveUsage } from "./fs/types";
import { COMPUTER, STORAGE, basename, isStorage, join, parseStorageRoute, samePath, segments, storageRoute } from "./path";

export interface Crumb {
  label: string;
  /** Ruta a la que lleva (`COMPUTER` es la vista «Mi equipo»). */
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
  if (path === COMPUTER) return [{ label: "Mi equipo", path: COMPUTER }];
  if (isStorage(path)) {
    const { root, at } = parseStorageRoute(path);
    const out: Crumb[] = [{ label: "Almacenamiento", path: STORAGE }];
    if (!root) return out;
    const drive = /^[A-Za-z]:[\\/]*$/.test(root) ? usage[root] ?? usage[root.toUpperCase()] : undefined;
    out.push({ label: /^[A-Za-z]:[\\/]*$/.test(root) ? driveLabel(root[0], drive) : basename(root), path: storageRoute(root) });
    let acc = "";
    for (const part of at.split("/").filter(Boolean)) {
      acc = acc ? `${acc}/${part}` : part;
      out.push({ label: part, path: storageRoute(root, acc) });
    }
    return out;
  }
  const segs = segments(path);
  if (/^[A-Za-z]:/.test(path)) {
    const [drive, ...rest] = segs;
    const u = usage[drive.path] ?? usage[drive.path.toUpperCase()];
    return [{ label: "Mi equipo", path: COMPUTER }, { label: driveLabel(drive.label[0], u), path: drive.path }, ...rest];
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
