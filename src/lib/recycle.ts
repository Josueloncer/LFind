import { normalize } from "./commandSearch";
import type { PurgeProgress, PurgeReport, RecycleItem } from "./fs";

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

const elements = (n: number) => `${n.toLocaleString("es")} elemento${n === 1 ? "" : "s"}`;

/** Resultado de una eliminación. `clean` es que se borró todo lo pedido: cualquier otra cosa hay que contársela al usuario. */
export function purgeSummary(r: PurgeReport): { clean: boolean; title: string; detail: string } {
  const pending = r.failed.length + r.notAttempted;
  if (!pending && !r.cancelled) return { clean: true, title: "Eliminado definitivamente", detail: `Se eliminaron ${elements(r.deleted)}.` };
  if (r.cancelled) return { clean: false, title: "Eliminación cancelada", detail: `Se eliminaron ${elements(r.deleted)}. ${pending ? `${elements(pending)} sigue${pending === 1 ? "" : "n"} en la Papelera.` : "No quedó nada por eliminar."}` };
  return { clean: false, title: "No se pudo eliminar todo", detail: `Se eliminaron ${elements(r.deleted)}. ${elements(pending)} sigue${pending === 1 ? "" : "n"} en la Papelera:` };
}

/** Avance de la barra, en elementos: una carpeta a medias cuenta la parte que lleva. */
export const progressValue = (p: PurgeProgress): number => p.done + (p.filesTotal ? Math.min(1, p.files / p.filesTotal) : 0);

export function progressDetail(p: PurgeProgress): string {
  if (!p.whole || !p.files) return p.current;
  const count = p.filesTotal ? `${p.files.toLocaleString("es")} de ${p.filesTotal.toLocaleString("es")}` : p.files.toLocaleString("es");
  return `${p.current} · ${count} archivos`;
}

export function confirmText(kind: "purge" | "empty", count: number): { title: string; detail: string } {
  if (kind === "empty") return { title: "Vaciar la Papelera", detail: `Se eliminarán definitivamente ${count} elemento${count === 1 ? "" : "s"}. Esta acción no se puede deshacer.` };
  return { title: `Eliminar definitivamente ${count === 1 ? "este elemento" : `estos ${count} elementos`}`, detail: "Esta acción no se puede deshacer." };
}
