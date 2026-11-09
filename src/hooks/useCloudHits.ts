import { useEffect, useRef, useState } from "react";
import { fs } from "../lib/fs";
import type { SearchHit } from "../lib/fs";
import { applyCloudResults, cloudCandidates } from "../lib/locations";
import { useApp } from "../stores/app";

/** El estado puede cambiar si el usuario descarga o libera un archivo: pasado este tiempo se vuelve a preguntar. */
const TTL_MS = 30_000;
/** Al desplazarse deprisa solo se consulta donde el usuario se detiene. */
const SETTLE_MS = 120;
const MAX_REMEMBERED = 5000;
const NONE: ReadonlySet<string> = new Set();

/**
 * Rutas solo-nube entre las filas visibles de la búsqueda. El índice no guarda este dato (añadirlo obligaría a reconstruirlo),
 * así que se pregunta al sistema solo por lo que se ve y solo dentro de OneDrive: leer atributos no descarga nada.
 */
export function useCloudHits(visible: readonly SearchHit[]): ReadonlySet<string> {
  const roots = useApp((s) => s.cloud);
  const [cloud, setCloud] = useState(NONE);
  const checked = useRef(new Map<string, number>());
  const running = useRef(false);
  const again = useRef(false);
  const latest = useRef(visible);
  latest.current = visible;
  const key = visible.map((h) => h.path).join("\u0000");

  useEffect(() => {
    if (!roots.length || !key) return;
    const rootPaths = roots.map((r) => r.path);
    const run = () => {
      if (running.current) {
        again.current = true;
        return;
      }
      const now = Date.now();
      const files = latest.current.filter((h) => !h.isDir).map((h) => h.path);
      const paths = cloudCandidates(files, rootPaths, checked.current, now, TTL_MS);
      if (!paths.length) return;
      running.current = true;
      fs.cloudOnly(paths)
        .then((res) => {
          if (checked.current.size > MAX_REMEMBERED) checked.current.clear();
          paths.forEach((p) => checked.current.set(p, now));
          setCloud((prev) => applyCloudResults(prev, paths, res));
        })
        .catch(() => {})
        .finally(() => {
          running.current = false;
          if (again.current) {
            again.current = false;
            run();
          }
        });
    };
    const t = setTimeout(run, SETTLE_MS);
    return () => clearTimeout(t);
  }, [key, roots]);

  return cloud;
}
