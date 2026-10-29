import { create } from "zustand";
import { fs } from "../lib/fs";
import type { DriveUsage, Place } from "../lib/fs";

interface AppState {
  places: Place[];
  drives: Place[];
  vibrancy: boolean;
  ready: boolean;
  /** Capacidad por unidad. Se carga después del arranque y nunca bloquea nada. */
  usage: Record<string, DriveUsage>;
  boot(): Promise<void>;
  loadUsage(): void;
}

let booting: Promise<void> | null = null;

export const useApp = create<AppState>((set) => ({
  places: [],
  drives: [],
  vibrancy: false,
  ready: false,
  usage: {},

  // Idempotente: una segunda llamada (p. ej. StrictMode en desarrollo) reutiliza la primera.
  boot() {
    return (booting ??= (async () => {
      const [info, vib] = await Promise.all([
        fs.getPlaces().catch(() => ({ places: [], drives: [] })),
        fs.vibrancy().catch(() => false),
      ]);
      set({ places: info.places, drives: info.drives, vibrancy: vib, ready: true });
    })());
  },

  // Una llamada por unidad, en paralelo: una unidad de red caída tarda lo suyo, pero no retrasa a las demás.
  loadUsage() {
    for (const d of useApp.getState().drives) {
      fs.getDriveUsage(d.path)
        .then((u) => set((s) => ({ usage: { ...s.usage, [d.path]: u } })))
        .catch(() => {});
    }
  },
}));
