import { useRef } from "react";
import type { KeyboardEvent, PointerEvent } from "react";
import { clampSplit, setDualSplit } from "../stores/dual";
import { useNav } from "../stores/nav";
import { usePrefs } from "../stores/prefs";
import { Pane } from "./Pane";
import { ArrowLeftRight } from "./icons";

function Splitter({ area }: { area: React.RefObject<HTMLDivElement | null> }) {
  const split = usePrefs((s) => s.dualSplit);
  const swap = useNav((s) => s.swap);
  const onDown = (ev: PointerEvent<HTMLDivElement>) => {
    if (ev.button !== 0 || (ev.target as Element).closest("button") || !area.current) return;
    const box = area.current.getBoundingClientRect();
    const el = ev.currentTarget;
    el.setPointerCapture(ev.pointerId);
    const move = (e: globalThis.PointerEvent) => area.current?.style.setProperty("--split", String(clampSplit(((e.clientX - box.left) / box.width) * 1000) / 1000));
    const up = (e: globalThis.PointerEvent) => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      setDualSplit(((e.clientX - box.left) / box.width) * 1000);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") (e.preventDefault(), setDualSplit(split + (e.key === "ArrowLeft" ? -25 : 25)));
  };
  return (
    <div className="pane-split" role="separator" aria-orientation="vertical" aria-label="Divisor de paneles" aria-valuenow={Math.round(split / 10)} aria-valuemin={20} aria-valuemax={80} tabIndex={0} title="Arrastra para ajustar · doble clic para igualar" onPointerDown={onDown} onKeyDown={onKey} onDoubleClick={() => setDualSplit(500)}>
      <button className="pane-swap" title="Intercambiar paneles (Ctrl+Mayús+X)" aria-label="Intercambiar paneles" onClick={swap}><ArrowLeftRight /></button>
    </div>
  );
}

/** Área de exploración: un panel o, en la vista doble, dos independientes con un divisor. */
export function Panes() {
  const dual = useNav((s) => !!s.other);
  const side = useNav((s) => s.side);
  const split = usePrefs((s) => s.dualSplit);
  const focusSide = useNav((s) => s.focusSide);
  const area = useRef<HTMLDivElement>(null);
  return (
    <div className="panes" ref={area} data-dual={dual || undefined} data-side={side} style={dual ? ({ "--split": split / 1000 } as React.CSSProperties) : undefined}>
      <Pane side={0} />
      {dual && <Splitter area={area} />}
      {dual && <Pane side={1} />}
      {dual && (
        <div className="pane-switch" role="group" aria-label="Panel visible">
          <button aria-pressed={side === 0} onClick={() => focusSide(0)}>Panel 1</button>
          <button aria-pressed={side === 1} onClick={() => focusSide(1)}>Panel 2</button>
        </div>
      )}
    </div>
  );
}
