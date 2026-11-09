import type { KeyboardEvent, MouseEvent } from "react";
import { useUi } from "../stores/ui";
import type { MenuItem } from "../stores/ui";
import { ChevronDown } from "./icons";

/**
 * Lista desplegable de LFind. Usa el menú propio en lugar del `<select>` del navegador, cuyo desplegable lo pinta Windows
 * con sus colores y no sigue los temas.
 */
export function Select<T extends string>({ value, options, onChange, label, disabled }: { value: T; options: readonly (readonly [T, string])[]; onChange(v: T): void; label?: string; disabled?: boolean }) {
  const openMenu = useUi((s) => s.openMenu);
  const shown = options.find(([v]) => v === value)?.[1] ?? "";
  const open = (e: MouseEvent<HTMLButtonElement> | KeyboardEvent<HTMLButtonElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    openMenu(r.left, r.bottom + 2, options.map(([v, l]): MenuItem => ({ kind: "item", label: l, checked: v === value, run: () => onChange(v) })));
  };
  const onKey = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const at = options.findIndex(([v]) => v === value);
    const next = options[at + (e.key === "ArrowDown" ? 1 : -1)];
    if (next) onChange(next[0]);
  };
  return (
    <button type="button" className="select" aria-haspopup="menu" aria-label={label} disabled={disabled} onClick={open} onKeyDown={onKey}>
      <span className="select-value">{shown}</span>
      <ChevronDown aria-hidden />
    </button>
  );
}
