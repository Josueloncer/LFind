import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ComponentType } from "react";
import { useUi } from "../stores/ui";
import type { MenuItem } from "../stores/ui";
import {
  AppWindow, Archive, ArrowUpDown, Check, ChevronRight, ClipboardPaste, Copy, ExternalLink, FilePlus, FolderOpen, FolderPlus, Info, LayoutGrid, Link, Link2, PackageOpen,
  Pencil, Pin, Plus, RotateCw, Scissors, Send, SquareArrowOutUpRight, SquareTerminal, Star, Trash2, Undo2,
} from "./icons";

const ICONS: Record<string, ComponentType<{ className?: string }>> = {
  open: FolderOpen, openwith: AppWindow, copy: Copy, cut: Scissors, paste: ClipboardPaste, trash: Trash2, rename: Pencil, folder: FolderPlus, file: FilePlus,
  props: Info, path: Link, shortcut: Link2, reveal: ExternalLink, undo: Undo2, refresh: RotateCw, star: Star, pin: Pin, tab: Plus, explorer: SquareArrowOutUpRight,
  terminal: SquareTerminal, zip: Archive, extract: PackageOpen, send: Send, new: FilePlus, sort: ArrowUpDown, view: LayoutGrid,
};

const selectable = (it: MenuItem | undefined): boolean => !!it && (it.kind === "item" || it.kind === "submenu") && !it.disabled;

/** Elementos del nivel `depth` según el camino de submenús abiertos (`trail`: índice activo de cada nivel). */
function levelItems(root: MenuItem[], trail: number[], depth: number): MenuItem[] {
  let items = root;
  for (let d = 0; d < depth; d++) {
    const it = items[trail[d]];
    items = it && it.kind === "submenu" ? it.items : [];
  }
  return items;
}

/** Un panel del menú. Los submenús se colocan junto a su elemento padre (a la derecha, o a la izquierda si no caben). */
function Panel(props: {
  items: MenuItem[];
  depth: number;
  trail: number[];
  anchor: DOMRect | null;
  at: { x: number; y: number };
  setTrail(t: number[]): void;
  run(it: MenuItem): void;
}) {
  const { items, depth, trail, anchor, at, setTrail, run } = props;
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: -9999, top: -9999 });
  const itemEls = useRef<(HTMLElement | null)[]>([]);
  const [childAnchor, setChildAnchor] = useState<DOMRect | null>(null);
  const active = trail[depth] ?? -1;
  const child = items[active];

  // Se coloca ya medido y recortado a la ventana para que nunca quede fuera de pantalla.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const W = window.innerWidth, H = window.innerHeight;
    if (!anchor) {
      setPos({ left: Math.max(8, Math.min(at.x, W - r.width - 8)), top: Math.max(8, Math.min(at.y, H - r.height - 8)) });
    } else {
      const right = anchor.right + 2;
      const left = right + r.width <= W - 8 ? right : Math.max(8, anchor.left - r.width - 2);
      setPos({ left, top: Math.max(8, Math.min(anchor.top - 5, H - r.height - 8)) });
    }
  }, [anchor, at.x, at.y]);

  // Posición del elemento activo (ancla de su submenú).
  useLayoutEffect(() => {
    setChildAnchor(child?.kind === "submenu" ? itemEls.current[active]?.getBoundingClientRect() ?? null : null);
  }, [active, child]);

  // Mantiene visible el elemento activo si el menú es más alto que la ventana.
  useEffect(() => {
    if (trail.length - 1 === depth) itemEls.current[active]?.scrollIntoView?.({ block: "nearest" });
  }, [active, depth, trail.length]);

  return (
    <>
      <div ref={ref} className="menu" role="menu" data-depth={depth} style={pos} onMouseDown={(e) => e.stopPropagation()} onContextMenu={(e) => e.preventDefault()}>
        {items.map((it, i) => {
          if (it.kind === "sep") return <div key={i} className="menu-sep" role="separator" />;
          if (it.kind === "header") return <div key={i} className="menu-head">{it.label}</div>;
          const Icon = it.icon ? ICONS[it.icon] : undefined;
          const sub = it.kind === "submenu";
          return (
            <button
              key={i}
              ref={(el) => { itemEls.current[i] = el; }}
              role="menuitem"
              aria-haspopup={sub || undefined}
              aria-expanded={sub ? active === i && trail.length > depth + 1 : undefined}
              className="menu-item"
              data-danger={(it.kind === "item" && it.danger) || undefined}
              data-active={i === active || undefined}
              disabled={it.disabled}
              onMouseEnter={() => !it.disabled && setTrail(sub ? [...trail.slice(0, depth), i, -1] : [...trail.slice(0, depth), i])}
              onClick={() => (sub ? setTrail([...trail.slice(0, depth), i, -1]) : run(it))}
            >
              <span className="menu-ico">{it.kind === "item" && it.checked ? <Check /> : Icon ? <Icon /> : null}</span>
              <span className="menu-label">{it.label}</span>
              {it.kind === "item" && it.shortcut && <span className="menu-key">{it.shortcut}</span>}
              {sub && <ChevronRight className="menu-sub" />}
            </button>
          );
        })}
      </div>
      {child?.kind === "submenu" && childAnchor && trail.length > depth + 1 && (
        <Panel items={child.items} depth={depth + 1} trail={trail} anchor={childAnchor} at={at} setTrail={setTrail} run={run} />
      )}
    </>
  );
}

