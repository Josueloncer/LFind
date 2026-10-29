import { kindLabel } from "./fileKinds";
import type { Entry, GroupBy } from "./fs";

export interface Group {
  id: string;
  label: string;
  /** Orden entre grupos (a igualdad, por etiqueta). */
  order: number;
}

export const GROUP_LABEL: Record<GroupBy, string> = {
  none: "(Ninguno)",
  type: "Tipo",
  modified: "Fecha de modificación",
  created: "Fecha de creación",
  size: "Tamaño",
  name: "Nombre",
};

const NONE: Group = { id: "-", label: "", order: 0 };
const KB = 1024;
const MB = KB * 1024;

const SIZE_BUCKETS: [number, string][] = [
  [0, "Vacío"],
  [10 * KB, "Diminuto (0–10 KB)"],
  [100 * KB, "Pequeño (10–100 KB)"],
  [MB, "Mediano (100 KB–1 MB)"],
  [16 * MB, "Grande (1–16 MB)"],
  [128 * MB, "Enorme (16–128 MB)"],
  [Infinity, "Gigantesco (> 128 MB)"],
];

function dateGroup(ms: number | null, now: number): Group {
  if (ms === null) return { id: "d6", label: "Sin fecha", order: 6 };
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const day = 86_400_000;
  const t = today.getTime();
  if (ms >= t) return { id: "d0", label: "Hoy", order: 0 };
  if (ms >= t - day) return { id: "d1", label: "Ayer", order: 1 };
  if (ms >= t - 6 * day) return { id: "d2", label: "Esta semana", order: 2 };
  if (ms >= t - 30 * day) return { id: "d3", label: "Este mes", order: 3 };
  if (ms >= t - 365 * day) return { id: "d4", label: "Este año", order: 4 };
  return { id: "d5", label: "Hace mucho tiempo", order: 5 };
}

export function groupOf(e: Entry, by: GroupBy, now: number): Group {
  switch (by) {
    case "type": {
      if (e.kind === "dir") return { id: "t-dir", label: "Carpetas", order: 0 };
      const l = kindLabel(e);
      return { id: `t-${l}`, label: l, order: 1 };
    }
    case "modified":
      return dateGroup(e.modified, now);
    case "created":
      return dateGroup(e.created, now);
    case "size": {
      if (e.kind === "dir") return { id: "s-dir", label: "Carpetas", order: 0 };
      const i = SIZE_BUCKETS.findIndex(([max], idx) => (idx === 0 ? e.size === 0 : e.size <= max));
      return { id: `s${i}`, label: SIZE_BUCKETS[i][1], order: 1 + i };
    }
    case "name": {
      const c = e.name.charAt(0).toLocaleUpperCase();
      if (/[0-9]/.test(c)) return { id: "n-num", label: "0–9", order: 0 };
      if (/\p{L}/u.test(c)) return { id: `n-${c}`, label: c, order: 1 };
      return { id: "n-other", label: "Otros", order: 2 };
    }
    default:
      return NONE;
  }
}
