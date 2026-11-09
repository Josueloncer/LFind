import { create } from "zustand";
import { errMsg, fs } from "../lib/fs";
import type { ConflictInfo, Decision, DoneInfo, OpHandle, OpKind, OpRequest, Progress } from "../lib/fs";
import { useUi } from "./ui";

/** Operaciones que se ejecutan a la vez. Las demás esperan en cola: dos copias grandes en paralelo solo se estorban en el disco. */
export const MAX_PARALLEL = 2;

export type JobStatus = "queued" | "running" | "done";
/** Estado que ve el usuario. */
export type JobState = "queued" | "running" | "completed" | "partial" | "failed" | "cancelled";

export interface Job {
  key: number;
  kind: OpKind;
  title: string;
  req: OpRequest;
  /** Identifica «la misma operación» para no lanzarla dos veces. */
  sig: string;
  status: JobStatus;
  progress: Progress | null;
  conflict: ConflictInfo | null;
  result: DoneInfo | null;
  /** Solo existe cuando la operación ya salió hacia el backend. */
  handle: OpHandle | null;
  paused: boolean;
  /** Bytes por segundo, suavizado. */
  rate: number;
  rateAt: number;
}

/** Velocidad suavizada: un archivo pequeño no debe hacer saltar la cifra. */
export function nextRate(prev: number, bytes: number, ms: number): number {
  if (ms <= 0 || bytes < 0) return prev;
  const now = (bytes * 1000) / ms;
  return prev > 0 ? prev * 0.75 + now * 0.25 : now;
}

/** Segundos que faltan, o `null` si aún no se puede estimar. */
export const etaSeconds = (remaining: number, rate: number): number | null => (rate > 0 && remaining > 0 ? Math.ceil(remaining / rate) : null);

export const formatEta = (s: number): string => (s < 60 ? `${s} s` : s < 3600 ? `${Math.round(s / 60)} min` : `${Math.floor(s / 3600)} h ${Math.round((s % 3600) / 60)} min`);

/** Avance conjunto de lo que está en marcha o en cola. */
export function overallProgress(jobs: readonly Job[]): { active: number; bytesDone: number; bytesTotal: number; rate: number } {
  const active = jobs.filter((j) => j.status !== "done");
  return {
    active: active.length,
    bytesDone: active.reduce((n, j) => n + (j.progress?.bytesDone ?? 0), 0),
    bytesTotal: active.reduce((n, j) => n + (j.progress?.bytesTotal ?? 0), 0),
    rate: active.reduce((n, j) => n + (j.paused ? 0 : j.rate), 0),
  };
}

/** En cola · en curso · completada · completada con errores · fallida (nada se pudo hacer) · cancelada. */
export function jobState(j: Pick<Job, "status" | "result">): JobState {
  if (j.status === "queued") return "queued";
  if (j.status === "running") return "running";
  const r = j.result;
  if (!r) return "failed";
  if (r.cancelled) return "cancelled";
  if (r.errors.length === 0) return "completed";
  return r.completed > 0 || r.skipped > 0 ? "partial" : "failed";
}

export const STATE_LABEL: Record<JobState, string> = {
  queued: "En cola",
  running: "En curso",
  completed: "Completado",
  partial: "Completado con errores",
  failed: "Error",
  cancelled: "Cancelado",
};

/** Firma de una operación, independiente del orden de los elementos y de las mayúsculas. */
export function signature(req: OpRequest): string {
  return [req.kind, (req.dest ?? "").toLowerCase(), ...[...req.sources].map((s) => s.toLowerCase()).sort()].join("|");
}

/** Clave de comparación de una ruta (sin distinguir mayúsculas ni el tipo de barra, sin barra final). */
export const pathKey = (p: string): string => p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();

/** Un elemento es de este trabajo si el error apunta a él o a algo que contiene. Sin ruta (error global) → todos. */
export function failedSources(sources: string[], errors: { path: string }[]): string[] {
  if (errors.some((e) => !e.path)) return sources;
  const ep = errors.map((e) => pathKey(e.path));
  return sources.filter((s) => {
    const k = pathKey(s);
    return ep.some((e) => e === k || e.startsWith(k + "\\"));
  });
}

