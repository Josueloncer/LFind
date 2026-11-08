import { useEffect, useState } from "react";
import { errMsg, fs } from "../lib/fs";
import type { ViewerSource } from "../lib/fs";

export type ViewerLoad = { status: "idle" | "loading" } | { status: "ready"; source: ViewerSource } | { status: "error"; message: string };

/** Espera antes de pedir el archivo: al recorrer una lista con las flechas solo se prepara aquél en el que el usuario se detiene. */
export const OPEN_DELAY_MS = 90;

/**
 * Pide al backend cómo mostrar un archivo. `key` debe cambiar si cambia el archivo (ruta, tamaño o fecha).
 * Lo pendiente se abandona al cambiar de archivo; al cerrar el visor se retiran los permisos.
 */
export function useViewerSource(key: string | null, path: string | null): ViewerLoad {
  const [state, setState] = useState<{ key: string | null; load: ViewerLoad }>({ key: null, load: { status: "idle" } });

  useEffect(() => {
    if (!key || !path) return setState({ key, load: { status: "idle" } });
    let live = true;
    setState({ key, load: { status: "loading" } });
    const timer = setTimeout(() => {
      fs.viewerOpen(path)
        .then((source) => live && source.kind !== "cancelled" && setState({ key, load: { status: "ready", source } }))
        .catch((e) => live && setState({ key, load: { status: "error", message: errMsg(e) } }));
    }, OPEN_DELAY_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [key, path]);

  useEffect(() => () => void fs.viewerClose().catch(() => {}), []);

  // Mientras React aplica el efecto de un archivo nuevo no se debe enseñar el contenido del anterior.
  return state.key === key ? state.load : { status: key ? "loading" : "idle" };
}
