import { create } from "zustand";
import { withPaneOrder } from "../lib/viewState";
import type { PaneOrder, PaneOrders } from "../lib/viewState";
import { samePath, upOf } from "../lib/path";
import { moveByKey } from "../lib/reorder";

export interface Tab {
  id: string;
  path: string;
  back: string[];
  forward: string[];
  /** La abrió la aplicación al arrancar: mientras el usuario no navegue, no cuenta como visita. */
  auto?: boolean;
  /** Orden propio de esta pestaña por carpeta, en la vista doble. */
  orders?: PaneOrders;
}

interface NavState {
  tabs: Tab[];
  activeId: string | null;
  init(path: string): void;
  openTab(path?: string): void;
  closeTab(id: string): void;
  setActive(id: string): void;
  moveTab(id: string, before: string | null): void;
  navigate(path: string): void;
  /** La ubicación que pulsa el usuario: si ya está abierta, la actualiza; si no, navega. */
  openLocation(path: string): void;
  refreshActive(): void;
  /** Última actualización pedida por el usuario, y en qué panel. */
  refresh: { tick: number; side: number; at: number };
  goBack(): void;
  goForward(): void;
  goUp(): void;
  /** El otro panel de la vista doble. `tabs` y `activeId` son siempre los del panel activo. */
  other: { tabs: Tab[]; activeId: string | null } | null;
  /** Lado del panel activo: 0 izquierda, 1 derecha. */
  side: 0 | 1;
  toggleDual(): void;
  focusSide(side: 0 | 1): void;
  /** Intercambia el contenido de los dos paneles. */
  swap(): void;
  setPaneOrder(side: 0 | 1, path: string, order: PaneOrder): void;
  /** Salta `n` posiciones atrás/adelante en el historial de la pestaña activa (menú del botón Atrás/Adelante). */
  jump(n: number): void;
}

let seq = 0;
/** Un doble clic en la misma ubicación no debe leer la carpeta dos veces. */
export const REFRESH_GAP = 400;
const mk = (path: string): Tab => ({ id: `t${++seq}`, path, back: [], forward: [] });

const patchActive = (s: NavState, fn: (t: Tab) => Tab): Partial<NavState> => ({
  tabs: s.tabs.map((t) => (t.id === s.activeId ? fn(t) : t)),
});

export const useNav = create<NavState>((set, get) => ({
  tabs: [],
  activeId: null,
  other: null,
  side: 0,

  toggleDual() {
    const { other, tabs, activeId } = get();
    if (other) return set({ other: null, side: 0, tabs: tabs.map((t) => (t.orders ? { ...t, orders: undefined } : t)) });
    const cur = tabs.find((t) => t.id === activeId);
    if (!cur) return;
    const t = mk(cur.path);
    set({ other: { tabs: [t], activeId: t.id } });
  },

  focusSide(side) {
    const { other, tabs, activeId, side: now } = get();
    if (!other || side === now) return;
    set({ tabs: other.tabs, activeId: other.activeId, other: { tabs, activeId }, side });
  },

  swap() {
    const { other, tabs, activeId } = get();
    if (other) set({ tabs: other.tabs, activeId: other.activeId, other: { tabs, activeId } });
  },

  setPaneOrder(side, path, order) {
    set((s) => {
      const write = (g: { tabs: Tab[]; activeId: string | null }) => g.tabs.map((t) => (t.id === g.activeId ? { ...t, orders: withPaneOrder(t.orders, path, order) } : t));
      if (side === s.side) return { tabs: write(s) };
      return s.other ? { other: { ...s.other, tabs: write(s.other) } } : s;
    });
  },

  init(path) {
    if (get().tabs.length) return;
    const t = { ...mk(path), auto: true };
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

  moveTab: (id, before) => set((s) => ({ tabs: [...moveByKey(s.tabs, (t) => t.id, id, before)] })),

  refresh: { tick: 0, side: 0, at: 0 },

  refreshActive() {
    const now = Date.now();
    if (now - get().refresh.at < REFRESH_GAP) return;
    set((s) => ({ refresh: { tick: s.refresh.tick + 1, side: s.side, at: now } }));
  },

  openLocation(path) {
    const { tabs, activeId } = get();
    const cur = tabs.find((t) => t.id === activeId);
    if (cur && samePath(cur.path, path)) get().refreshActive();
    else get().navigate(path);
  },

  navigate(path) {
    set((s) =>
      patchActive(s, (t) =>
        samePath(t.path, path) ? t : { ...t, path, back: [...t.back, t.path], forward: [], auto: false },
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
    const up = t && upOf(t.path);
    if (up) get().navigate(up);
  },
}));

/** Pestaña visible en un lado concreto de la vista doble. */
export const selectPaneTab = (side: 0 | 1) => (s: NavState): Tab | undefined => {
  const group = s.side === side ? s : s.other;
  return group?.tabs.find((t) => t.id === group.activeId);
};

/** Ruta del otro panel, si existe y es una carpeta real. */
export const otherPanePath = (): string | null => {
  const s = useNav.getState();
  const t = s.other?.tabs.find((x) => x.id === s.other?.activeId);
  return t && !t.path.startsWith("lfind://") ? t.path : null;
};

export const selectActiveTab = (s: NavState) => s.tabs.find((t) => t.id === s.activeId) ?? null;
