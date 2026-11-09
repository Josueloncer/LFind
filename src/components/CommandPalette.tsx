import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { prepare, searchCommands } from "../lib/commandSearch";
import type { Command } from "../lib/commandSearch";
import { allCommands, queryCommands, usePalette } from "../stores/palette";

/** Centro de comandos (Ctrl+K). Solo existe mientras está abierto: cerrado no ocupa nada ni calcula nada. */
export function CommandPalette() {
  const open = usePalette((s) => s.open);
  return open ? <Panel /> : null;
}

function Panel() {
  const hide = usePalette((s) => s.hide);
  const recent = usePalette((s) => s.recent);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const list = useRef<HTMLDivElement>(null);
  const before = useRef(document.activeElement as HTMLElement | null);
  const prepared = useMemo(() => prepare(allCommands()), []);
  const results = useMemo(() => [...queryCommands(query), ...searchCommands(prepared, query, recent)], [prepared, query, recent]);

  useEffect(() => () => before.current?.focus?.({ preventScroll: true }), []);
  useEffect(() => {
    list.current?.querySelector('[data-active="true"]')?.scrollIntoView({ block: "nearest" });
  }, [active, results]);

  const run = (c: Command | undefined) => {
    if (!c) return;
    hide();
    usePalette.getState().used(c.id);
    c.run();
  };
  const onKey = (e: KeyboardEvent) => {
    const n = results.length;
    if (e.key === "Escape") (e.preventDefault(), e.stopPropagation(), hide());
    else if (e.key === "ArrowDown" && n) (e.preventDefault(), setActive((a) => (a + 1) % n));
    else if (e.key === "ArrowUp" && n) (e.preventDefault(), setActive((a) => (a - 1 + n) % n));
    else if (e.key === "Home" && n && !query) (e.preventDefault(), setActive(0));
    else if (e.key === "End" && n && !query) (e.preventDefault(), setActive(n - 1));
    else if (e.key === "Enter") (e.preventDefault(), run(results[active]));
  };

  let section = "";
  return (
    <div className="cp-backdrop" onMouseDown={(e) => e.target === e.currentTarget && hide()}>
      <div className="cp" role="dialog" aria-modal="true" aria-label="Paleta de comandos">
        <input className="cp-input" autoFocus spellCheck={false} value={query} placeholder="Busca un comando, una carpeta o escribe una ruta" role="combobox" aria-expanded="true" aria-controls="cp-list" aria-activedescendant={results[active] ? `cp-${active}` : undefined} onKeyDown={onKey} onChange={(e) => (setQuery(e.target.value), setActive(0))} />
        <div id="cp-list" className="cp-list" ref={list} role="listbox" aria-label="Resultados">
          {results.map((c, i) => {
            const head = !query && c.section !== section ? c.section : null;
            section = c.section;
            return (
              <div key={c.id}>
                {head && <div className="cp-section" role="presentation">{head}</div>}
                <button id={`cp-${i}`} type="button" className="cp-row" role="option" aria-selected={i === active} data-active={i === active} tabIndex={-1} onMouseMove={() => i !== active && setActive(i)} onClick={() => run(c)}>
                  <span className="cp-title">{c.title}{c.subtitle && <span className="cp-sub">{c.subtitle}</span>}</span>
                  {c.shortcut ? <kbd className="cp-key">{c.shortcut}</kbd> : query ? <span className="cp-tag">{c.section}</span> : null}
                </button>
              </div>
            );
          })}
          {!results.length && <p className="cp-empty">Ningún comando coincide con «{query}».</p>}
        </div>
        <footer className="cp-foot"><span><kbd>↑</kbd><kbd>↓</kbd> moverse</span><span><kbd>Intro</kbd> ejecutar</span><span><kbd>Esc</kbd> cerrar</span></footer>
      </div>
    </div>
  );
}
