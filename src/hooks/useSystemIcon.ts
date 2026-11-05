import { useEffect, useState } from "react";
import type { Entry } from "../lib/fs";
import { iconPx } from "../lib/iconKinds";
import { iconService } from "../lib/iconService";

/**
 * Icono del sistema de un elemento (el del Explorador de Windows). Devuelve `null` mientras no esté listo o si el
 * sistema no tiene uno: la interfaz muestra entonces su glifo. La solicitud solo existe mientras el componente
 * está montado y el lote sale con todos los elementos visibles juntos (ver `IconService`).
 */
export function useSystemIcon(entry: Pick<Entry, "kind" | "name" | "size" | "modified">, path: string, cssPx: number, enabled: boolean): string | null {
  const px = iconPx(cssPx, typeof window === "undefined" ? 1 : window.devicePixelRatio || 1);
  const target = { path, entry };
  const [url, setUrl] = useState<string | null>(() => (enabled ? iconService.peek(target, px) ?? null : null));
  const [gen, setGen] = useState(0);
  useEffect(() => iconService.onClear(() => setGen((g) => g + 1)), []);

  useEffect(() => {
    if (!enabled) return setUrl(null);
    const known = iconService.peek({ path, entry }, px);
    if (known !== undefined) return setUrl(known);
    setUrl(null);
    let live = true;
    const h = iconService.request({ path, entry }, px);
    void h.promise.then((u) => live && setUrl(u));
    return () => {
      live = false;
      h.release();
    };
    // `entry` se compara por los campos que entran en la clave del icono.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, path, px, entry.kind, entry.name, entry.size, entry.modified, gen]);

  return url;
}
