import { create } from "zustand";
import { errMsg, fs } from "../lib/fs";
import type { IndexStatus } from "../lib/fs";
import { useSearch } from "./search";

/**
 * Estado del indexado. La interfaz lo consulta cada pocos segundos (una llamada barata: contadores y estado) —con más frecuencia
 * mientras se indexa— y, mientras hay una búsqueda global en pantalla, la repite en silencio para que los resultados nuevos
 * aparezcan aunque el índice aún se esté construyendo.
 */
interface IndexState {
  status: IndexStatus | null;
  /** Error de la última consulta de estado (índice no disponible, etc.). */
  error: string | null;
  poll(): Promise<void>;
  start(): () => void;
}

export const POLL_BUSY_MS = 1500;
export const POLL_IDLE_MS = 10_000;
/** Mínimo entre dos refrescos de la búsqueda en pantalla mientras se indexa. */
export const REFRESH_EVERY_MS = 3000;

let lastRefresh = 0;
let timer: ReturnType<typeof setTimeout> | null = null;
let busy = false;
let loop = 0;

export const useIndex = create<IndexState>((set, get) => ({
  status: null,
  error: null,

  async poll() {
    if (busy) return;
    busy = true;
    try {
      const prev = get().status?.phase;
      const status = await fs.indexStatus();
      set({ status, error: null });
      const s = useSearch.getState();
      if (s.scope === "global" && s.snap.active) {
        const now = Date.now();
        const finished = prev !== undefined && prev !== "idle" && status.phase === "idle";
        if ((status.phase === "scanning" && now - lastRefresh >= REFRESH_EVERY_MS) || finished) {
          lastRefresh = now;
          s.refresh();
        }
      }
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      busy = false;
    }
  },

  /** Arranca la consulta periódica. Devuelve la función que la detiene (StrictMode monta, desmonta y vuelve a montar: solo vive un ciclo). */
  start() {
    if (!fs.available) return () => {};
    const id = ++loop;
    const tick = async () => {
      await get().poll();
      if (id !== loop) return; // se detuvo (o se arrancó otro ciclo) mientras esperaba
      timer = setTimeout(tick, get().status?.phase === "scanning" ? POLL_BUSY_MS : POLL_IDLE_MS);
    };
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(tick, 0);
    return () => {
      if (id === loop) loop++;
      if (timer !== null) clearTimeout(timer);
      timer = null;
    };
  },
}));
