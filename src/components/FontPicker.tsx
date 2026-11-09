import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { fontChoices, fontLabel, fontStack } from "../lib/appearance";
import { fs } from "../lib/fs";

const SAMPLE = "AaBb Gg 123";
const ROW = 36;
let known: string[] = [];

/**
 * Selector de tipografía. El nombre va siempre en la fuente de la interfaz, para que se lea también en las fuentes de símbolos;
 * al lado, una muestra en la propia fuente. Solo se pintan las filas visibles: así no se cargan cientos de fuentes a la vez.
 */
export function FontPicker({ value, onChange }: { value: string; onChange(id: string): void }) {
  const [fonts, setFonts] = useState(known);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const list = useRef<HTMLDivElement>(null);
  const items = useMemo(() => fontChoices(fonts, query), [fonts, query]);
  const virtual = useVirtualizer({ count: items.length, getScrollElement: () => list.current, estimateSize: () => ROW, overscan: 4 });
  const label = fontLabel(value);

  useEffect(() => {
    if (!open || known.length || !fs.available) return;
    let live = true;
    void fs.listFonts().then((f) => {
      known = f;
      if (live) setFonts(f);
    }).catch(() => {});
    return () => void (live = false);
  }, [open]);

  const choose = (id: string) => {
    onChange(id);
    setOpen(false);
    setQuery("");
  };
  const move = (to: number) => {
    const next = Math.max(0, Math.min(items.length - 1, to));
    setActive(next);
    virtual.scrollToIndex(next);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") (e.preventDefault(), move(active + 1));
    else if (e.key === "ArrowUp") (e.preventDefault(), move(active - 1));
    else if (e.key === "Enter" && items[active]) (e.preventDefault(), choose(items[active].id));
    else if (e.key === "Escape") (e.stopPropagation(), setOpen(false));
  };

  return (
    <div className="fp">
      <button type="button" className="fp-current" aria-haspopup="listbox" aria-expanded={open} aria-label={`Fuente: ${label}`} onClick={() => setOpen((o) => !o)}>
        <span className="fp-name">{label}</span>
        <span className="fp-sample" style={{ fontFamily: fontStack(value) ?? undefined }} aria-hidden>{SAMPLE}</span>
      </button>
      {open && (
        <>
          <input className="fp-search" autoFocus value={query} spellCheck={false} placeholder="Buscar fuente" aria-label="Buscar fuente" aria-controls="fp-list" aria-activedescendant={items[active] ? `fp-${active}` : undefined} onKeyDown={onKey} onChange={(e) => (setQuery(e.target.value), setActive(0), list.current?.scrollTo(0, 0))} />
          <div id="fp-list" className="fp-list" ref={list} role="listbox" aria-label="Fuentes instaladas">
            <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
              {virtual.getVirtualItems().map((row) => {
                const f = items[row.index];
                return (
                  <button type="button" key={f.id} id={`fp-${row.index}`} role="option" className="fp-row" aria-selected={f.label === label} data-active={row.index === active || undefined} style={{ height: ROW, transform: `translateY(${row.start}px)` }} onClick={() => choose(f.id)}>
                    <span className="fp-name">{f.label}</span>
                    <span className="fp-sample" style={{ fontFamily: f.stack ?? undefined }} aria-hidden>{SAMPLE}</span>
                  </button>
                );
              })}
            </div>
            {!items.length && <p className="fp-empty">Ninguna fuente coincide.</p>}
          </div>
        </>
      )}
    </div>
  );
}
