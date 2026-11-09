import { create } from "zustand";

export interface Toast {
  id: number;
  text: string;
  tone: "info" | "success" | "error";
  action?: { label: string; run(): void };
}

export type MenuItem =
  | { kind: "item"; label: string; run(): void; shortcut?: string; disabled?: boolean; danger?: boolean; checked?: boolean; icon?: string }
  /** Submenú (un nivel). Se abre al pasar el ratón o con la flecha derecha. */
  | { kind: "submenu"; label: string; items: MenuItem[]; icon?: string; disabled?: boolean }
  | { kind: "sep" }
  | { kind: "header"; label: string };

export type Dialog =
  | { type: "delete"; paths: string[]; permanent: boolean }
  | { type: "properties"; paths: string[] }
  | { type: "index" }
  | { type: "update" }
  | null;

interface UiState {
  toasts: Toast[];
  menu: { x: number; y: number; items: MenuItem[] } | null;
  dialog: Dialog;
  toast(text: string, tone?: Toast["tone"], action?: Toast["action"], ms?: number): void;
  dismissToast(id: number): void;
  openMenu(x: number, y: number, items: MenuItem[]): void;
  closeMenu(): void;
  openDialog(d: Exclude<Dialog, null>): void;
  closeDialog(): void;
}

let seq = 0;

export const useUi = create<UiState>((set, get) => ({
  toasts: [],
  menu: null,
  dialog: null,

  toast(text, tone = "info", action, ms) {
    const id = ++seq;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { id, text, tone, action }] }));
    // Los errores y los avisos con acción duran más para dar tiempo a leerlos o pulsar «Deshacer».
    setTimeout(() => get().dismissToast(id), ms ?? (tone === "error" ? 9000 : action ? 8000 : 4500));
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  openMenu: (x, y, items) => set({ menu: { x, y, items } }),
  closeMenu: () => set({ menu: null }),
  openDialog: (d) => set({ dialog: d, menu: null }),
  closeDialog: () => set({ dialog: null }),
}));
