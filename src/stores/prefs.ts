import { create } from "zustand";
import { DEFAULT_PREFS, fs } from "../lib/fs";
import type { Prefs } from "../lib/fs";
import { movePin, recordVisit, removeRecent, renamePin, togglePin } from "../lib/locations";
import { normalizeTheme } from "../lib/themes";
import { applyViewChange, effectiveView, withPinned } from "../lib/viewState";
import type { EffectiveView } from "../lib/viewState";

interface PrefsState extends Prefs {
  loaded: boolean;
  load(): Promise<void>;
  update(patch: Partial<Prefs>): void;
  /** Fija o quita una carpeta de Ubicaciones. */
  toggleFavorite(path: string, label: string): void;
  /** Cambia solo el nombre visible del acceso fijado. */
  renameFavorite(path: string, label: string): void;
  moveFavorite(from: number, before: number): void;
  /** Cambia la vista: modo y tamaño → vista global (o la de la carpeta si está fijada); orden y agrupación → carpeta. */
  setView(path: string, patch: Partial<EffectiveView>): void;
  /** Recuerda (o deja de recordar) el modo y el tamaño de icono de esa carpeta. */
  setPinned(path: string, pinned: boolean): void;
  /** Guarda ya las preferencias pendientes (al cerrar la ventana). */
  flush(): Promise<void>;
  /** Convierte la vista de esa carpeta en la predeterminada y olvida las vistas por carpeta. */
  applyViewToAll(path: string): void;
  /** Registra una visita real a una carpeta. Si no cambia nada, no se escribe. */
  visitFolder(path: string): void;
  forgetRecent(path: string): void;
  clearRecents(): void;
}

const KEYS = Object.keys(DEFAULT_PREFS) as (keyof Prefs)[];
const pick = (s: Prefs): Prefs => Object.fromEntries(KEYS.map((k) => [k, s[k]])) as unknown as Prefs;

let timer: ReturnType<typeof setTimeout> | undefined;
let loading: Promise<void> | null = null;
// Escritura diferida: varios cambios seguidos producen un solo guardado.
const scheduleSave = (get: () => Prefs) => {
  clearTimeout(timer);
  timer = setTimeout(() => {
    timer = undefined;
    void fs.savePrefs(pick(get())).catch(() => {});
  }, 400);
};

export const usePrefs = create<PrefsState>((set, get) => ({
  ...DEFAULT_PREFS,
  loaded: false,

  // Idempotente: una segunda llamada (p. ej. StrictMode en desarrollo) reutiliza la primera.
  load() {
    return (loading ??= (async () => {
      try {
        const p = await fs.loadPrefs();
        set({ ...p, theme: normalizeTheme(p.theme), loaded: true });
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
    get().update({ favorites: togglePin(get().favorites, path, label) });
  },

  renameFavorite(path, label) {
    const cur = get().favorites;
    const next = renamePin(cur, path, label);
    if (next !== cur) get().update({ favorites: [...next] });
  },

  moveFavorite(from, before) {
    const cur = get().favorites;
    const next = movePin(cur, from, before);
    if (next !== cur) get().update({ favorites: [...next] });
  },

  setView(path, patch) {
    get().update(applyViewChange(get(), path, patch));
  },

  setPinned(path, pinned) {
    get().update({ folderViews: withPinned(get(), path, pinned) });
  },

  async flush() {
    if (timer === undefined || !get().loaded) return;
    clearTimeout(timer);
    timer = undefined;
    await fs.savePrefs(pick(get())).catch(() => {});
  },

  applyViewToAll(path) {
    get().update({ ...effectiveView(get(), path), folderViews: [] });
  },

  visitFolder(path) {
    const cur = get().recentFolders;
    const next = recordVisit(cur, path, Date.now());
    if (next !== cur) get().update({ recentFolders: [...next] });
  },

  forgetRecent(path) {
    get().update({ recentFolders: removeRecent(get().recentFolders, path) });
  },

  clearRecents() {
    if (get().recentFolders.length) get().update({ recentFolders: [] });
  },
}));
