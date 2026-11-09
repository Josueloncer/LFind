import { kindLabel } from "./fileKinds";
import type { Entry, SortDir, SortKey } from "./fs";
import { SEP, sortPermutation } from "./sortCore";
import type { SortColumns, SortRequest } from "./sortCore";

export function toColumns(list: Entry[], key: SortKey): SortColumns {
  const n = list.length;
  const names = new Array<string>(n);
  const isDir = new Uint8Array(n);
  const size = new Float64Array(n);
  const modified = new Float64Array(n);
  const created = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    const e = list[i];
    names[i] = e.name;
    isDir[i] = e.kind === "dir" ? 1 : 0;
    size[i] = e.size;
    modified[i] = e.modified ?? NaN; // sin fecha: lo resuelve el comparador
    created[i] = e.created ?? NaN;
  }
  // La etiqueta de tipo se calcula una vez por entrada, no en cada comparación.
  return { names, isDir, size, modified, created, kinds: key === "kind" ? list.map(kindLabel) : null };
}

export function applyPermutation(list: Entry[], perm: Uint32Array): Entry[] {
  const out = new Array<Entry>(perm.length);
  for (let i = 0; i < perm.length; i++) out[i] = list[perm[i]];
  return out;
}

/** Carpetas siempre primero; después, el criterio elegido (desempate por nombre). Síncrono. */
export function sortEntries(list: Entry[], key: SortKey, dir: SortDir): Entry[] {
  return applyPermutation(list, sortPermutation(toColumns(list, key), key, dir));
}

/** Prepara el mensaje para el worker (copia mínima: un string de nombres + arrays tipados transferibles). */
export function toRequest(id: number, list: Entry[], key: SortKey, dir: SortDir): SortRequest {
  const c = toColumns(list, key);
  return { id, names: c.names.join(SEP), kinds: c.kinds ? c.kinds.join(SEP) : null, isDir: c.isDir, size: c.size, modified: c.modified, created: c.created, key, dir };
}
