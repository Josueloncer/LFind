// Núcleo de ordenación. Sin dependencias de React ni de Tauri: lo usan tanto
// `sort.ts` (hilo principal, listas pequeñas) como `sort.worker.ts` (listas grandes).
import type { SortDir, SortKey } from "./fs/types";

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Separador de nombres al enviarlos al worker. NUL no es válido en nombres de archivo en ningún sistema. */
export const SEP = "\u0000";

export interface SortColumns {
  names: string[];
  isDir: Uint8Array;
  size: Float64Array;
  modified: Float64Array;
  created: Float64Array;
  /** Etiqueta de tipo por entrada; solo se necesita al ordenar por tipo. */
  kinds: string[] | null;
}

export interface SortRequest {
  id: number;
  names: string; // nombres unidos con SEP: una sola cadena es mucho más barata de copiar al worker que un array
  kinds: string | null;
  isDir: Uint8Array;
  size: Float64Array;
  modified: Float64Array;
  created: Float64Array;
  key: SortKey;
  dir: SortDir;
}

/** Las fechas ordenan carpetas y archivos juntos; con nombre, tamaño o tipo las carpetas siguen delante. */
export const mixesFolders = (key: SortKey): boolean => key === "modified" || key === "created";

/** Devuelve el orden como permutación de índices, sin tocar la lista original. Desempate por nombre y, si coincide, por posición. */
export function sortPermutation(c: SortColumns, key: SortKey, dir: SortDir): Uint32Array {
  const { names, isDir, size, modified, created, kinds } = c;
  const n = names.length;
  const idx = new Array<number>(n);
  for (let i = 0; i < n; i++) idx[i] = i;
  const m = dir === "asc" ? 1 : -1;
  const mixed = mixesFolders(key);
  const byName = (a: number, b: number) => collator.compare(names[a], names[b]) || (names[a] < names[b] ? -1 : names[a] > names[b] ? 1 : a - b);

  idx.sort((a, b) => {
    if (!mixed) {
      const da = isDir[a] ? 0 : 1;
      const db = isDir[b] ? 0 : 1;
      if (da !== db) return da - db;
    }
    if (key === "modified" || key === "created") {
      // Sin fecha conocida (NaN): siempre al final, sea cual sea el sentido.
      const col = key === "modified" ? modified : created;
      const [x, y] = [col[a], col[b]];
      if (x !== x || y !== y) {
        if (x === x) return -1;
        if (y === y) return 1;
        return byName(a, b);
      }
    }
    let r = 0;
    switch (key) {
      case "modified": r = modified[a] - modified[b]; break;
      case "created": r = created[a] - created[b]; break;
      case "size": r = size[a] - size[b]; break;
      case "kind": r = kinds ? collator.compare(kinds[a], kinds[b]) : 0; break;
    }
    return r !== 0 ? r * m : byName(a, b) * (key === "name" ? m : 1);
  });
  return Uint32Array.from(idx);
}

export function handleSortRequest(r: SortRequest): Uint32Array {
  if (r.isDir.length === 0) return new Uint32Array(0);
  return sortPermutation(
    { names: r.names.split(SEP), kinds: r.kinds === null ? null : r.kinds.split(SEP), isDir: r.isDir, size: r.size, modified: r.modified, created: r.created },
    r.key,
    r.dir,
  );
}
