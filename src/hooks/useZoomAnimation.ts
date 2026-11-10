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
  /** Solo si se va a animar. */
  boxes: Map<string, Box> | null;
  /** El primer elemento visible y a qué distancia estaba del borde superior, según la distribución: seguirá en el mismo sitio. */
  anchor: { name: string; offset: number } | null;
}

const reduced = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

function stop(root: HTMLElement): void {
  for (const el of root.querySelectorAll<HTMLElement>(".item[data-n]")) for (const a of el.getAnimations()) a.cancel();
}

/**
 * `rowTopOf` es el de la distribución que se ve ahora. El ancla se toma de ahí y no de `getBoundingClientRect`: durante una
 * animación el elemento está desplazado y el desplazamiento de la vista rebotaría siguiéndolo.
 */
function capture(root: HTMLElement, rowTopOf: (name: string) => number | null, animate: boolean): Snapshot {
  const items = root.querySelectorAll<HTMLElement>(".item[data-n]");
  const top = root.getBoundingClientRect().top;
  const boxes = animate && items.length <= MAX_ANIMATED ? new Map<string, Box>() : null;
  let anchor: Snapshot["anchor"] = null;
  for (const el of items) {
    const r = el.getBoundingClientRect();
    const name = el.dataset.n as string;
    boxes?.set(name, { x: r.left, y: r.top, w: r.width });
    if (!anchor && r.bottom > top) {
      const t = rowTopOf(name);
      if (t !== null) anchor = { name, offset: t - root.scrollTop };
      if (!boxes) break;
    }
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
 * Al cambiar el tamaño, mantiene a la vista lo que se estaba mirando y, si `motion` lo permite, lleva cada icono de donde
 * estaba a su nuevo sitio. `motion` es falso durante un gesto continuo (trackpad, rueda rápida): el gesto ya es el movimiento,
 * y animar cada fotograma solo relanzaría animaciones sin parar. La animación corre sobre lo ya colocado y al terminar no
 * queda ningún efecto aplicado.
 */
export function useZoomAnimation(
  scroller: RefObject<HTMLDivElement | null>,
  size: number,
  enabled: boolean,
  rowTopOf: (name: string) => number | null,
  motion: RefObject<boolean>,
): void {
  const seen = useRef(size);
  const snapshot = useRef<Snapshot | null>(null);
  const shown = useRef(rowTopOf); // la distribución que está en pantalla
  // Hay que medir antes de que React aplique el nuevo tamaño: después ya no se sabe dónde estaba cada elemento.
  if (seen.current !== size) {
    seen.current = size;
    const root = scroller.current;
    snapshot.current = enabled && root ? capture(root, shown.current, !!motion.current && !reduced()) : null;
  }
  useLayoutEffect(() => {
    shown.current = rowTopOf;
    const snap = snapshot.current;
    const root = scroller.current;
    snapshot.current = null;
    if (!snap || !root) return;
    const anchorTop = snap.anchor && rowTopOf(snap.anchor.name);
    if (snap.anchor && anchorTop !== null && anchorTop !== undefined) root.scrollTop = Math.max(0, anchorTop - snap.anchor.offset);
    if (snap.boxes) play(root, snap.boxes);
    else stop(root); // lo que quedara de un salto anterior no debe seguir moviéndose mientras el gesto manda
  });
}
