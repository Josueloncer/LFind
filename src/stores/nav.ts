import { create } from "zustand";
import { parent, samePath } from "../lib/path";

export interface Tab {
  id: string;
  path: string;
  back: string[];
  forward: string[];
}

interface NavState {
  tabs: Tab[];
  activeId: string | null;
  init(path: string): void;
  openTab(path?: string): void;
  closeTab(id: string): void;
  setActive(id: string): void;
  navigate(path: string): void;
  goBack(): void;
  goForward(): void;
  goUp(): void;
  /** Salta `n` posiciones atrás/adelante en el historial de la pestaña activa (menú del botón Atrás/Adelante). */
  jump(n: number): void;
}

let seq = 0;
const mk = (path: string): Tab => ({ id: `t${++seq}`, path, back: [], forward: [] });

const patchActive = (s: NavState, fn: (t: Tab) => Tab): Partial<NavState> => ({
  tabs: s.tabs.map((t) => (t.id === s.activeId ? fn(t) : t)),
});

export const useNav = create<NavState>((set, get) => ({
  tabs: [],
  activeId: null,

  init(path) {
    if (get().tabs.length) return;
    const t = mk(path);
    set({ tabs: [t], activeId: t.id });
  },

  openTab(path) {
    const cur = get().tabs.find((t) => t.id === get().activeId);
    const p = path ?? cur?.path;
    if (!p) return;
    const t = mk(p);
    set((s) => ({ tabs: [...s.tabs, t], activeId: t.id }));
  },

  closeTab(id) {
    const { tabs, activeId } = get();
    if (tabs.length <= 1) return;
    const i = tabs.findIndex((t) => t.id === id);
    const next = tabs.filter((t) => t.id !== id);
    set({ tabs: next, activeId: activeId === id ? next[Math.min(i, next.length - 1)].id : activeId });
  },

  setActive: (id) => set({ activeId: id }),

  navigate(path) {
    set((s) =>
      patchActive(s, (t) =>
        samePath(t.path, path) ? t : { ...t, path, back: [...t.back, t.path], forward: [] },
      ),
    );
  },

  goBack() {
    set((s) =>
      patchActive(s, (t) => {
        const prev = t.back[t.back.length - 1];
        return prev === undefined
          ? t
          : { ...t, path: prev, back: t.back.slice(0, -1), forward: [t.path, ...t.forward] };
      }),
    );
  },

  goForward() {
    set((s) =>
      patchActive(s, (t) => {
        const next = t.forward[0];
        return next === undefined
          ? t
          : { ...t, path: next, back: [...t.back, t.path], forward: t.forward.slice(1) };
      }),
    );
  },

  jump(n) {
    set((s) =>
      patchActive(s, (t) => {
        if (n < 0) {
          const k = Math.min(-n, t.back.length);
          if (!k) return t;
          const target = t.back[t.back.length - k];
          return { ...t, path: target, back: t.back.slice(0, -k), forward: [...(k > 1 ? t.back.slice(-(k - 1)) : []), t.path, ...t.forward] };
        }
        const k = Math.min(n, t.forward.length);
        if (!k) return t;
        const target = t.forward[k - 1];
        return { ...t, path: target, back: [...t.back, t.path, ...t.forward.slice(0, k - 1)], forward: t.forward.slice(k) };
      }),
    );
  },

  goUp() {
    const t = get().tabs.find((x) => x.id === get().activeId);
    const up = t && parent(t.path);
    if (up) get().navigate(up);
  },
}));

export const selectActiveTab = (s: NavState) => s.tabs.find((t) => t.id === s.activeId) ?? null;
