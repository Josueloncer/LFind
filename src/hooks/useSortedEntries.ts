import { useEffect, useMemo, useState } from "react";
import type { Entry, SortDir, SortKey } from "../lib/fs";
import { sortEntries } from "../lib/sort";
import { cancelSort, sortInWorker, SortCancelled } from "../lib/sortWorker";

/** Hasta este tamaño se ordena en el hilo principal (coste despreciable); por encima, en un Worker. */
export const SYNC_MAX = 3000;

interface Done {
  src: Entry[];
  key: SortKey;
  dir: SortDir;
  out: Entry[];
}

/**
 * Devuelve `list` ordenada. Con listas grandes la ordenación se hace en un Worker:
 * mientras tanto se devuelve la lista sin ordenar (`sorting: true`), de modo que la
 * interfaz no se congela. Si el worker falla o tarda demasiado, se ordena de forma síncrona.
 */
export function useSortedEntries(list: Entry[], key: SortKey, dir: SortDir, enabled: boolean) {
  const small = list.length <= SYNC_MAX;
  const sync = useMemo(() => (enabled && small ? sortEntries(list, key, dir) : null), [list, key, dir, enabled, small]);
  const [done, setDone] = useState<Done | null>(null);

  useEffect(() => {
    if (!enabled || small) return;
    let live = true;
    sortInWorker(list, key, dir)
      .then((out) => live && setDone({ src: list, key, dir, out }))
      .catch((err) => {
        if (!live || err instanceof SortCancelled) return;
        setDone({ src: list, key, dir, out: sortEntries(list, key, dir) });
      });
    return () => {
      live = false;
      cancelSort();
    };
  }, [list, key, dir, enabled, small]);

  if (!enabled) return { entries: list, sorting: false };
  if (sync) return { entries: sync, sorting: false };
  if (done && done.src === list && done.key === key && done.dir === dir) return { entries: done.out, sorting: false };
  return { entries: list, sorting: true };
}
