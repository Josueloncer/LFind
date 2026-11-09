import { create } from "zustand";
import { errMsg, fs } from "../lib/fs";
import type { DriveUsage, Place } from "../lib/fs";

/** Comprobar si cambiaron las unidades no toca ningún disco: es una máscara que devuelve Windows al instante. */
const DRIVE_POLL_MS = 4000;

interface AppState {
  places: Place[];
  drives: Place[];
  /** Carpetas de OneDrive. Se cargan después del arranque. */
  cloud: Place[];
  vibrancy: boolean;
  ready: boolean;
  /** Capacidad por unidad. Se carga después del arranque y nunca bloquea nada. */
  usage: Record<string, DriveUsage>;
  /** Unidades que no respondieron a la última consulta (sin medio, red caída…). */
  usageErrors: Record<string, string>;
  boot(): Promise<void>;
  /** `maxAgeMs`: no volver a preguntar si los datos son más recientes que esto. */
  loadUsage(maxAgeMs?: number): void;
  loadCloud(): void;
  refreshDrives(): Promise<void>;
  /** Detecta unidades que aparecen o desaparecen. Devuelve la función para dejar de vigilar. */
  watchDrives(): () => void;
}

let booting: Promise<void> | null = null;
let usageAt = 0;
// Una consulta por unidad como máximo: una unidad de red que tarda no acumula llamadas.
const inFlight = new Set<string>();

const without = <T,>(m: Record<string, T>, key: string): Record<string, T> => {
  if (!(key in m)) return m;
  const { [key]: _gone, ...rest } = m;
  return rest;
};
const only = <T,>(m: Record<string, T>, keep: Set<string>): Record<string, T> => Object.fromEntries(Object.entries(m).filter(([k]) => keep.has(k)));

export const useApp = create<AppState>((set, get) => ({
  places: [],
  drives: [],
  cloud: [],
  vibrancy: false,
  ready: false,
  usage: {},
  usageErrors: {},

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
  loadUsage(maxAgeMs = 0) {
    if (maxAgeMs > 0 && Date.now() - usageAt < maxAgeMs) return;
    usageAt = Date.now();
    for (const d of get().drives) {
      if (inFlight.has(d.path)) continue;
      inFlight.add(d.path);
      fs.getDriveUsage(d.path)
        .then((u) => set((s) => ({ usage: { ...s.usage, [d.path]: u }, usageErrors: without(s.usageErrors, d.path) })))
        .catch((e) => set((s) => ({ usage: without(s.usage, d.path), usageErrors: { ...s.usageErrors, [d.path]: errMsg(e) } })))
        .finally(() => inFlight.delete(d.path));
    }
  },

  loadCloud() {
    fs.getCloudPlaces()
      .then((cloud) => {
        // Sin cambios no se toca el estado: la barra lateral no se vuelve a pintar.
        if (JSON.stringify(cloud) !== JSON.stringify(get().cloud)) set({ cloud });
      })
      .catch(() => {});
  },

  async refreshDrives() {
    const drives = await fs.getDrives();
    const keep = new Set(drives.map((d) => d.path));
    set((s) => ({ drives, usage: only(s.usage, keep), usageErrors: only(s.usageErrors, keep) }));
    get().loadUsage();
  },

  watchDrives() {
    let mask: number | null = null;
    let busy = false;
    const tick = async () => {
      if (busy || document.visibilityState !== "visible") return;
      busy = true;
      try {
        const now = await fs.drivesMask();
        if (mask !== null && now !== mask) await get().refreshDrives();
        mask = now;
      } catch {
        /* se reintenta en la siguiente vuelta */
      } finally {
        busy = false;
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), DRIVE_POLL_MS);
    return () => window.clearInterval(id);
  },
}));
