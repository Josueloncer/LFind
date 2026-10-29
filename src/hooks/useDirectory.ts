import { useCallback, useEffect, useRef, useState } from "react";
import { fs } from "../lib/fs";
import type { Entry } from "../lib/fs";
import { mark, markOnce, reportOnce } from "../lib/perf";

export interface DirState {
  entries: Entry[];
  status: "idle" | "loading" | "ready" | "error";
  error: string | null;
}

const IDLE: DirState = { entries: [], status: "idle", error: null };
const FLUSH_MS = 60;

/**
 * Carga `path` de forma incremental:
 *  - el primer lote se pinta de inmediato; los siguientes se agrupan cada 60 ms
 *    para no re-renderizar por cada lote;
 *  - al cambiar `path` (o desmontar) se cancela la lectura anterior en Rust y se
 *    ignoran sus eventos pendientes, de modo que nunca se mezclan carpetas;
 *  - `reload()` es silencioso: la lista actual sigue visible (y su desplazamiento intacto)
 *    hasta que la nueva lectura termina y la sustituye de golpe.
 */
export function useDirectory(path: string | null) {
  const [state, setState] = useState<DirState>(IDLE);
  const [nonce, setNonce] = useState(0);
  const quietNext = useRef(false);
  const reload = useCallback(() => {
    quietNext.current = true;
    setNonce((n) => n + 1);
  }, []);

  useEffect(() => {
    if (!path) {
      setState(IDLE);
      return;
    }
    let live = true;
    const quiet = quietNext.current;
    quietNext.current = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let first = true;
    const all: Entry[] = [];
    if (!quiet) setState({ entries: [], status: "loading", error: null });
    markOnce("dir-open");

    const flush = () => {
      timer = undefined;
      if (live) setState({ entries: all.slice(), status: "loading", error: null });
    };

    const handle = fs.openDir(path, (e) => {
      if (!live) return;
      if (e.type === "batch") {
        markOnce("first-batch");
        for (const x of e.entries) all.push(x);
        if (quiet) return; // recarga silenciosa: se publica todo al terminar
        if (first) {
          first = false;
          flush();
        } else if (timer === undefined) {
          timer = setTimeout(flush, FLUSH_MS);
        }
      } else if (e.type === "done") {
        clearTimeout(timer);
        if (!e.cancelled) {
          setState({ entries: all, status: "ready", error: null });
          mark("dir-done");
          reportOnce({ carpeta: path, entradas: e.total });
        }
      } else {
        clearTimeout(timer);
        setState({ entries: [], status: "error", error: e.message });
      }
    });

    return () => {
      live = false;
      clearTimeout(timer);
      handle.cancel();
    };
  }, [path, nonce]);

  return { ...state, reload };
}
