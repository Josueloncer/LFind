import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, DragEvent, MouseEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { kindLabel } from "../lib/fileKinds";
import { formatBytes, formatDate } from "../lib/format";
import type { Column, Entry, GroupBy, SortDir, SortKey, ViewMode } from "../lib/fs";
import { buildLayout } from "../lib/layout";
import { iconGeometry } from "../lib/view";
import { EntryGlyph } from "./Glyphs";
import { ArrowDown, ArrowUp, Link2 } from "./icons";

const HEADER_H = 38;
const DETAILS_H = 32;
const CONTENT_H = 60;
const TILE = { w: 264, h: 68 };
const LIST = { w: 232, h: 30 };

export const COL_ORDER: Column[] = ["modified", "created", "type", "size"];
export const COL_LABEL: Record<Column, string> = { modified: "Fecha de modificación", created: "Fecha de creación", type: "Tipo", size: "Tamaño" };
const COL_W: Record<Column, number> = { modified: 172, created: 172, type: 150, size: 96 };
const COL_SORT: Record<Column, SortKey> = { modified: "modified", created: "created", type: "kind", size: "size" };
const orderColumns = (c: Column[]) => COL_ORDER.filter((x) => c.includes(x));

type Variant = "details" | "content" | "tile" | "icon" | "small" | "list";

export interface FileViewProps {
  entries: Entry[];
  mode: ViewMode;
  iconSize: number;
  columns: Column[];
  groupBy: GroupBy;
  sortKey: SortKey;
  sortDir: SortDir;
  onSort(k: SortKey): void;
  selected: Set<string>;
  onSelect(next: Set<string>): void;
  cut: Set<string>;
  renaming: string | null;
  onRename(oldName: string, newName: string): void;
  onCancelRename(): void;
  onOpen(e: Entry): void;
  onContext(ev: MouseEvent, e: Entry | null): void;
  onHeaderContext(ev: MouseEvent): void;
  onZoom(dir: 1 | -1): void;
  onDragStart(ev: DragEvent, e: Entry): void;
  canDrop(target: Entry | null): boolean;
  dropEffect(ev: DragEvent, target: Entry | null): "copy" | "move";
  onDrop(ev: DragEvent, target: Entry | null): void;
  onDragEnd(): void;
  /** Nombre a mostrar (desplazar hasta él y enfocarlo): p. ej. un elemento recién creado. */
  reveal: string | null;
  onRevealed(): void;
}

interface Handlers {
  down(i: number, ev: MouseEvent): void;
  click(i: number, ev: MouseEvent): void;
  open(e: Entry): void;
  context(ev: MouseEvent, e: Entry): void;
  dragStart(ev: DragEvent, e: Entry): void;
  dragOver(ev: DragEvent, e: Entry): void;
  drop(ev: DragEvent, e: Entry): void;
  dragEnd(): void;
  rename(oldName: string, newName: string): void;
  cancelRename(): void;
}

function RenameInput({ entry, onCommit, onCancel }: { entry: Entry; onCommit(n: string): void; onCancel(): void }) {
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    const dot = entry.kind === "dir" ? -1 : entry.name.lastIndexOf(".");
    el.setSelectionRange(0, dot > 0 ? dot : entry.name.length); // como el Explorador: se selecciona sin la extensión
  }, [entry]);
  const finish = (commit: boolean) => {
    if (done.current) return;
    done.current = true;
    if (commit && ref.current) onCommit(ref.current.value);
    else onCancel();
  };
  return (
    <input
      ref={ref}
      className="rename"
      defaultValue={entry.name}
      spellCheck={false}
      draggable={false}
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finish(true);
        else if (e.key === "Escape") finish(false);
      }}
      onBlur={() => finish(true)}
    />
  );
}

