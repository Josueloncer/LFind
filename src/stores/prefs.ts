import { create } from "zustand";
import { DEFAULT_PREFS, fs } from "../lib/fs";
import type { Prefs } from "../lib/fs";
import { samePath } from "../lib/path";
import { effectiveView, withFolderView, withRecent } from "../lib/viewState";
import type { EffectiveView } from "../lib/viewState";

interface PrefsState extends Prefs {
  loaded: boolean;
  load(): Promise<void>;
  update(patch: Partial<Prefs>): void;
  toggleFavorite(path: string, label: string): void;
  /** Cambia la vista solo para esa carpeta. */
  setFolderView(path: string, patch: Partial<EffectiveView>): void;
  /** Convierte la vista de esa carpeta en la predeterminada y olvida las vistas por carpeta. */
  applyViewToAll(path: string): void;
  pushRecent(path: string): void;
}

const KEYS = Object.keys(DEFAULT_PREFS) as (keyof Prefs)[];
const pick = (s: Prefs): Prefs => Object.fromEntries(KEYS.map((k) => [k, s[k]])) as unknown as Prefs;

let timer: ReturnType<typeof setTimeout> | undefined;
let loading: Promise<void> | null = null;
// Escritura diferida: varios cambios seguidos producen un solo guardado.
const scheduleSave = (get: () => Prefs) => {
  clearTimeout(timer);
  timer = setTimeout(() => void fs.savePrefs(pick(get())).catch(() => {}), 400);
};

export const usePrefs = create<PrefsState>((set, get) => ({
  ...DEFAULT_PREFS,
  loaded: false,

  // Idempotente: una segunda llamada (p. ej. StrictMode en desarrollo) reutiliza la primera.
  load() {
    return (loading ??= (async () => {
      try {
        set({ ...(await fs.loadPrefs()), loaded: true });
      } catch {
        set({ loaded: true });
      }
    })());
  },

  update(patch) {
    set(patch);
    scheduleSave(get);
  },

  toggleFavorite(path, label) {
    const { favorites } = get();
    const exists = favorites.some((f) => samePath(f.path, path));
    get().update({
      favorites: exists ? favorites.filter((f) => !samePath(f.path, path)) : [...favorites, { path, label }],
    });
  },

  setFolderView(path, patch) {
    get().update({ folderViews: withFolderView(get(), path, patch) });
  },

  applyViewToAll(path) {
    get().update({ ...effectiveView(get(), path), folderViews: [] });
  },

  pushRecent(path) {
    const next = withRecent(get().recents, path);
    if (next[0] !== get().recents[0] || next.length !== get().recents.length) get().update({ recents: next });
  },
}));
