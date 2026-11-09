import { create } from "zustand";
import type { BgSlot } from "../lib/appearance";

/** Modo «Personalizar interfaz». Dura mientras está abierto: no se guarda. */
export const useCustomize = create<{ active: boolean; enter(): void; exit(): void; appearance: boolean; openAppearance(): void; closeAppearance(): void; picking: BgSlot | null; pickBusy: boolean; startPick(slot: BgSlot): void; cancelPick(): void }>((set) => ({
  active: false,
  appearance: false,
  openAppearance: () => set({ appearance: true }),
  closeAppearance: () => set({ appearance: false, picking: null }),
  /** Mientras dura, abrir un archivo en el explorador lo elige como fondo de esa zona. */
  picking: null,
  pickBusy: false,
  startPick: (slot) => set({ picking: slot }),
  cancelPick: () => set({ picking: null }),
  enter: () => set({ active: true }),
  exit: () => set({ active: false }),
}));
