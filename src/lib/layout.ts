import { groupOf } from "./group";
import type { Group } from "./group";
import type { Entry, GroupBy } from "./fs";

export type Row =
  | { kind: "header"; label: string; count: number; h: number }
  | { kind: "items"; from: number; to: number; h: number };

export interface Layout {
  /** Elementos en el orden en que se ven (agrupados). Los índices de selección/teclado son de esta lista. */
  flat: Entry[];
  rows: Row[];
  /** `itemRow[i]` = fila (de `rows`) que contiene a `flat[i]`. */
  itemRow: number[];
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/**
 * Convierte la lista ordenada en filas virtualizables: opcionalmente agrupada (con cabeceras)
 * y troceada en filas de `perRow` elementos. Mantiene el orden dentro de cada grupo.
 */
export function buildLayout(entries: Entry[], by: GroupBy, perRow: number, itemH: number, headerH: number, now: number): Layout {
  const per = Math.max(1, perRow);
  let flat = entries;
  const bounds: { label: string; count: number }[] = [];

  if (by === "none") {
    bounds.push({ label: "", count: entries.length });
  } else {
    const buckets = new Map<string, { g: Group; items: Entry[] }>();
    for (const e of entries) {
      const g = groupOf(e, by, now);
      let b = buckets.get(g.id);
      if (!b) buckets.set(g.id, (b = { g, items: [] }));
      b.items.push(e);
    }
    const sorted = [...buckets.values()].sort((a, b) => a.g.order - b.g.order || collator.compare(a.g.label, b.g.label));
    flat = sorted.flatMap((b) => b.items);
    for (const b of sorted) bounds.push({ label: b.g.label, count: b.items.length });
  }

  const rows: Row[] = [];
  const itemRow = new Array<number>(flat.length);
  let at = 0;
  for (const b of bounds) {
    if (by !== "none") rows.push({ kind: "header", label: b.label, count: b.count, h: headerH });
    for (let i = 0; i < b.count; i += per) {
      const from = at + i;
      const to = Math.min(at + b.count, from + per);
      for (let k = from; k < to; k++) itemRow[k] = rows.length;
      rows.push({ kind: "items", from, to, h: itemH });
    }
    at += b.count;
  }
  return { flat, rows, itemRow };
}
