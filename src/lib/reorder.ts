// Orden personalizado de listas: lógica pura, sin React. Ver `reorder.test.ts`.

/** Ordena `items` según las claves guardadas. Lo que no estaba guardado (elementos nuevos) va después, en su orden natural. Lo guardado que ya no existe se ignora. */
export function applyOrder<T>(items: readonly T[], keyOf: (t: T) => string, order: readonly string[]): T[] {
  if (!order.length) return [...items];
  const rank = new Map<string, number>();
  order.forEach((k) => { if (!rank.has(k)) rank.set(k, rank.size); });
  const at = (t: T) => rank.get(keyOf(t)) ?? Infinity;
  return items.map((t, i) => [t, i] as const).sort((a, b) => (at(a[0]) === at(b[0]) ? a[1] - b[1] : at(a[0]) < at(b[0]) ? -1 : 1)).map((x) => x[0]);
}

/** Coloca el elemento `key` delante de `before` (`null`: al final). Si el destino no es válido, devuelve la misma lista. */
export function moveByKey<T>(list: readonly T[], keyOf: (t: T) => string, key: string, before: string | null): readonly T[] {
  if (key === before) return list;
  const from = list.findIndex((t) => keyOf(t) === key);
  if (from < 0 || (before !== null && !list.some((t) => keyOf(t) === before))) return list;
  const rest = list.filter((_, i) => i !== from);
  const at = before === null ? rest.length : rest.findIndex((t) => keyOf(t) === before);
  const out = [...rest.slice(0, at), list[from], ...rest.slice(at)];
  return out.every((t, i) => t === list[i]) ? list : out;
}

export const moveKey = (list: readonly string[], key: string, before: string | null): string[] => [...moveByKey(list, (k) => k, key, before)];
