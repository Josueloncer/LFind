import type { ReactNode } from "react";
import { formatBytes } from "../lib/format";
import { LADDER_MAX } from "../lib/view";
import type { EffectiveView } from "../lib/viewState";
import { Grid2x2, Table2 } from "./icons";

interface Props {
  total: number;
  shown: number;
  selected: number;
  selectedBytes: number;
  loading: boolean;
  sorting: boolean;
  filtered: boolean;
  view: EffectiveView;
  /** Posición actual en la escalera de zoom (0 = detalles … máximo = iconos extragrandes). */
  ladder: number;
  onLadder(pos: number): void;
  /** Búsqueda global: sustituye al contador de la carpeta (y se oculta el zoom, que no aplica a la lista de resultados). */
  info?: ReactNode;
}

const n = new Intl.NumberFormat();

export function StatusBar({ total, shown, selected, selectedBytes, loading, sorting, filtered, view, ladder, onLadder, info }: Props) {
  if (info) return <footer className="status">{info}</footer>;
  const left = selected
    ? `${n.format(selected)} de ${n.format(shown)} seleccionados${selectedBytes ? ` · ${formatBytes(selectedBytes)}` : ""}`
    : `${n.format(shown)} elementos${filtered ? ` (de ${n.format(total)})` : ""}`;
  return (
    <footer className="status">
      <span className="status-left">
        {left}
        {(loading || sorting) && <span className="busy"><i className="dot" />{loading ? "Leyendo carpeta…" : "Ordenando…"}</span>}
      </span>
      <span className="zoom">
        <button className="ibtn" aria-pressed={view.viewMode === "details"} title="Vista de detalles" aria-label="Vista de detalles" onClick={() => onLadder(0)}><Table2 /></button>
        <input
          type="range" min={0} max={LADDER_MAX} step={1} value={ladder}
          aria-label="Tamaño de los elementos: de detalles a iconos extragrandes"
          title="Tamaño de los elementos (Ctrl+rueda)"
          onChange={(e) => onLadder(Number(e.target.value))}
        />
        <button className="ibtn" aria-pressed={view.viewMode === "icons"} title="Iconos grandes" aria-label="Iconos grandes" onClick={() => onLadder(6)}><Grid2x2 /></button>
      </span>
    </footer>
  );
}
