import { useEffect, useState } from "react";
import type { ThumbFailKind } from "../lib/fs";
import { thumbService } from "../lib/thumbService";

export interface ThumbState {
  url: string | null;
  /** Motivo por el que no hay miniatura (cuando ya se sabe). */
  failure: ThumbFailKind | null;
}

const NONE: ThumbState = { url: null, failure: null };

/**
 * Miniatura de un archivo. Mientras no esté lista devuelve `url: null` y la interfaz muestra el icono.
 * La solicitud solo existe mientras el componente está montado: si el elemento sale de pantalla
 * (la lista está virtualizada) antes de que la cola lo atienda, se retira sin llegar al backend.
 * `key` debe cambiar cuando cambie el archivo (ruta, fecha, tamaño) o el tamaño pedido.
 */
export function useThumb(key: string | null, path: string, px: number, front = false): ThumbState {
  const [state, setState] = useState<ThumbState>(() => {
    const hit = key ? thumbService.cached(key) : undefined;
    return hit ? { url: hit, failure: null } : NONE;
  });
  const [gen, setGen] = useState(0);
  useEffect(() => thumbService.onClear(() => setGen((g) => g + 1)), []);

  useEffect(() => {
    if (!key) return setState(NONE);
    const hit = thumbService.cached(key);
    if (hit) return setState({ url: hit, failure: null });
    setState(NONE);
    let live = true;
    const h = thumbService.request(key, path, px, front);
    void h.promise.then((o) => {
      if (live) setState(o.ok ? { url: o.url, failure: null } : { url: null, failure: o.kind === "cancelled" ? null : o.kind });
    });
    return () => {
      live = false;
      h.release();
    };
  }, [key, path, px, front, gen]);

  return state;
}
