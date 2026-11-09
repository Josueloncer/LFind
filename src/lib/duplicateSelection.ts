import type { DupGroup } from "./fs";

export const DUP_FLAG = { hidden: 1, readOnly: 2, hardLink: 8, special: 16 } as const;

/** ¿Se puede marcar esta copia? En cada grupo tiene que quedar siempre una sin marcar. */
export function canSelect(group: Pick<DupGroup, "members">, selected: ReadonlySet<string>, path: string): boolean {
  if (selected.has(path)) return true;
  return group.members.filter((m) => !selected.has(m.path)).length > 1;
}

/** Marca todas las copias menos la más reciente de cada grupo. Es solo una ayuda: nada se elimina sin confirmarlo. */
export function selectAllButNewest(groups: readonly Pick<DupGroup, "members">[]): Set<string> {
  const out = new Set<string>();
  for (const g of groups) {
    const newest = g.members.reduce((a, b) => (b.modified > a.modified ? b : a), g.members[0]);
    for (const m of g.members) if (m !== newest && !(m.flags & DUP_FLAG.hardLink)) out.add(m.path);
  }
  return out;
}

export const selectedBytes = (groups: readonly DupGroup[], selected: ReadonlySet<string>): number =>
  groups.reduce((n, g) => n + g.members.filter((m) => selected.has(m.path)).length * g.size, 0);

export function warningsOf(flags: number): string[] {
  const out: string[] = [];
  if (flags & DUP_FLAG.hardLink) out.push("Enlace duro: eliminarlo no libera espacio");
  if (flags & DUP_FLAG.special) out.push("Ubicación especial");
  if (flags & DUP_FLAG.hidden) out.push("Oculto");
  if (flags & DUP_FLAG.readOnly) out.push("Solo lectura");
  return out;
}

export const parseExclusions = (text: string): string[] => [...new Set(text.split(/\r?\n/).map((l) => l.trim()).filter(Boolean))];
