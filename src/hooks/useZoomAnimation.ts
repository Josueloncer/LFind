import { useLayoutEffect, useRef } from "react";
import type { RefObject } from "react";

const DURATION = 170;
const EASING = "cubic-bezier(0.2, 0.8, 0.2, 1)";
/** Por encima de esto, medir cada elemento costaría más que lo que aporta la animación. */
const MAX_ANIMATED = 350;

interface Box {
  x: number;
  y: number;
  w: number;
}

interface Snapshot {
  boxes: Map<string, Box>;
  /** El primer elemento visible y a qué distancia estaba del borde superior: seguirá en el mismo sitio. */
  anchor: { name: string; offset: number } | null;
}

const reduced = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

function capture(root: HTMLElement): Snapshot | null {
  const items = root.querySelectorAll<HTMLElement>(".item[data-n]");
  if (items.length > MAX_ANIMATED) return null;
  const top = root.getBoundingClientRect().top;
  const boxes = new Map<string, Box>();
  let anchor: Snapshot["anchor"] = null;
  for (const el of items) {
    const r = el.getBoundingClientRect();
    const name = el.dataset.n as string;
    boxes.set(name, { x: r.left, y: r.top, w: r.width });
    if (!anchor && r.bottom > top) anchor = { name, offset: r.top - top };
  }
  return { boxes, anchor };
}

function play(root: HTMLElement, before: Map<string, Box>): void {
  for (const el of root.querySelectorAll<HTMLElement>(".item[data-n]")) {
    const old = before.get(el.dataset.n as string);
    if (!old) continue;
    for (const a of el.getAnimations()) a.cancel();
    const now = el.getBoundingClientRect();
    if (!now.width) continue;
    const dx = old.x - now.left;
    const dy = old.y - now.top;
    const scale = old.w / now.width;
    if (Math.abs(dx) < 0.5 && Math.abs(dy) < 0.5 && Math.abs(scale - 1) < 0.01) continue;
    // Solo `transform`: lo resuelve el compositor, sin repintar ni recalcular la distribución en cada fotograma.
    el.animate([{ transformOrigin: "0 0", transform: `translate(${dx}px, ${dy}px) scale(${scale})` }, { transformOrigin: "0 0", transform: "none" }], { duration: DURATION, easing: EASING });
  }
}

/**
 * Lleva cada icono de donde estaba a su nuevo sitio al cambiar el tamaño, y mantiene a la vista lo que se estaba mirando.
 * La animación corre sobre lo ya colocado: la interfaz sigue respondiendo y al terminar no queda ningún efecto aplicado.
 */
export function useZoomAnimation(scroller: RefObject<HTMLDivElement | null>, size: number, enabled: boolean, rowTopOf: (name: string) => number | null): void {
  const seen = useRef(size);
  const snapshot = useRef<Snapshot | null>(null);
  // Hay que medir antes de que React aplique el nuevo tamaño: después ya no se sabe dónde estaba cada elemento.
  if (seen.current !== size) {
    seen.current = size;
    snapshot.current = enabled && scroller.current ? capture(scroller.current) : null;
  }
  useLayoutEffect(() => {
    const snap = snapshot.current;
    const root = scroller.current;
    snapshot.current = null;
    if (!snap || !root) return;
    const anchorTop = snap.anchor && rowTopOf(snap.anchor.name);
    if (snap.anchor && anchorTop !== null && anchorTop !== undefined) root.scrollTop = Math.max(0, anchorTop - snap.anchor.offset);
    if (!reduced()) play(root, snap.boxes);
  }, [size]); // eslint-disable-line react-hooks/exhaustive-deps
}
