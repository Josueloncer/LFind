import { create } from "zustand";

/** Si el panel de terminal está abierto. Cerrado, no hay proceso ni xterm cargado. */
export const useTerminal = create<{ open: boolean; toggle(): void; hide(): void }>((set) => ({
  open: false,
  toggle: () => set((s) => ({ open: !s.open })),
  hide: () => set({ open: false }),
}));