/** Menú emergente propio de LFind (clic derecho, «Ver», «Ordenar», «Nuevo», historial…). Todo en memoria: abrirlo no consulta nada. */
export function ContextMenu() {
  const menu = useUi((s) => s.menu);
  const close = useUi((s) => s.closeMenu);
  // trail[d] = elemento activo del nivel d; un nivel extra (-1) significa «submenú abierto, ningún elemento activo aún».
  const [trail, setTrailRaw] = useState<number[]>([-1]);
  const trailRef = useRef(trail);
  const setTrail = (t: number[]) => ((trailRef.current = t), setTrailRaw(t));

  useEffect(() => setTrail([-1]), [menu]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!menu) return;
    const run = (it: MenuItem) => {
      if (it.kind !== "item" || it.disabled) return;
      close();
      it.run();
    };
    const onKey = (e: KeyboardEvent) => {
      const t = trailRef.current;
      const depth = t.length - 1;
      const items = levelItems(menu.items, t, depth);
      const at = t[depth];
      const move = (dir: 1 | -1) => {
        const idx = items.map((it, i) => (selectable(it) ? i : -1)).filter((i) => i >= 0);
        if (!idx.length) return;
        const pos = idx.indexOf(at);
        const next = pos < 0 ? (dir === 1 ? idx[0] : idx[idx.length - 1]) : idx[(pos + dir + idx.length) % idx.length];
        setTrail([...t.slice(0, depth), next]);
      };
      const openSub = () => {
        const it = items[at];
        if (it?.kind !== "submenu") return false;
        const first = it.items.findIndex(selectable);
        setTrail([...t.slice(0, depth), at, first]);
        return true;
      };
      if (e.key === "Escape") return e.preventDefault(), depth > 0 ? setTrail(t.slice(0, depth)) : close();
      if (e.key === "ArrowDown") return e.preventDefault(), move(1);
      if (e.key === "ArrowUp") return e.preventDefault(), move(-1);
      if (e.key === "ArrowRight") return e.preventDefault(), void openSub();
      if (e.key === "ArrowLeft") return e.preventDefault(), depth > 0 ? setTrail(t.slice(0, depth)) : undefined;
      if (e.key === "Home" || e.key === "End") {
        e.preventDefault();
        const idx = items.map((it, i) => (selectable(it) ? i : -1)).filter((i) => i >= 0);
        if (idx.length) setTrail([...t.slice(0, depth), e.key === "Home" ? idx[0] : idx[idx.length - 1]]);
        return;
      }
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        if (!openSub() && items[at]) run(items[at]);
      }
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
    };
  }, [menu, close]);

  if (!menu) return null;
  const run = (it: MenuItem) => {
    if (it.kind !== "item" || it.disabled) return;
    close();
    it.run();
  };
  return (
    <div className="menu-backdrop" onMouseDown={close} onContextMenu={(e) => (e.preventDefault(), close())}>
      <Panel items={menu.items} depth={0} trail={trail} anchor={null} at={{ x: menu.x, y: menu.y }} setTrail={setTrail} run={run} />
    </div>
  );
}
