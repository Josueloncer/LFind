import { isInside, parent, rootOf, samePath } from "./path";

export const MIME = "application/x-nova-paths";

/**
 * Durante `dragover` el navegador no permite leer los datos (solo los tipos), así que se guarda
 * aquí lo que se está arrastrando para poder decidir mover/copiar y si se puede soltar.
 */
let current: string[] | null = null;

export function beginDrag(ev: { dataTransfer: DataTransfer | null }, paths: string[]) {
  current = paths;
  if (!ev.dataTransfer) return;
  ev.dataTransfer.setData(MIME, JSON.stringify(paths));
  ev.dataTransfer.setData("text/plain", paths.join("\n"));
  ev.dataTransfer.effectAllowed = "copyMove";
}

export const endDrag = () => {
  current = null;
};
export const draggedPaths = () => current;

/** ¿Se puede soltar `paths` en la carpeta `dest`? (no sobre sí mismos, ni dentro de sí mismos, ni en su carpeta actual) */
export function canDropInto(paths: string[], dest: string): boolean {
  if (!paths.length) return false;
  return paths.every((p) => !isInside(p, dest) && !(parent(p) && samePath(parent(p) as string, dest)));
}

/** Como Windows: Ctrl copia, Mayús mueve; sin teclas, mover dentro de la misma unidad y copiar entre unidades. */
export function dropOperation(paths: string[], dest: string, mod: { ctrlKey: boolean; shiftKey: boolean }): "copy" | "move" {
  if (mod.ctrlKey) return "copy";
  if (mod.shiftKey) return "move";
  return paths.every((p) => rootOf(p) === rootOf(dest)) ? "move" : "copy";
}
