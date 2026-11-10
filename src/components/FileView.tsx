import { MIN_ICON, fitColumns, iconCells } from "../lib/view";
import { rowDelta } from "../lib/appearance";
import { usePrefs } from "../stores/prefs";
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, DragEvent, MouseEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { kindLabel } from "../lib/fileKinds";
import { formatBytes, formatDate } from "../lib/format";
import type { Column, Entry, GroupBy, SortDir, SortKey, ViewMode } from "../lib/fs";
import { buildLayout } from "../lib/layout";
import { MarqueeSession, combineSelection, edgeScroll, hitTest, makeGeometry, namesOf, rectFrom, rowTops, sameSet } from "../lib/marquee";
import type { Rect } from "../lib/marquee";
import type { ContextPoint } from "../lib/contextMenu";
import { isThumbEligible } from "../lib/thumbKinds";
import { iconService } from "../lib/iconService";
import { thumbService } from "../lib/thumbService";
import { iconGeometry } from "../lib/view";
import { CONTENT_H, DETAILS_H, HEADER_H, LIST, TILE } from "../lib/viewGeometry";
import { EntryIcon } from "./EntryIcon";
import { ArrowDown, ArrowUp, Cloud, Link2 } from "./icons";
import { Thumb } from "./Thumb";
import { useZoomAnimation } from "../hooks/useZoomAnimation";

export const COL_ORDER: Column[] = ["modified", "created", "type", "size"];
export const COL_LABEL: Record<Column, string> = { modified: "Fecha de modificación", created: "Fecha de creación", type: "Tipo", size: "Tamaño" };
const COL_W: Record<Column, number> = { modified: 172, created: 172, type: 150, size: 96 };
const COL_SORT: Record<Column, SortKey> = { modified: "modified", created: "created", type: "kind", size: "size" };
const orderColumns = (c: Column[]) => COL_ORDER.filter((x) => c.includes(x));

type Variant = "details" | "content" | "tile" | "icon" | "small" | "list";

export interface FileViewProps {
  entries: Entry[];
  /** Carpeta que se está mostrando (para componer la ruta de cada elemento). */
  basePath: string;
  /** Mostrar miniaturas reales (solo en las vistas con iconos de tamaño suficiente). */
  thumbs: boolean;
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
  /** `ev`: clic derecho real o, desde el teclado, una posición sintética. `shiftKey` pide los verbos extendidos. */
  onContext(ev: ContextPoint, e: Entry | null): void;
  onHeaderContext(ev: MouseEvent): void;
  onZoom(dir: 1 | -1): void;
  /** Ajuste continuo del tamaño de los iconos (Ctrl+rueda). */
  onZoomBy(deltaY: number): void;
  onDragStart(ev: DragEvent, e: Entry): void;
  canDrop(target: Entry | null): boolean;
  dropEffect(ev: DragEvent, target: Entry | null): "copy" | "move";
  onDrop(ev: DragEvent, target: Entry | null): void;
  onDragEnd(): void;
  /** Nombre a mostrar (desplazar hasta él y enfocarlo): p. ej. un elemento recién creado. */
  reveal: string | null;
  onRevealed(): void;
  /** Recibe el orden real en pantalla (con agrupación), para que el visor recorra lo mismo que ve el usuario. */
  orderRef?: { current: Entry[] };
  /** Con el panel de vista previa abierto, ←/→ recorren los archivos también en las vistas de una columna. */
  previewOpen?: boolean;
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

/** Miniatura real si procede (y está lista); si no, el icono del tipo de archivo. */
function Visual({ e, size, thumbs, basePath }: { e: Entry; size: number; thumbs: boolean; basePath: string }) {
  return thumbs && isThumbEligible(e) ? <Thumb entry={e} basePath={basePath} size={size} /> : <EntryIcon entry={e} basePath={basePath} size={size} />;
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
  thumbs: boolean;
  basePath: string;
  style?: CSSProperties;
}

const Item = memo(function Item({ e, i, variant, size, sel, focus, cut, renaming, drop, cols, h, thumbs, basePath, style }: ItemProps) {
  const name = renaming ? <RenameInput entry={e} onCommit={(n) => h.rename(e.name, n)} onCancel={h.cancelRename} /> : <span className="nm" title={e.name}>{e.name}</span>;
  const cloud = e.cloud && <Cloud className="link-badge" aria-label="Solo en la nube"><title>Solo en la nube</title></Cloud>;
  const link = (e.link || e.cloud) && <>{e.link && <Link2 className="link-badge" />}{cloud}</>;
  const common = {
    className: `item item-${variant}`,
    role: "option" as const,
    "aria-selected": sel,
    "data-n": e.name,
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
          <div className="cell-name"><EntryIcon entry={e} basePath={basePath} size={22} />{name}{link}</div>
          {cols.map((c) => <div key={c} className={c === "size" ? "cell cell-num" : "cell"}>{cellValue(c, e)}</div>)}
        </div>
      );
    case "content":
      return (
        <div {...common}>
          <Visual e={e} size={40} thumbs={thumbs} basePath={basePath} />
          <div className="meta"><div className="nm-line">{name}{link}</div><div className="sub">{[kindLabel(e), e.kind === "dir" ? "" : formatBytes(e.size), formatDate(e.modified)].filter(Boolean).join("  ·  ")}</div></div>
        </div>
      );
    case "tile":
      return (
        <div {...common}>
          <Visual e={e} size={46} thumbs={thumbs} basePath={basePath} />
          <div className="meta"><div className="nm-line">{name}{link}</div><div className="sub">{kindLabel(e)}</div><div className="sub">{e.kind === "dir" ? formatDate(e.modified) : formatBytes(e.size)}</div></div>
        </div>
      );
    case "icon":
      return (
        <div {...common} title={e.name}>
          <Visual e={e} size={size} thumbs={thumbs} basePath={basePath} />
          <div className="gname" data-cloud={(cloud && !renaming) || undefined}>{!renaming && cloud}{name}</div>
        </div>
      );
    default: // small | list
      return (
        <div {...common}>
          <EntryIcon entry={e} basePath={basePath} size={20} />{name}{link}
        </div>
      );
  }
});

