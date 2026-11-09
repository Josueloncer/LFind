import { normalize } from "./commandSearch";
import type { RecycleItem } from "./fs";

/** Filtra por nombre o ubicación original, sin distinguir acentos ni mayúsculas. */
export function filterRecycle(items: readonly RecycleItem[], query: string): RecycleItem[] {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  if (!words.length) return [...items];
  return items.filter((it) => {
    const text = normalize(`${it.name} ${it.original}`);
    return words.every((w) => text.includes(w));
  });
}

export const recycleBytes = (items: readonly RecycleItem[]): number => items.reduce((n, it) => n + (it.size ?? 0), 0);

/** Lo eliminado más recientemente, primero. */
export const sortRecycle = (items: readonly RecycleItem[]): RecycleItem[] => [...items].sort((a, b) => b.deleted - a.deleted);

export function confirmText(kind: "purge" | "empty", count: number): { title: string; detail: string } {
  if (kind === "empty") return { title: "Vaciar la Papelera", detail: `Se eliminarán definitivamente ${count} elemento${count === 1 ? "" : "s"}. Esta acción no se puede deshacer.` };
  return { title: `Eliminar definitivamente ${count === 1 ? "este elemento" : `estos ${count} elementos`}`, detail: "Esta acción no se puede deshacer." };
}
