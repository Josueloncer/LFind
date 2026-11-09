import { create } from "zustand";

/** Modo «Personalizar interfaz». Dura mientras está abierto: no se guarda. */
export const useCustomize = create<{ active: boolean; enter(): void; exit(): void }>((set) => ({
  active: false,
  enter: () => set({ active: true }),
  exit: () => set({ active: false }),
}));
