import { create } from "zustand";
import { parseExclusions } from "../lib/duplicateSelection";
import { errMsg, fs } from "../lib/fs";
import type { DupConfig, DupEvent, DupGroup, DupPending, DupProgress, DupSummary } from "../lib/fs";
import { useUi } from "./ui";

const PAGE = 100;

interface DuplicatesState {
  roots: string[];
  exclusions: string;
  minKb: number;
  maxMbps: number;
  /** Análisis interrumpido que el usuario puede continuar. */
  pending: DupPending | null;
  progress: DupProgress | null;
  summary: DupSummary | null;
  groups: DupGroup[];
  hasMore: boolean;
  loaded: boolean;
  /** Prepara la herramienta al abrirla. No analiza nada. */
  open(): void;
  addRoot(path: string): void;
  removeRoot(path: string): void;
  setOptions(o: Partial<Pick<DuplicatesState, "exclusions" | "minKb" | "maxMbps">>): void;
  /** Solo la llama el botón «Detectar duplicados». */
  start(): void;
  /** Solo la llama el botón «Reanudar». */
  resume(): void;
  discard(): void;
  /** Para guardando el avance. */
  suspend(): void;
  pause(paused: boolean): void;
  cancel(): void;
  reload(): Promise<void>;
  loadMore(): Promise<void>;
  trash(paths: string[]): Promise<void>;
  afterTrash(paths: string[]): Promise<void>;
  forget(): Promise<void>;
}

export const DEFAULT_MAX_MBPS = 200;

const EMPTY_PROGRESS: DupProgress = { phase: "scanning", paused: false, files: 0, candidates: 0, candidateBytes: 0, readBytes: 0, reused: 0, skippedCloud: 0, skippedProtected: 0, links: 0, issues: 0, current: "", step: 0, stepDone: 0, stepTotal: 0 };

/** Avance de la etapa, de 0 a 100. `null` si la etapa no tiene un total conocido. */
export const stepPercent = (p: Pick<DupProgress, "step" | "stepDone" | "stepTotal">): number | null => (p.step > 0 && p.stepTotal > 0 ? Math.min(100, (p.stepDone / p.stepTotal) * 100) : null);

export const useDuplicates = create<DuplicatesState>((set, get) => {
  const failed = (e: unknown) => {
    set({ progress: null });
    useUi.getState().toast(errMsg(e), "error");
    void fs.dupPending().then((pending) => set({ pending })).catch(() => {});
  };
  const track = (e: DupEvent) => {
    if (e.type === "progress") return set({ progress: e });
    set({ progress: null });
    if (e.outcome === "completed") void get().reload();
    else if (e.outcome === "failed") useUi.getState().toast(e.message ?? "No se pudo completar el análisis.", "error");
    void fs.dupPending().then((pending) => set({ pending })).catch(() => {});
  };
  return {
  roots: [],
  exclusions: "",
  minKb: 1,
  maxMbps: DEFAULT_MAX_MBPS,
  pending: null,
  progress: null,
  summary: null,
  groups: [],
  hasMore: false,
  loaded: false,

  open() {
    if (get().loaded || !fs.available) return;
    set({ loaded: true });
    void fs.dupDefaultExclusions().then((list) => set({ exclusions: list.join("\n") })).catch(() => {});
    void fs.dupPending().then((pending) => set({ pending })).catch(() => {});
    void get().reload();
  },

  addRoot(path) {
    const p = path.trim();
    if (p && !get().roots.some((r) => r.toLowerCase() === p.toLowerCase())) set((s) => ({ roots: [...s.roots, p] }));
  },
  removeRoot: (path) => set((s) => ({ roots: s.roots.filter((r) => r !== path) })),
  setOptions: (o) => set(o),

  start() {
    const { roots, exclusions, minKb, maxMbps, progress } = get();
    const { toast } = useUi.getState();
    if (progress) return;
    if (!roots.length) return toast("Elige al menos una carpeta o unidad.", "info");
    const config: DupConfig = { roots, exclusions: parseExclusions(exclusions), minSize: Math.max(1, Math.round(minKb * 1024)), maxMbps: Math.max(0, Math.round(maxMbps)) };
    set({ pending: null, progress: { ...EMPTY_PROGRESS } });
    fs.dupStart(config, track).catch(failed);
  },

  resume() {
    if (get().progress || !get().pending) return;
    set({ pending: null, progress: { ...EMPTY_PROGRESS } });
    fs.dupResume(track).catch(failed);
  },

  discard() {
    set({ pending: null });
    void fs.dupDiscard().catch((e) => useUi.getState().toast(errMsg(e), "error"));
  },

  suspend: () => void fs.dupSuspend().catch(() => {}),

  pause(paused) {
    set((s) => (s.progress ? { progress: { ...s.progress, paused } } : s));
    void fs.dupPause(paused).catch(() => {});
  },
  cancel: () => void fs.dupCancel().catch(() => {}),

  async reload() {
    try {
      const [summary, groups] = await Promise.all([fs.dupSummary(), fs.dupGroups(0, PAGE)]);
      set({ summary, groups, hasMore: groups.length === PAGE });
    } catch (e) {
      useUi.getState().toast(errMsg(e), "error");
    }
  },

  async loadMore() {
    const next = await fs.dupGroups(get().groups.length, PAGE).catch(() => []);
    set((s) => ({ groups: [...s.groups, ...next], hasMore: next.length === PAGE }));
  },

  async trash(paths) {
    const { toast, openDialog } = useUi.getState();
    try {
      const check = await fs.dupCheckRemoval(paths);
      if (check.blocked.length) toast(`${check.blocked.length} no se pueden eliminar: ${check.blocked[0][1]}`, "error");
      if (check.allowed.length) openDialog({ type: "delete", paths: check.allowed, permanent: false });
    } catch (e) {
      toast(errMsg(e), "error");
    }
  },

  async afterTrash(paths) {
    await fs.dupPrune(paths).catch(() => {});
    await get().reload();
  },

  async forget() {
    await fs.dupForget().catch((e) => useUi.getState().toast(errMsg(e), "error"));
    set({ summary: null, groups: [], hasMore: false });
  },
  };
});