function cellValue(c: Column, e: Entry): string {
  switch (c) {
    case "modified": return formatDate(e.modified);
    case "created": return formatDate(e.created);
    case "type": return kindLabel(e);
    case "size": return e.kind === "dir" ? "" : formatBytes(e.size);
  }
}

interface ItemProps {
  e: Entry;
  i: number;
  variant: Variant;
  size: number;
  sel: boolean;
  focus: boolean;
  cut: boolean;
  renaming: boolean;
  drop: boolean;
  cols: Column[];
  h: Handlers;
  style?: CSSProperties;
}

const Item = memo(function Item({ e, i, variant, size, sel, focus, cut, renaming, drop, cols, h, style }: ItemProps) {
  const name = renaming ? <RenameInput entry={e} onCommit={(n) => h.rename(e.name, n)} onCancel={h.cancelRename} /> : <span className="nm" title={e.name}>{e.name}</span>;
  const link = e.link && <Link2 className="link-badge" />;
  const common = {
    className: `item item-${variant}`,
    role: "option" as const,
    "aria-selected": sel,
    "data-sel": sel || undefined,
    "data-focus": focus || undefined,
    "data-cut": cut || undefined,
    "data-drop": drop || undefined,
    "data-hidden": e.hidden || undefined,
    draggable: !renaming,
    style,
    onMouseDown: (ev: MouseEvent) => h.down(i, ev),
    onClick: (ev: MouseEvent) => h.click(i, ev),
    onDoubleClick: () => !renaming && h.open(e),
    onContextMenu: (ev: MouseEvent) => h.context(ev, e),
    onDragStart: (ev: DragEvent) => h.dragStart(ev, e),
    onDragOver: (ev: DragEvent) => h.dragOver(ev, e),
    onDrop: (ev: DragEvent) => h.drop(ev, e),
    onDragEnd: h.dragEnd,
  };

  switch (variant) {
    case "details":
      return (
        <div {...common}>
          <div className="cell-name"><EntryGlyph entry={e} size={22} />{name}{link}</div>
          {cols.map((c) => <div key={c} className={c === "size" ? "cell cell-num" : "cell"}>{cellValue(c, e)}</div>)}
        </div>
      );
    case "content":
      return (
        <div {...common}>
          <EntryGlyph entry={e} size={40} />
          <div className="meta"><div className="nm-line">{name}{link}</div><div className="sub">{[kindLabel(e), e.kind === "dir" ? "" : formatBytes(e.size), formatDate(e.modified)].filter(Boolean).join("  ·  ")}</div></div>
        </div>
      );
    case "tile":
      return (
        <div {...common}>
          <EntryGlyph entry={e} size={46} />
          <div className="meta"><div className="nm-line">{name}{link}</div><div className="sub">{kindLabel(e)}</div><div className="sub">{e.kind === "dir" ? formatDate(e.modified) : formatBytes(e.size)}</div></div>
        </div>
      );
    case "icon":
      return (
        <div {...common} title={e.name}>
          <EntryGlyph entry={e} size={size} />
          <div className="gname">{name}</div>
        </div>
      );
    default: // small | list
      return (
        <div {...common}>
          <EntryGlyph entry={e} size={20} />{name}{link}
        </div>
      );
  }
});

