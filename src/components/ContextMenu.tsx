import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ComponentType } from "react";
import { useUi } from "../stores/ui";
import type { MenuItem } from "../stores/ui";
import {
  Check, ClipboardPaste, Copy, ExternalLink, FilePlus, FolderOpen, FolderPlus, Info, Link, Link2, Pencil, RotateCw, Scissors, Star, Trash2, Undo2, SquareArrowOutUpRight, Plus,
} from "./icons";

const ICONS: Record<string, ComponentType<{ className?: string }>> = {
  open: FolderOpen, copy: Copy, cut: Scissors, paste: ClipboardPaste, trash: Trash2, rename: Pencil, folder: FolderPlus, file: FilePlus,
  props: Info, path: Link, shortcut: Link2, reveal: ExternalLink, undo: Undo2, refresh: RotateCw, star: Star, tab: Plus, explorer: SquareArrowOutUpRight,
};

/** Menú emergente genérico (clic derecho, menús de «Ver», «Ordenar», «Nuevo», historial…). */
export function ContextMenu() {
  const menu = useUi((s) => s.menu);
  const close = useUi((s) => s.closeMenu);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: -9999, top: -9999 });
  const [active, setActive] = useState(-1);

  // Se coloca ya medido y recortado a la ventana para que nunca quede fuera de pantalla.
  useLayoutEffect(() => {
    if (!menu || !ref.current) return;
    const r = ref.current.getBoundingClientRect();
    setPos({ left: Math.max(8, Math.min(menu.x, window.innerWidth - r.width - 8)), top: Math.max(8, Math.min(menu.y, window.innerHeight - r.height - 8)) });
    setActive(-1);
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const run = (it: MenuItem) => {
      if (it.kind !== "item" || it.disabled) return;
      close();
      it.run();
    };
    const selectable = menu.items.map((it, i) => (it.kind === "item" && !it.disabled ? i : -1)).filter((i) => i >= 0);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") return e.preventDefault(), close();
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const at = selectable.indexOf(active);
        const next = e.key === "ArrowDown" ? (at + 1) % selectable.length : (at <= 0 ? selectable.length - 1 : at - 1);
        setActive(selectable[next] ?? -1);
      } else if (e.key === "Enter" && active >= 0) {
        e.preventDefault();
        run(menu.items[active]);
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
  }, [menu, active, close]);

  if (!menu) return null;
  return (
    <div className="menu-backdrop" onMouseDown={close} onContextMenu={(e) => (e.preventDefault(), close())}>
      <div ref={ref} className="menu" role="menu" style={pos} onMouseDown={(e) => e.stopPropagation()} onContextMenu={(e) => e.preventDefault()}>
        {menu.items.map((it, i) => {
          if (it.kind === "sep") return <div key={i} className="menu-sep" role="separator" />;
          if (it.kind === "header") return <div key={i} className="menu-head">{it.label}</div>;
          const Icon = it.icon ? ICONS[it.icon] : undefined;
          return (
            <button
              key={i}
              role="menuitem"
              className="menu-item"
              data-danger={it.danger || undefined}
              data-active={i === active || undefined}
              disabled={it.disabled}
              onMouseEnter={() => setActive(i)}
              onClick={() => {
                close();
                it.run();
              }}
            >
              <span className="menu-ico">{it.checked ? <Check /> : Icon ? <Icon /> : null}</span>
              <span className="menu-label">{it.label}</span>
              {it.shortcut && <span className="menu-key">{it.shortcut}</span>}
            </button>
          );
        })}
      </div>
    </div>
  );
}