interface OpsState {
  jobs: Job[];
  /**
   * Eliminación optimista: rutas (por `pathKey`) que la vista oculta desde que se confirma la eliminación, sin esperar al
   * sistema. Se restauran si falla, se cancela o se deshace; si va bien se retiran cuando la recarga confirma que ya no están.
   */
  hidden: ReadonlySet<string>;
  /** La recarga de la carpeta terminó: deja de ocultar lo que ya no está en disco. `present` = claves de lo que sí está. */
  settle(present: ReadonlySet<string>): void;
  /** Sube al terminar cualquier operación: las carpetas abiertas se recargan. */
  version: number;
  /** `false` si se rechazó por ser idéntica a otra en cola o en curso. */
  start(req: OpRequest): boolean;
  resolve(key: number, decision: Decision, all: boolean): void;
  cancel(key: number): void;
  dismiss(key: number): void;
  undo(): Promise<void>;
  setPaused(key: number, paused: boolean): void;
  /** Renombra sin esperar turno (es instantáneo y lo está mirando el usuario), pero visible para el resto del gestor. */
  rename(path: string, newName: string): Promise<string>;
}

const VERB: Record<OpKind, [string, string]> = {
  rename: ["Renombrando", "Renombrado"],
  copy: ["Copiando", "Copiado"],
  move: ["Moviendo", "Movido"],
  trash: ["Enviando a la Papelera", "Enviado a la Papelera"],
  delete: ["Eliminando", "Eliminado"],
};

/** Tarjetas que se muestran: lo terminado sin problemas ya lo cuenta su aviso. */
export const visibleJobs = (jobs: readonly Job[]): Job[] => jobs.filter((j) => jobState(j) !== "completed");

const plural = (n: number) => `${n} elemento${n === 1 ? "" : "s"}`;
let seq = 0;