export function FileView(props: FileViewProps) {
  const { entries, mode, iconSize, columns, groupBy, sortKey, sortDir, selected, cut, renaming } = props;
  const P = useRef(props);
  P.current = props;

  const root = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const [dim, setDim] = useState({ w: 800, h: 600 });
  const [focusIdx, setFocusIdx] = useState<number | null>(null);
  const [dropName, setDropName] = useState<string | null>(null);
  const anchor = useRef<number | null>(null);
  const collapse = useRef<number | null>(null);
  const typed = useRef({ buf: "", t: 0 });

  // El tamaño del área solo importa en los modos que reparten por columnas o filas.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el || mode === "details" || mode === "content") return;
    const upd = () => setDim((d) => (d.w === el.clientWidth && d.h === el.clientHeight ? d : { w: el.clientWidth, h: el.clientHeight }));
    upd();
    const ro = new ResizeObserver(upd);
    ro.observe(el);
    return () => ro.disconnect();
  }, [mode]);

  const geo = iconGeometry(iconSize);
  const variant: Variant = mode === "details" ? "details" : mode === "content" ? "content" : mode === "tiles" ? "tile" : mode === "list" ? "list" : geo.inline ? "small" : "icon";
  const perRow = mode === "tiles" ? Math.max(1, Math.floor((dim.w - 16) / TILE.w)) : mode === "icons" ? Math.max(1, Math.floor((dim.w - 16) / geo.cellW)) : 1;
  const itemH = mode === "details" ? DETAILS_H : mode === "content" ? CONTENT_H : mode === "tiles" ? TILE.h : mode === "icons" ? geo.cellH : LIST.h;
  const cols = useMemo(() => orderColumns(columns), [columns]);
  const tpl = useMemo(() => `minmax(0,1fr) ${cols.map((c) => `${COL_W[c]}px`).join(" ")}`.trim(), [cols]);

  const layout = useMemo(
    () => buildLayout(entries, mode === "list" ? "none" : groupBy, perRow, itemH, HEADER_H, Date.now()),
    [entries, mode, groupBy, perRow, itemH],
  );
  const L = useRef(layout);
  L.current = layout;

  const vv = useVirtualizer({
    count: mode === "list" ? 0 : layout.rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (i) => layout.rows[i]?.h ?? itemH,
    overscan: 6,
  });
  useEffect(() => vv.measure(), [layout, vv]);

  const rpc = Math.max(1, Math.floor((dim.h - 14) / LIST.h)); // filas por columna en el modo Lista
  const ncols = Math.ceil(layout.flat.length / rpc);
  const hv = useVirtualizer({
    horizontal: true,
    count: mode === "list" ? ncols : 0,
    getScrollElement: () => scroller.current,
    estimateSize: () => LIST.w,
    overscan: 3,
  });
  useEffect(() => hv.measure(), [ncols, mode, hv]);

  const scrollTo = useCallback((i: number) => {
    if (P.current.mode === "list") hv.scrollToIndex(Math.floor(i / rpc), { align: "auto" });
    else vv.scrollToIndex(L.current.itemRow[i], { align: "auto" });
  }, [hv, vv, rpc]);

  const pick = useCallback((i: number, ev: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }) => {
    const flat = L.current.flat;
    const name = flat[i]?.name;
    if (name === undefined) return;
    const { selected: cur, onSelect } = P.current;
    if (ev.shiftKey && anchor.current !== null) {
      const [a, b] = [Math.min(anchor.current, i), Math.max(anchor.current, i)];
      onSelect(new Set(flat.slice(a, b + 1).map((x) => x.name)));
    } else if (ev.ctrlKey || ev.metaKey) {
      const s = new Set(cur);
      s.has(name) ? s.delete(name) : s.add(name);
      onSelect(s);
      anchor.current = i;
    } else {
      onSelect(new Set([name]));
      anchor.current = i;
    }
    setFocusIdx(i);
  }, []);

  const H = useMemo<Handlers>(() => ({
    down(i, ev) {
      collapse.current = null;
      const e = L.current.flat[i];
      if (!e) return;
      const sel = P.current.selected;
      if (ev.button === 2) {
        if (!sel.has(e.name)) pick(i, ev);
        return;
      }
      if (ev.button !== 0) return;
      // Pulsar sobre un elemento ya seleccionado (con varios) no colapsa la selección hasta soltar:
      // así se puede arrastrar el grupo completo.
      if (!ev.ctrlKey && !ev.metaKey && !ev.shiftKey && sel.has(e.name) && sel.size > 1) {
        collapse.current = i;
        setFocusIdx(i);
        return;
      }
      pick(i, ev);
    },
    click(i, ev) {
      if (collapse.current === i) {
        collapse.current = null;
        pick(i, ev);
      }
    },
    open: (e) => P.current.onOpen(e),
    context(ev, e) {
      ev.preventDefault();
      ev.stopPropagation();
      P.current.onContext(ev, e);
    },
    dragStart(ev, e) {
      collapse.current = null;
      P.current.onDragStart(ev, e);
    },
    dragOver(ev, e) {
      if (e.kind !== "dir" || !P.current.canDrop(e)) return; // otros elementos: el área de fondo decide
      ev.preventDefault();
      ev.stopPropagation();
      ev.dataTransfer.dropEffect = P.current.dropEffect(ev, e);
      setDropName((n) => (n === e.name ? n : e.name));
    },
    drop(ev, e) {
      if (e.kind !== "dir" || !P.current.canDrop(e)) return;
      ev.preventDefault();
      ev.stopPropagation();
      setDropName(null);
      P.current.onDrop(ev, e);
    },
    dragEnd() {
      setDropName(null);
      P.current.onDragEnd();
    },
    rename: (o, n) => P.current.onRename(o, n),
    cancelRename: () => P.current.onCancelRename(),
  }), [pick]);

  // Ctrl + rueda: cambia el tamaño de los iconos sin interferir con el desplazamiento normal.
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    let acc = 0;
    let last = 0;
    const onWheel = (ev: WheelEvent) => {
      if (!ev.ctrlKey) return; // sin Ctrl, el navegador desplaza como siempre
      ev.preventDefault();
      const now = performance.now();
      if (now - last > 250) acc = 0;
      last = now;
      acc += ev.deltaY;
      if (Math.abs(acc) >= 50) {
        P.current.onZoom(acc < 0 ? 1 : -1);
        acc = 0;
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  // Mostrar un elemento concreto (recién creado o renombrado).
  const { reveal, onRevealed } = props;
  useEffect(() => {
    if (!reveal) return;
    const i = layout.flat.findIndex((x) => x.name === reveal);
    if (i < 0) return;
    scrollTo(i);
    setFocusIdx(i);
    anchor.current = i;
    onRevealed();
  }, [reveal, layout, scrollTo, onRevealed]);

  const move = (to: number, ev: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }) => {
    const i = Math.max(0, Math.min(layout.flat.length - 1, to));
    pick(i, ev);
    scrollTo(i);
  };

  const onKeyDown = (ev: React.KeyboardEvent) => {
    const n = layout.flat.length;
    if (!n || ev.altKey) return;
    const cur = focusIdx ?? -1;
    const grid = mode === "icons" || mode === "tiles";
    const vStep = grid ? perRow : 1;
    const hStep = mode === "list" ? rpc : 1;
    const nav = (to: number) => (ev.preventDefault(), move(to, ev));
    switch (ev.key) {
      case "ArrowDown": return nav(cur + vStep);
      case "ArrowUp": return nav(cur < 0 ? 0 : cur - vStep);
      case "ArrowRight": return grid || mode === "list" ? nav(cur + hStep) : undefined;
      case "ArrowLeft": return grid || mode === "list" ? nav(cur - hStep) : undefined;
      case "Home": return nav(0);
      case "End": return nav(n - 1);
      case "PageDown": return nav(cur + vStep * 8);
      case "PageUp": return nav(cur - vStep * 8);
      case "Enter": if (focusIdx !== null && layout.flat[focusIdx]) props.onOpen(layout.flat[focusIdx]); return;
      case "Escape": return props.onSelect(new Set());
    }
    if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "a") {
      ev.preventDefault();
      return props.onSelect(new Set(layout.flat.map((x) => x.name)));
    }
    // Escribir letras salta al primer elemento cuyo nombre empieza así.
    if (ev.key.length === 1 && !ev.ctrlKey && !ev.metaKey) {
      const t = typed.current;
      const now = performance.now();
      t.buf = now - t.t > 800 ? ev.key.toLowerCase() : t.buf + ev.key.toLowerCase();
      t.t = now;
      const i = layout.flat.findIndex((x) => x.name.toLowerCase().startsWith(t.buf));
      if (i >= 0) move(i, { ctrlKey: false, metaKey: false, shiftKey: false });
    }
  };

  const sortIcon = sortDir === "asc" ? <ArrowUp /> : <ArrowDown />;
  const head = (key: SortKey, label: string, cls = "") => (
    <button key={key} className={cls} aria-sort={sortKey === key ? (sortDir === "asc" ? "ascending" : "descending") : undefined} onClick={() => props.onSort(key)}>
      {label}{sortKey === key && sortIcon}
    </button>
  );

  const item = (e: Entry, i: number, style?: CSSProperties) => (
    <Item key={e.name} e={e} i={i} variant={variant} size={iconSize} sel={selected.has(e.name)} focus={focusIdx === i}
      cut={cut.has(e.name)} renaming={renaming === e.name} drop={dropName === e.name} cols={cols} h={H} style={style} />
  );

  return (
    <div
      ref={root}
      className="fileview"
      data-mode={mode}
      tabIndex={0}
      role="listbox"
      aria-multiselectable
      style={{ "--dcols": tpl } as CSSProperties}
      onKeyDown={onKeyDown}
      onMouseDown={(ev) => !(ev.target instanceof HTMLInputElement) && root.current?.focus({ preventScroll: true })}
    >
      {mode === "details" && (
        <div className="hdr" onContextMenu={(ev) => (ev.preventDefault(), props.onHeaderContext(ev))}>
          {head("name", "Nombre")}
          {cols.map((c) => head(COL_SORT[c], COL_LABEL[c], c === "size" ? "num" : ""))}
        </div>
      )}
      <div
        className="scroller"
        ref={scroller}
        onMouseDown={(ev) => ev.target === ev.currentTarget && props.onSelect(new Set())}
        onContextMenu={(ev) => (ev.preventDefault(), props.onContext(ev, null))}
        onDragOver={(ev) => {
          setDropName(null);
          if (!props.canDrop(null)) return;
          ev.preventDefault();
          ev.dataTransfer.dropEffect = props.dropEffect(ev, null);
        }}
        onDrop={(ev) => {
          if (!props.canDrop(null)) return;
          ev.preventDefault();
          props.onDrop(ev, null);
        }}
        onDragLeave={(ev) => !ev.currentTarget.contains(ev.relatedTarget as Node | null) && setDropName(null)}
      >
        {mode === "list" ? (
          <div className="vs" style={{ width: hv.getTotalSize(), height: "100%" }}>
            {hv.getVirtualItems().map((vc) => {
              const from = vc.index * rpc;
              return (
                <div key={vc.index} className="lcol" style={{ width: LIST.w, transform: `translateX(${vc.start}px)` }}>
                  {layout.flat.slice(from, from + rpc).map((e, k) => item(e, from + k))}
                </div>
              );
            })}
          </div>
        ) : (
          <div className="vs" style={{ height: vv.getTotalSize() }}>
            {vv.getVirtualItems().map((vr) => {
              const row = layout.rows[vr.index];
              if (!row) return null;
              const pos = { height: row.h, transform: `translateY(${vr.start}px)` };
              if (row.kind === "header") return <div key={vr.index} className="ghdr" style={pos}><span>{row.label}</span><i>{row.count}</i></div>;
              const cells = [];
              for (let k = row.from; k < row.to; k++) cells.push(item(layout.flat[k], k));
              return (
                <div key={vr.index} className="irow" style={{ ...pos, gridTemplateColumns: perRow > 1 ? `repeat(${perRow}, ${mode === "tiles" ? TILE.w : geo.cellW}px)` : undefined }}>
                  {cells}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
