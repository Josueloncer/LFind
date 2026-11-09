// Lógica pura de Recientes, accesos fijados y unidades (sin React ni IPC). Ver `locations.test.ts`.
import type { DriveUsage, Favorite, PathState, Place, RecentFolder } from "./fs/types";
import { basename, canonicalKey, isDriveRoot, isInside } from "./path";

export const MAX_RECENTS = 15;
/** Filas visibles antes de «Mostrar más»: la barra lateral no debe convertirse en una lista interminable. */
export const VISIBLE_RECENTS = 6;
/** Tiempo mínimo en una carpeta para contar como visita: lo que se atraviesa de paso no se registra. */
export const VISIT_DWELL_MS = 1500;
/** Si se entra en una subcarpeta poco después de abrir la de arriba, esta última solo era el camino: se sustituye. */
export const PASS_THROUGH_MS = 15_000;
/** Volver a la carpeta más reciente dentro de este margen no reescribe las preferencias. */
const TOUCH_EVERY_MS = 60_000;

/** Registra una visita. Devuelve la misma lista si no cambia nada (así no se escribe en disco). */
export function recordVisit(list: readonly RecentFolder[], path: string, now: number, limit = MAX_RECENTS): readonly RecentFolder[] {
  if (isDriveRoot(path)) return list;
  const key = canonicalKey(path);
  const top = list[0];
  if (top && canonicalKey(top.path) === key && now - top.at < TOUCH_EVERY_MS) return list;
  const passing = top && top.at > 0 && now - top.at < PASS_THROUGH_MS && isInside(top.path, path) && canonicalKey(top.path) !== key;
  const rest = (passing ? list.slice(1) : list).filter((r) => canonicalKey(r.path) !== key);
  return [{ path, at: now }, ...rest].slice(0, limit);
}

export const removeRecent = (list: readonly RecentFolder[], path: string): RecentFolder[] => list.filter((r) => canonicalKey(r.path) !== canonicalKey(path));

/** Quita los que se sabe que ya no existen (una unidad que no responde no cuenta). Sin cambios, la misma lista. */
export function dropMissingRecents(list: RecentFolder[], states: Record<string, PathState>): RecentFolder[] {
  const kept = list.filter((r) => states[canonicalKey(r.path)] !== "missing");
  return kept.length === list.length ? list : kept;
}

/** Recientes que aportan algo: lo que ya está fijado o es una ubicación del sistema no se repite. */
export function visibleRecents(list: readonly RecentFolder[], shown: readonly string[]): RecentFolder[] {
  const hide = new Set(shown.map(canonicalKey));
  return list.filter((r) => !hide.has(canonicalKey(r.path)));
}

export const isPinned = (pins: readonly Favorite[], path: string): boolean => pins.some((p) => canonicalKey(p.path) === canonicalKey(path));

/** Fija o quita un acceso. Rutas equivalentes (mayúsculas, barras) son el mismo acceso. */
export function togglePin(pins: readonly Favorite[], path: string, label?: string): Favorite[] {
  const key = canonicalKey(path);
  return pins.some((p) => canonicalKey(p.path) === key) ? pins.filter((p) => canonicalKey(p.path) !== key) : [...pins, { path, label: label?.trim() || basename(path) }];
}

/** Cambia el nombre visible; el de la carpeta real no se toca. Un nombre vacío no se acepta. */
export function renamePin(pins: readonly Favorite[], path: string, label: string): readonly Favorite[] {
  const clean = label.trim();
  const key = canonicalKey(path);
  if (!clean || !pins.some((p) => canonicalKey(p.path) === key && p.label !== clean)) return pins;
  return pins.map((p) => (canonicalKey(p.path) === key ? { ...p, label: clean } : p));
}

/** ¿Soltar aquí cambiaría algo? Justo antes o después de sí mismo el orden queda igual, así que no se ofrece. */
export const canMovePin = (count: number, from: number, before: number): boolean =>
  Number.isInteger(from) && Number.isInteger(before) && from >= 0 && from < count && before >= 0 && before <= count && before !== from && before !== from + 1;

/** Coloca el elemento `from` delante del que está ahora en `before` (`pins.length` = al final). */
export function movePin(pins: readonly Favorite[], from: number, before: number): readonly Favorite[] {
  if (!canMovePin(pins.length, from, before)) return pins;
  const out = [...pins];
  const [item] = out.splice(from, 1);
  out.splice(before > from ? before - 1 : before, 0, item);
  return out;
}

export const DRIVE_KIND_LABEL: Record<string, string> = { fixed: "Disco local", removable: "Unidad extraíble", network: "Unidad de red", cdrom: "Unidad óptica", ramdisk: "Disco RAM", unknown: "Unidad" };

export const driveKindLabel = (kind: string | null | undefined): string => DRIVE_KIND_LABEL[kind ?? "unknown"] ?? DRIVE_KIND_LABEL.unknown;

const DRIVE_GROUPS = [
  { title: "Discos locales", kinds: ["fixed"] },
  { title: "Unidades extraíbles", kinds: ["removable"] },
  { title: "Unidades de red", kinds: ["network"] },
  { title: "Otras unidades", kinds: ["cdrom", "ramdisk", "unknown"] },
];

/** Unidades por tipo; los grupos vacíos no aparecen. */
export function groupDrives(drives: readonly Place[], kindOf: (d: Place) => string | null | undefined = (d) => d.driveKind): { title: string; drives: Place[] }[] {
  const known = new Set(DRIVE_GROUPS.flatMap((g) => g.kinds));
  return DRIVE_GROUPS.map((g) => ({
    title: g.title,
    drives: drives.filter((d) => {
      const k = kindOf(d) ?? "unknown";
      return g.kinds.includes(known.has(k) ? k : "unknown");
    }),
  })).filter((g) => g.drives.length > 0);
}

export const MAX_CLOUD_QUERY = 128;

/** Rutas que hay que preguntar: dentro de una carpeta de OneDrive y sin un dato reciente. Fuera de OneDrive no se consulta nada. */
export function cloudCandidates(paths: readonly string[], roots: readonly string[], checkedAt: ReadonlyMap<string, number>, now: number, ttlMs: number): string[] {
  if (!roots.length) return [];
  return paths.filter((p) => now - (checkedAt.get(p) ?? -Infinity) >= ttlMs && roots.some((r) => isInside(r, p))).slice(0, MAX_CLOUD_QUERY);
}

/** Aplica una respuesta al conjunto de rutas solo-nube. Sin cambios devuelve el mismo conjunto: no se vuelve a pintar. */
export function applyCloudResults(prev: ReadonlySet<string>, paths: readonly string[], res: readonly boolean[]): ReadonlySet<string> {
  const next = new Set(prev);
  paths.forEach((p, i) => (res[i] ? next.add(p) : next.delete(p)));
  return next.size === prev.size && [...next].every((p) => prev.has(p)) ? prev : next;
}

/** Porcentaje usado, o `null` si no hay datos utilizables. */
export function usedPercent(u: Pick<DriveUsage, "total" | "used"> | undefined): number | null {
  return u && u.total > 0 ? Math.min(100, (u.used / u.total) * 100) : null;
}
