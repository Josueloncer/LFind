import type { Entry, SortDir, SortKey } from "./fs";
import { applyPermutation, toRequest } from "./sort";

const TIMEOUT_MS = 15_000;

interface Pending {
  resolve(perm: Uint32Array): void;
  reject(err: Error): void;
}

let worker: Worker | null = null;
let seq = 0;
const pending = new Map<number, Pending>();

export class SortCancelled extends Error {}

function rejectAll(err: Error) {
  for (const p of pending.values()) p.reject(err);
  pending.clear();
}

function getWorker(): Worker {
  if (!worker) {
    const w = new Worker(new URL("./sort.worker.ts", import.meta.url), { type: "module" });
    w.onmessage = (e: MessageEvent<{ id: number; perm: Uint32Array }>) => {
      const p = pending.get(e.data.id);
      if (p) {
        pending.delete(e.data.id);
        p.resolve(e.data.perm);
      }
    };
    w.onerror = () => {
      rejectAll(new Error("sort worker error"));
      w.terminate();
      if (worker === w) worker = null;
    };
    worker = w;
  }
  return worker;
}

/** Ordena fuera del hilo de la interfaz. Rechaza si falla o tarda demasiado (el llamador cae al orden síncrono). */
export function sortInWorker(list: Entry[], key: SortKey, dir: SortDir): Promise<Entry[]> {
  return new Promise((resolve, reject) => {
    const id = ++seq;
    const w = getWorker();
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error("sort worker timeout"));
    }, TIMEOUT_MS);
    pending.set(id, {
      resolve: (perm) => (clearTimeout(timer), resolve(applyPermutation(list, perm))),
      reject: (e) => (clearTimeout(timer), reject(e)),
    });
    const req = toRequest(id, list, key, dir);
    w.postMessage(req, [req.isDir.buffer as ArrayBuffer, req.size.buffer as ArrayBuffer, req.modified.buffer as ArrayBuffer]);
  });
}

/** Descarta el trabajo en curso (p. ej. cambió la carpeta): termina el worker para no acumular cola. */
export function cancelSort() {
  if (worker && pending.size) {
    rejectAll(new SortCancelled("cancelled"));
    worker.terminate();
    worker = null;
  }
}
