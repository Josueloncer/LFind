import { useState } from "react";
import type { DragEvent } from "react";
import { MIME, canDropInto, draggedPaths, dropOperation, endDrag } from "../lib/dnd";
import { isVirtual } from "../lib/path";
import { useOps } from "../stores/ops";
import { externalDragActive, externalEffect, sendExternalDrop } from "../lib/externalDrop";

/** Paths que se están arrastrando (desde nuestro propio estado o, si no, del DataTransfer). */
export function payload(ev: DragEvent): string[] | null {
  const mine = draggedPaths();
  if (mine) return mine;
  try {
    const raw = ev.dataTransfer.getData(MIME);
    return raw ? (JSON.parse(raw) as string[]) : null;
  } catch {
    return null;
  }
}

/** Inicia copia/movimiento de lo arrastrado a `dest`. Devuelve `false` si no procede. */
export function dropInto(ev: DragEvent, dest: string): boolean {
  const paths = payload(ev);
  endDrag();
  // Desde otra aplicación (Explorador, WinRAR…): las rutas las pone WebView2.
  if (!paths) return externalDragActive() && !isVirtual(dest) && sendExternalDrop(ev, dest);
  if (!canDropInto(paths, dest)) return false;
  useOps.getState().start({ kind: dropOperation(paths, dest, ev), sources: paths, dest });
  return true;
}

/**
 * Convierte cualquier elemento (pestaña, favorito, miga de pan…) en destino de arrastre.
 * `over` sirve para resaltarlo. Con `dest = null` no hace nada.
 */
export function useDropTarget(dest: string | null) {
  const [over, setOver] = useState(false);
  // «Mi equipo» no es una carpeta: no se puede soltar nada en ella.
  const bind = dest && !isVirtual(dest)
    ? {
        onDragOver(ev: DragEvent) {
          const p = draggedPaths();
          if (!p && externalDragActive()) {
            ev.preventDefault();
            ev.dataTransfer.dropEffect = externalEffect(ev);
            if (!over) setOver(true);
            return;
          }
          if (!p || !canDropInto(p, dest)) return;
          ev.preventDefault();
          ev.dataTransfer.dropEffect = dropOperation(p, dest, ev);
          if (!over) setOver(true);
        },
        onDragLeave() {
          setOver(false);
        },
        onDrop(ev: DragEvent) {
          ev.preventDefault();
          setOver(false);
          dropInto(ev, dest);
        },
      }
    : {};
  return { over, bind };
}
