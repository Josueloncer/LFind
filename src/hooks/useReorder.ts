import { useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { canMovePin, insertionFromY } from "../lib/locations";

/** Distancia que hay que recorrer antes de que cuente como arrastre: por debajo es un clic normal. */
const THRESHOLD_PX = 5;
const AUTOSCROLL_EDGE = 28;
const AUTOSCROLL_STEP = 10;
const FLIP_MS = 160;

interface Options {
  /** Identifica esta lista en el DOM. */
  group: string;
  /** Claves visibles, en el orden en que se ven. */
  ids: readonly string[];
  axis?: "x" | "y";
  onMove(id: string, before: string | null): void;
  /** Lo que coincida con este selector no inicia un arrastre (botones internos, campos). */
  ignore?: string;
  /** Condición adicional para empezar. */
  accept?(ev: ReactPointerEvent): boolean;
}

/**
 * Reordenar arrastrando, con eventos de puntero: mismo comportamiento para secciones, filas, herramientas y pestañas.
 * Nada cambia hasta superar el umbral, así que un clic sigue siendo un clic. Escape o perder el foco cancelan sin mover nada.
 */
export function useReorder({ group, ids, axis = "y", onMove, ignore, accept }: Options) {
  const [drag, setDrag] = useState<{ id: string; at: number | null } | null>(null);
  const latest = useRef({ ids, onMove });
  latest.current = { ids, onMove };
  const before = useRef<Map<string, DOMRect> | null>(null);

  const elements = () => {
    const found = new Map<string, HTMLElement>();
    document.querySelectorAll<HTMLElement>(`[data-reorder-group="${group}"]`).forEach((el) => found.set(el.dataset.reorderId ?? "", el));
    return latest.current.ids.flatMap((id) => found.get(id) ?? []);
  };

  // Tras reordenar, cada elemento se desliza desde donde estaba: sin saltos.
  useLayoutEffect(() => {
    const old = before.current;
    before.current = null;
    if (!old || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    for (const el of elements()) {
      const prev = old.get(el.dataset.reorderId ?? "");
      if (!prev) continue;
      const now = el.getBoundingClientRect();
      const [dx, dy] = [prev.left - now.left, prev.top - now.top];
      if (Math.abs(dx) > 1 || Math.abs(dy) > 1) el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], { duration: FLIP_MS, easing: "cubic-bezier(0.22, 0.8, 0.24, 1)" });
    }
  }, [ids.join("\u0000")]); // eslint-disable-line react-hooks/exhaustive-deps

  const begin = (id: string) => (ev: ReactPointerEvent<HTMLElement>) => {
    if (ev.button !== 0 || (ignore && (ev.target as Element).closest(ignore)) || (accept && !accept(ev))) return;
    const handle = ev.currentTarget;
    const { pointerId, clientX: startX, clientY: startY } = ev;
    let active = false;
    let at: number | null = null;
    let ghost: HTMLElement | null = null;
    const scroller = handle.closest<HTMLElement>(".side-scroll");

    const point = (e: { clientX: number; clientY: number }) => {
      const list = elements();
      const centers = list.map((el) => {
        const r = el.getBoundingClientRect();
        return axis === "x" ? r.left + r.width / 2 : r.top + r.height / 2;
      });
      const pos = insertionFromY(centers, axis === "x" ? e.clientX : e.clientY);
      return canMovePin(latest.current.ids.length, latest.current.ids.indexOf(id), pos) ? pos : null;
    };

    const finish = () => {
      window.removeEventListener("pointermove", onMove2, true);
      window.removeEventListener("pointerup", onUp, true);
      window.removeEventListener("pointercancel", cancel, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", cancel);
      if (!active) return;
      ghost?.remove();
      delete document.body.dataset.reordering;
      try { handle.releasePointerCapture(pointerId); } catch { /* ya liberado */ }
      setDrag(null);
    };

    function cancel() {
      finish();
    }

    function onKey(e: KeyboardEvent) {
      if (e.key !== "Escape" || !active) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      finish();
    }

    function onMove2(e: PointerEvent) {
      if (e.pointerId !== pointerId) return;
      if (!active) {
        if (Math.hypot(e.clientX - startX, e.clientY - startY) < THRESHOLD_PX) return;
        active = true;
        const r = handle.getBoundingClientRect();
        ghost = handle.cloneNode(true) as HTMLElement;
        ghost.removeAttribute("data-reorder-group");
        ghost.removeAttribute("data-reorder-id");
        ghost.removeAttribute("data-movable");
        ghost.classList.add("reorder-ghost");
        ghost.setAttribute("aria-hidden", "true");
        Object.assign(ghost.style, { left: `${r.left}px`, top: `${r.top}px`, width: `${r.width}px`, height: `${r.height}px` });
        document.body.appendChild(ghost);
        document.body.dataset.reordering = group;
        try { handle.setPointerCapture(pointerId); } catch { /* el arrastre sigue funcionando sin captura */ }
      }
      e.preventDefault();
      if (ghost) ghost.style.transform = axis === "x" ? `translateX(${e.clientX - startX}px)` : `translateY(${e.clientY - startY}px)`;
      if (scroller) {
        const r = scroller.getBoundingClientRect();
        if (e.clientY < r.top + AUTOSCROLL_EDGE) scroller.scrollTop -= AUTOSCROLL_STEP;
        else if (e.clientY > r.bottom - AUTOSCROLL_EDGE) scroller.scrollTop += AUTOSCROLL_STEP;
      }
      at = point(e);
      setDrag((d) => (d && d.id === id && d.at === at ? d : { id, at }));
    }

    function onUp(e: PointerEvent) {
      if (e.pointerId !== pointerId) return;
      const moved = active;
      const dest = active ? point(e) : null;
      const snapshot = active && dest !== null ? new Map(elements().map((el) => [el.dataset.reorderId ?? "", el.getBoundingClientRect()])) : null;
      finish();
      if (!moved) return;
      // El clic que sigue a soltar no debe abrir ni activar lo que estaba debajo.
      const swallow = (c: Event) => (c.stopPropagation(), c.preventDefault());
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, true), 60);
      if (dest === null) return;
      before.current = snapshot;
      const { ids: list, onMove: commit } = latest.current;
      commit(id, dest < list.length ? list[dest] : null);
    }

    window.addEventListener("pointermove", onMove2, true);
    window.addEventListener("pointerup", onUp, true);
    window.addEventListener("pointercancel", cancel, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", cancel);
  };

  const n = ids.length;
  return {
    dragging: drag?.id ?? null,
    /** Atributos del elemento de la lista. */
    attrs(id: string) {
      const i = ids.indexOf(id);
      const at = drag?.at ?? null;
      return {
        "data-reorder-group": group,
        "data-reorder-id": id,
        "data-axis": axis,
        "data-mark": at === null ? undefined : at === i ? "before" : at === n && i === n - 1 ? "after" : undefined,
        "data-dragging": drag?.id === id ? true : undefined,
      };
    },
    /** Lo que inicia el arrastre (puede ser todo el elemento o solo su asa). */
    handle: (id: string) => ({ onPointerDown: begin(id) }),
  };
}