export const useOps = create<OpsState>((set, get) => {
  const hide = (paths: string[]) => set((s) => ({ hidden: new Set([...s.hidden, ...paths.map(pathKey)]) }));
  const unhide = (paths: string[]) =>
    set((s) => {
      if (!paths.length) return s;
      const h = new Set(s.hidden);
      for (const p of paths) h.delete(pathKey(p));
      return { hidden: h };
    });
  const patch = (key: number, f: (j: Job) => Job) => set((s) => ({ jobs: s.jobs.map((j) => (j.key === key ? f(j) : j)) }));

  const finish = (key: number, r: DoneInfo) => {
    const job = get().jobs.find((j) => j.key === key);
    if (!job || job.status === "done") return; // un segundo «terminado» no debe repetir el aviso ni la recarga
    patch(key, (j) => ({ ...j, status: "done", conflict: null, result: r }));
    if (job.kind === "trash" || job.kind === "delete") {
      // Lo que falló (o se canceló) vuelve a verse YA; lo que salió bien sigue oculto hasta que la recarga lo confirme.
      unhide(r.cancelled ? job.req.sources : failedSources(job.req.sources, r.errors));
      setTimeout(() => unhide(job.req.sources), 8000); // cinturón: nada se queda oculto para siempre
    }
    set((s) => ({ version: s.version + 1 }));
    const did = VERB[job.kind][1];
    const ui = useUi.getState();
    const undo = r.undoable ? { label: "Deshacer", run: () => void get().undo() } : undefined;
    const state = jobState({ status: "done", result: r });
    if (state === "partial" || state === "failed") {
      ui.toast(`${state === "failed" ? "No se pudo completar" : `${did} ${plural(r.completed)}; ${r.errors.length} con error`}. ${r.errors[0]?.message ?? ""} Detalles en el panel de transferencias.`.trim(), "error", undo);
    } else if (state === "cancelled") {
      ui.toast(`Operación cancelada (${plural(r.completed)} completado${r.completed === 1 ? "" : "s"}).`, "info", undo);
    } else {
      ui.toast(`${did} ${plural(r.completed)}${r.skipped ? ` · ${r.skipped} omitido${r.skipped === 1 ? "" : "s"}` : ""}`, "success", undo);
      setTimeout(() => get().dismiss(key), 4000); // sin errores: el trabajo se retira solo del panel
    }
    pump();
  };

  // Lanza las operaciones en cola mientras haya hueco, en orden de llegada.
  const pump = () => {
    for (;;) {
      const jobs = get().jobs;
      if (jobs.filter((j) => j.status === "running").length >= MAX_PARALLEL) return;
      const next = jobs.find((j) => j.status === "queued");
      if (!next) return;
      const key = next.key;
      patch(key, (j) => ({ ...j, status: "running" }));
      const handle = fs.startOp(next.req, (e) => {
        if (e.type === "progress") {
          patch(key, (j) => {
            const now = performance.now();
            if (j.paused || !j.progress) return { ...j, progress: e, rateAt: now };
            return { ...j, progress: e, rate: nextRate(j.rate, e.bytesDone - j.progress.bytesDone, now - j.rateAt), rateAt: now };
          });
        }
        else if (e.type === "conflict") patch(key, (j) => ({ ...j, conflict: { src: e.src, dst: e.dst } }));
        else finish(key, e);
      });
      patch(key, (j) => ({ ...j, handle, rateAt: performance.now() }));
    }
  };

  return {
    jobs: [],
    version: 0,
    hidden: new Set<string>(),

    settle(present) {
      const { hidden } = get();
      if (!hidden.size) return;
      const running = new Set(get().jobs.filter((j) => j.status !== "done").flatMap((j) => j.req.sources.map(pathKey)));
      // Se retira lo que ya no está en disco y cuyo trabajo ya terminó; lo que sigue en curso o aún aparece se mantiene oculto.
      const next = new Set([...hidden].filter((k) => running.has(k) || present.has(k)));
      if (next.size !== hidden.size) set({ hidden: next });
    },

    start(req) {
      const sig = signature(req);
      if (get().jobs.some((j) => j.sig === sig && j.status !== "done")) {
        useUi.getState().toast("Esa misma operación ya está en curso o en cola.");
        return false;
      }
      const key = ++seq;
      const title = `${VERB[req.kind][0]} ${plural(req.sources.length)}`;
      set((s) => ({ jobs: [...s.jobs, { key, kind: req.kind, title, req, sig, status: "queued", progress: null, conflict: null, result: null, handle: null, paused: false, rate: 0, rateAt: 0 }] }));
      if (req.kind === "trash" || req.kind === "delete") hide(req.sources); // desaparece de la vista al instante
      pump();
      return true;
    },

    resolve(key, decision, all) {
      const j = get().jobs.find((x) => x.key === key);
      if (!j?.handle) return;
      patch(key, (x) => ({ ...x, conflict: null }));
      j.handle.resolve(decision, all);
    },

    cancel(key) {
      const j = get().jobs.find((x) => x.key === key);
      if (!j) return;
      if (j.status === "queued") {
        // Aún no ha salido hacia el backend: se cancela sin más y la cola avanza.
        patch(key, (x) => ({ ...x, status: "done", result: { completed: 0, skipped: 0, errors: [], cancelled: true, undoable: false } }));
        if (j.kind === "trash" || j.kind === "delete") unhide(j.req.sources);
        setTimeout(() => get().dismiss(key), 2500);
        pump();
      } else if (j.status === "running") {
        j.handle?.cancel(); // el backend responde con «terminado (cancelado)»; el estado cambia entonces
      }
    },

    setPaused(key, paused) {
      const j = get().jobs.find((x) => x.key === key);
      if (!j?.handle || j.status !== "running") return;
      j.handle.pause(paused);
      patch(key, (x) => ({ ...x, paused, rate: 0, rateAt: performance.now() }));
    },

    async rename(path, newName) {
      const req: OpRequest = { kind: "rename" as OpKind, sources: [path], dest: newName };
      const sig = signature(req);
      if (get().jobs.some((j) => j.sig === sig && j.status !== "done")) throw "Ese renombrado ya está en curso.";
      const key = ++seq;
      set((s) => ({ jobs: [...s.jobs, { key, kind: "rename", title: `Renombrando ${newName}`, req, sig, status: "running", progress: null, conflict: null, result: null, handle: null, paused: false, rate: 0, rateAt: 0 }] }));
      try {
        return await fs.renamePath(path, newName);
      } finally {
        set((s) => ({ jobs: s.jobs.filter((j) => j.key !== key) }));
      }
    },

    dismiss: (key) => set((s) => ({ jobs: s.jobs.filter((j) => !(j.key === key && j.status === "done")) })),

    async undo() {
      const ui = useUi.getState();
      try {
        const r = await fs.undoLast();
        set((s) => ({ version: s.version + 1, hidden: new Set<string>() })); // lo restaurado debe verse
        if (r.errors.length) ui.toast(`Deshacer ${r.description}: ${r.restored} restaurado(s); ${r.errors[0].message}`, "error");
        else ui.toast(`Deshecho: ${r.description}.`, "success");
      } catch (e) {
        ui.toast(errMsg(e), "error");
      }
    },
  };
});