export function FileView(props: FileViewProps) {
  const { entries, mode, iconSize, columns, groupBy, sortKey, sortDir, selected, cut, renaming } = props;
  const P = useRef(props);
  P.current = props;

  // Cada carpeta abierta es una «época» nueva: lo que quedaba en cola de la anterior se descarta.
  // Se hace al crear el componente (antes de que sus hijos pidan nada), no en un efecto.
  useState(() => (thumbService.newEpoch(), iconService.newEpoch()));

  const root = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const [dim, setDim] = useState({ w: 800, h: 600 });
  const [focusIdx, setFocusIdx] = useState<number | null>(null);
  const [dropName, setDropName] = useState<string | null>(null);
  const anchor = useRef<number | null>(null);
  const collapse = useRef<number | null>(null);
  const typed = useRef({ buf: "", t: 0 });
  const marquee = useRef<HTMLDivElement>(null);
  const stopMarquee = useRef<(() => void) | null>(null);
  useEffect(() => () => stopMarquee.current?.(), []); // si el componente se desmonta a mitad de un arrastre

  // El tamaño del área solo importa en los modos que reparten por columnas o filas.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el || mode === "content") return;
    const upd = () => setDim((d) => (d.w === el.clientWidth && d.h === el.clientHeight ? d : { w: el.clientWidth, h: el.clientHeight }));
    upd();
    const ro = new ResizeObserver(upd);
    ro.observe(el);
    return () => ro.disconnect();
  }, [mode]);

  const fontScale = usePrefs((s) => s.appearance.fontScale);
  const geo = iconGeometry(iconSize, fontScale);
  const variant: Variant = mode === "details" ? "details" : mode === "content" ? "content" : mode === "tiles" ? "tile" : mode === "list" ? "list" : geo.inline ? "small" : "icon";
  const iconGrid = iconCells(iconSize, dim.w);
  const perRow = mode === "tiles" ? Math.max(1, Math.floor((dim.w - 16) / TILE.w)) : mode === "icons" ? iconGrid.perRow : 1;
  const rowExtra = usePrefs((s) => rowDelta(s.appearance.density));
  const itemH = mode === "details" ? DETAILS_H + rowExtra : mode === "content" ? CONTENT_H : mode === "tiles" ? TILE.h : mode === "icons" ? geo.cellH : LIST.h;
  // En un panel estrecho se retiran las últimas columnas: el nombre siempre se puede leer.
  const cols = useMemo(() => fitColumns(orderColumns(columns), (c) => COL_W[c], dim.w), [columns, dim.w]);
  const tpl = useMemo(() => `minmax(0,1fr) ${cols.map((c) => `${COL_W[c]}px`).join(" ")}`.trim(), [cols]);

  const layout = useMemo(
    () => buildLayout(entries, mode === "list" ? "none" : groupBy, perRow, itemH, HEADER_H, Date.now()),
    [entries, mode, groupBy, perRow, itemH],
  );
  const L = useRef(layout);
  L.current = layout;
  // Posiciones calculadas de esta misma distribución: el virtualizador las actualiza un fotograma más tarde y la animación mediría mal.
  const tops = useMemo(() => rowTops(layout.rows), [layout.rows]);
  const zoomAnchor = useCallback((name: string) => {
    const i = L.current.flat.findIndex((x) => x.name === name);
    return i < 0 ? null : rowTops(L.current.rows)[L.current.itemRow[i]];
  }, []);
  useZoomAnimation(scroller, iconSize, mode === "icons", zoomAnchor);
  if (props.orderRef) props.orderRef.current = layout.flat;

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
      if (performance.now() - kbMenuAt.current > 500) P.current.onContext(ev, e); // el menú por teclado ya se abrió
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
    let pending = 0;
    let frame = 0;
    const flush = () => {
      frame = 0;
      const delta = pending;
      pending = 0;
      if (delta) P.current.onZoomBy(delta);
    };
    const onWheel = (ev: WheelEvent) => {
      if (!ev.ctrlKey) return; // sin Ctrl, el navegador desplaza como siempre
      ev.preventDefault();
      const { mode: m, iconSize: size } = P.current;
      if (m === "icons" && !(size <= MIN_ICON && ev.deltaY > 0)) {
        // Continuo: como mucho un cambio por fotograma, por muchos eventos que lleguen.
        acc = 0;
        pending += ev.deltaY;
        frame ||= requestAnimationFrame(flush);
        return;
      }
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
    return () => (cancelAnimationFrame(frame), el.removeEventListener("wheel", onWheel));
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

  /** Menú contextual desde el teclado (tecla Menú o Mayús+F10): sale junto al elemento con el foco. */
  const kbMenuAt = useRef(0);
  const openKeyboardMenu = () => {
    const el = scroller.current;
    if (!el) return;
    const e = focusIdx !== null ? layout.flat[focusIdx] : null;
    const box = el.getBoundingClientRect();
    const node = e ? el.querySelector<HTMLElement>("[data-focus]") : null;
    const r = node?.getBoundingClientRect();
    // Sin elemento con el foco (o fuera de la vista): menú del espacio vacío, junto a la esquina del área.
    const inView = r && r.bottom > box.top && r.top < box.bottom;
    const x = inView ? Math.round(r.left + Math.min(24, r.width / 2)) : Math.round(box.left + 24);
    const y = inView ? Math.round(r.top + Math.min(r.height / 2, 24)) : Math.round(box.top + 24);
    kbMenuAt.current = performance.now();
    P.current.onContext({ clientX: x, clientY: y, shiftKey: false, keyboard: true }, inView ? e : null);
  };

  const onKeyDown = (ev: React.KeyboardEvent) => {
    if (ev.key === "ContextMenu" || (ev.key === "F10" && ev.shiftKey && !ev.ctrlKey && !ev.altKey)) {
      ev.preventDefault();
      return openKeyboardMenu();
    }
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
      case "ArrowRight": return grid || mode === "list" || props.previewOpen ? nav(cur + hStep) : undefined;
      case "ArrowLeft": return grid || mode === "list" || props.previewOpen ? nav(cur - hStep) : undefined;
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
      // Un espacio solo pertenece a la búsqueda por escritura si ya se está escribiendo; si no, abre el visor (lo atiende `Pane`).
      if (ev.key === " " && (now - t.t > 800 || !t.buf)) return;
      if (ev.key === " ") ev.preventDefault();
      t.buf = now - t.t > 800 ? ev.key.toLowerCase() : t.buf + ev.key.toLowerCase();
      t.t = now;
      const i = layout.flat.findIndex((x) => x.name.toLowerCase().startsWith(t.buf));
      if (i >= 0) move(i, { ctrlKey: false, metaKey: false, shiftKey: false });
    }
  };

  /** Rectángulo de selección: nace al pulsar en una zona vacía y se activa al superar el umbral de arrastre. */
  const startMarquee = (ev: React.MouseEvent<HTMLDivElement>) => {
    const el = scroller.current;
    if (!el || ev.button !== 0 || P.current.renaming) return;
    if ((ev.target as HTMLElement).closest(".item")) return; // sobre un elemento: abrir/arrastrar/seleccionar, no rectángulo
    const box0 = el.getBoundingClientRect();
    if (ev.clientX >= box0.left + el.clientWidth || ev.clientY >= box0.top + el.clientHeight) return; // barras de desplazamiento
    ev.preventDefault(); // sin selección de texto ni arrastre nativo
    stopMarquee.current?.();

    const mods = { ctrl: ev.ctrlKey || ev.metaKey, shift: ev.shiftKey };
    const base = new Set(P.current.selected);
    const sess = new MarqueeSession({ x: ev.clientX, y: ev.clientY });
    const geom = makeGeometry({ mode: P.current.mode, iconSize: P.current.iconSize, fontScale: usePrefs.getState().appearance.fontScale, width: el.clientWidth, layout: L.current, rpc });
    const anchor = { x: ev.clientX - box0.left + el.scrollLeft, y: ev.clientY - box0.top + el.scrollTop };
    let pointer = { x: ev.clientX, y: ev.clientY };
    let raf = 0;
    let last: Set<string> | null = null;

    const draw = (r: Rect) => {
      const m = marquee.current;
      if (m) m.style.cssText = `display:block;left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px`;
    };
    const tick = () => {
      raf = 0;
      const b = el.getBoundingClientRect();
      const dx = edgeScroll(pointer.x - b.left, el.clientWidth);
      const dy = edgeScroll(pointer.y - b.top, el.clientHeight);
      if (dx || dy) {
        el.scrollLeft += dx; // desplazamiento automático al arrastrar junto a un borde
        el.scrollTop += dy;
        raf = requestAnimationFrame(tick);
      }
      const cur = { x: pointer.x - b.left + el.scrollLeft, y: pointer.y - b.top + el.scrollTop };
      const rect = rectFrom(anchor, cur, { w: el.scrollWidth, h: el.scrollHeight });
      draw(rect);
      const next = combineSelection(base, namesOf(L.current.flat, hitTest(rect, geom)), mods);
      if (!last || !sameSet(last, next)) {
        last = next;
        P.current.onSelect(next);
      }
    };
    const onMove = (e: globalThis.MouseEvent) => {
      pointer = { x: e.clientX, y: e.clientY };
      if (sess.update(pointer) && !raf) raf = requestAnimationFrame(tick);
    };
    const end = (restore: boolean) => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
      cancelAnimationFrame(raf);
      if (marquee.current) marquee.current.style.display = "none";
      stopMarquee.current = null;
      if (sess.active) {
        if (restore) P.current.onSelect(base); // Esc o pérdida de foco: se deshace el arrastre
      } else if (!restore && !mods.ctrl && !mods.shift) {
        P.current.onSelect(new Set()); // clic en una zona vacía sin arrastrar: limpia la selección
      }
    };
    const onUp = () => end(false);
    const onBlur = () => end(true);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && (e.stopPropagation(), end(true));
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onBlur);
    stopMarquee.current = () => end(true);
  };

  const sortIcon = sortDir === "asc" ? <ArrowUp /> : <ArrowDown />;
  const head = (key: SortKey, label: string, cls = "") => (
    <button key={key} className={cls} aria-sort={sortKey === key ? (sortDir === "asc" ? "ascending" : "descending") : undefined} onClick={() => props.onSort(key)}>
      {label}{sortKey === key && sortIcon}
    </button>
  );

  const item = (e: Entry, i: number, style?: CSSProperties) => (
    <Item key={e.name} e={e} i={i} variant={variant} size={iconSize} sel={selected.has(e.name)} focus={focusIdx === i}
      cut={cut.has(e.name)} renaming={renaming === e.name} drop={dropName === e.name} cols={cols} h={H}
      thumbs={props.thumbs && (variant === "icon" || variant === "tile" || variant === "content")} basePath={props.basePath} style={style} />
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
        onMouseDown={startMarquee}
        onContextMenu={(ev) => (ev.preventDefault(), performance.now() - kbMenuAt.current > 500 && props.onContext(ev, null))}
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
            <div ref={marquee} className="marquee" />
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
          <div className="vs" style={{ height: tops[tops.length - 1] ?? 0 }}>
            <div ref={marquee} className="marquee" />
            {vv.getVirtualItems().map((vr) => {
              const row = layout.rows[vr.index];
              if (!row) return null;
              const pos = { height: row.h, transform: `translateY(${tops[vr.index]}px)` };
              if (row.kind === "header") return <div key={vr.index} className="ghdr" style={pos}><span>{row.label}</span><i>{row.count}</i></div>;
              const cells = [];
              for (let k = row.from; k < row.to; k++) cells.push(item(layout.flat[k], k));
              return (
                <div key={vr.index} className="irow" style={{ ...pos, gridTemplateColumns: perRow > 1 ? `repeat(${perRow}, ${mode === "tiles" ? TILE.w : iconGrid.cellW}px)` : undefined }}>
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
