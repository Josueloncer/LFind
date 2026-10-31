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

interface OpsState {
  jobs: Job[];
  /** Sube al terminar cualquier operación: las carpetas abiertas se recargan. */
  version: number;
  /** `false` si se rechazó por ser idéntica a otra en cola o en curso. */
  start(req: OpRequest): boolean;
  resolve(key: number, decision: Decision, all: boolean): void;
  cancel(key: number): void;
  dismiss(key: number): void;
  undo(): Promise<void>;
}

const VERB: Record<OpKind, [string, string]> = {
  copy: ["Copiando", "Copiado"],
  move: ["Moviendo", "Movido"],
  trash: ["Enviando a la Papelera", "Enviado a la Papelera"],
  delete: ["Eliminando", "Eliminado"],
};

const plural = (n: number) => `${n} elemento${n === 1 ? "" : "s"}`;
let seq = 0;

export const useOps = create<OpsState>((set, get) => {
  const patch = (key: number, f: (j: Job) => Job) => set((s) => ({ jobs: s.jobs.map((j) => (j.key === key ? f(j) : j)) }));

  const finish = (key: number, r: DoneInfo) => {
    const job = get().jobs.find((j) => j.key === key);
    if (!job) return;
    patch(key, (j) => ({ ...j, status: "done", conflict: null, result: r }));
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
        if (e.type === "progress") patch(key, (j) => ({ ...j, progress: e }));
        else if (e.type === "conflict") patch(key, (j) => ({ ...j, conflict: { src: e.src, dst: e.dst } }));
        else finish(key, e);
      });
      patch(key, (j) => ({ ...j, handle }));
    }
  };

  return {
    jobs: [],
    version: 0,

    start(req) {
      const sig = signature(req);
      if (get().jobs.some((j) => j.sig === sig && j.status !== "done")) {
        useUi.getState().toast("Esa misma operación ya está en curso o en cola.");
        return false;
      }
      const key = ++seq;
      const title = `${VERB[req.kind][0]} ${plural(req.sources.length)}`;
      set((s) => ({ jobs: [...s.jobs, { key, kind: req.kind, title, req, sig, status: "queued", progress: null, conflict: null, result: null, handle: null }] }));
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
        setTimeout(() => get().dismiss(key), 2500);
        pump();
      } else if (j.status === "running") {
        j.handle?.cancel(); // el backend responde con «terminado (cancelado)»; el estado cambia entonces
      }
    },

    dismiss: (key) => set((s) => ({ jobs: s.jobs.filter((j) => !(j.key === key && j.status === "done")) })),

    async undo() {
      const ui = useUi.getState();
      try {
        const r = await fs.undoLast();
        set((s) => ({ version: s.version + 1 }));
        if (r.errors.length) ui.toast(`Deshacer ${r.description}: ${r.restored} restaurado(s); ${r.errors[0].message}`, "error");
        else ui.toast(`Deshecho: ${r.description}.`, "success");
      } catch (e) {
        ui.toast(errMsg(e), "error");
      }
    },
  };
});
