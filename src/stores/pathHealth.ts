import { create } from "zustand";
import { fs } from "../lib/fs";
import type { PathState } from "../lib/fs";
import { dropMissingRecents } from "../lib/locations";
import { canonicalKey } from "../lib/path";
import { usePrefs } from "./prefs";

interface PathHealth {
  /** Estado por ruta canónica de los accesos fijados y de los recientes. */
  states: Record<string, PathState>;
  check(): Promise<void>;
}

let running = false;
let again = false;

/**
 * Comprueba en segundo plano las rutas guardadas (nunca al pintar). Un reciente que ya no existe sale de la lista; un
 * acceso fijado se conserva y se marca, porque la carpeta puede volver. Si la unidad no responde no se concluye nada.
 */
export const usePathHealth = create<PathHealth>((set, get) => ({
  states: {},

  async check() {
    if (running) {
      again = true;
      return;
    }
    const { favorites, recentFolders } = usePrefs.getState();
    const paths = [...favorites.map((f) => f.path), ...recentFolders.map((r) => r.path)];
    running = true;
    try {
      const res = paths.length ? await fs.checkPaths(paths) : [];
      const states: Record<string, PathState> = {};
      paths.forEach((p, i) => (states[canonicalKey(p)] = res[i]));
      if (JSON.stringify(states) !== JSON.stringify(get().states)) set({ states });
      const p = usePrefs.getState();
      const kept = dropMissingRecents(p.recentFolders, states);
      if (kept !== p.recentFolders) p.update({ recentFolders: kept });
    } catch {
      /* sin datos no se cambia nada */
    } finally {
      running = false;
      if (again) {
        again = false;
        void get().check();
      }
    }
  },
}));
