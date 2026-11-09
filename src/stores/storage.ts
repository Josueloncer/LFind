import { create } from "zustand";
import { errMsg, fs } from "../lib/fs";
import type { AnalyzeEvent, AnalyzeProgress, ReportInfo } from "../lib/fs";
import { parseStorageRoute, storageRoute } from "../lib/path";
import type { MenuItem } from "./ui";
import { useNav } from "./nav";
import { useUi } from "./ui";

export type JobPhase = "scanning" | "paused" | "finishing" | "cancelling";

export interface StorageJob {
  root: string;
  phase: JobPhase;
  progress: AnalyzeProgress | null;
}

interface StorageState {
  job: StorageJob | null;
  reports: ReportInfo[];
  /** Cambia al terminar un análisis, al quitar un informe o al pedir «Actualizar»: las vistas vuelven a consultar. */
  version: number;
  loadReports(): void;
  refresh(): void;
  /** Abre el informe de una ubicación; solo analiza si todavía no hay ninguno. */
  analyze(root: string): void;
  /** Inicia un análisis nuevo. Nunca se llama sin una acción del usuario. */
  start(root: string): void;
  pause(): void;
  resume(): void;
  cancel(): void;
  forget(root: string): void;
}

const sameRoot = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();

export const useStorage = create<StorageState>((set, get) => ({
  job: null,
  reports: [],
  version: 0,

  loadReports() {
    if (fs.available) void fs.analyzerReports().then((reports) => set({ reports })).catch(() => {});
  },

  refresh() {
    set((s) => ({ version: s.version + 1 }));
    get().loadReports();
  },

  analyze(root) {
    useNav.getState().navigate(storageRoute(root));
    const job = get().job;
    if (job && sameRoot(job.root, root)) return;
    void fs.analyzerReport(root).then((r) => {
      if (!r && !get().job) get().start(root);
    }).catch((e) => useUi.getState().toast(errMsg(e), "error"));
  },

  start(root) {
    const toast = useUi.getState().toast;
    if (get().job) return toast("Ya hay un análisis en curso. Espera a que termine o cancélalo.", "info");
    set({ job: { root, phase: "scanning", progress: null } });
    const onEvent = (e: AnalyzeEvent) => {
      if (e.type === "progress") {
        set((s) => (s.job ? { job: { ...s.job, progress: e, phase: s.job.phase === "cancelling" ? "cancelling" : e.paused ? "paused" : "scanning" } } : s));
      } else if (e.type === "finishing") {
        set((s) => (s.job ? { job: { ...s.job, phase: s.job.phase === "cancelling" ? "cancelling" : "finishing" } } : s));
      } else {
        set((s) => ({ job: null, version: s.version + 1 }));
        get().loadReports();
        if (e.outcome === "completed") toast("Análisis completado.", "success");
        else if (e.outcome === "cancelled") toast("Análisis cancelado. Se conserva el informe anterior, si existía.", "info");
        else toast(e.message ?? "No se pudo completar el análisis.", "error");
      }
    };
    fs.analyzerStart(root, onEvent).catch((e) => {
      set({ job: null });
      toast(errMsg(e), "error");
    });
  },

  pause() {
    set((s) => (s.job ? { job: { ...s.job, phase: "paused" } } : s));
    void fs.analyzerPause().catch(() => {});
  },

  resume() {
    set((s) => (s.job ? { job: { ...s.job, phase: "scanning" } } : s));
    void fs.analyzerResume().catch(() => {});
  },

  cancel() {
    set((s) => (s.job ? { job: { ...s.job, phase: "cancelling" } } : s));
    void fs.analyzerCancel().catch(() => {});
  },

  forget(root) {
    fs.analyzerForget(root).then(() => get().refresh()).catch((e) => useUi.getState().toast(errMsg(e), "error"));
  },
}));

/** Subcarpetas de una miga del analizador (para su menú). Datos del informe: no toca el disco analizado. */
export async function storageCrumbMenu(route: string): Promise<MenuItem[]> {
  const { root, at } = parseStorageRoute(route);
  const nav = useNav.getState();
  if (!root) return useStorage.getState().reports.slice(0, 15).map((r): MenuItem => ({ kind: "item", label: r.root, icon: "folder", run: () => nav.navigate(storageRoute(r.root)) }));
  const node = await fs.analyzerNode(root, at).catch(() => null);
  if (!node) return [];
  const page = await fs.analyzerDirs(root, { dir: node.id, tree: false, sort: "size", desc: true, offset: 0, limit: 100 }).catch(() => null);
  return (page?.items ?? []).map((d): MenuItem => ({ kind: "item", label: d.name, icon: "folder", run: () => nav.navigate(storageRoute(root, at ? `${at}/${d.name}` : d.name)) }));
}
