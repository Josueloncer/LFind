import { create } from "zustand";
import { errMsg, fs } from "../lib/fs";
import type { ConflictInfo, Decision, DoneInfo, OpHandle, OpKind, OpRequest, Progress } from "../lib/fs";
import { useUi } from "./ui";

export interface Job {
  key: number;
  kind: OpKind;
  title: string;
  status: "running" | "done";
  progress: Progress | null;
  conflict: ConflictInfo | null;
  result: DoneInfo | null;
  handle: OpHandle;
}

interface OpsState {
  jobs: Job[];
  /** Sube al terminar cualquier operación: las carpetas abiertas se recargan. */
  version: number;
  start(req: OpRequest): void;
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

  return {
    jobs: [],
    version: 0,

    start(req) {
      const key = ++seq;
      const [doing, did] = VERB[req.kind];
      const title = `${doing} ${plural(req.sources.length)}`;
      const handle = fs.startOp(req, (e) => {
        if (e.type === "progress") patch(key, (j) => ({ ...j, progress: e }));
        else if (e.type === "conflict") patch(key, (j) => ({ ...j, conflict: { src: e.src, dst: e.dst } }));
        else {
          patch(key, (j) => ({ ...j, status: "done", conflict: null, result: e }));
          set((s) => ({ version: s.version + 1 }));
          const ui = useUi.getState();
          const ok = e.completed;
          if (e.errors.length) {
            ui.toast(`${did} ${plural(ok)}; ${e.errors.length} con error. Detalles en el panel de transferencias.`, "error");
          } else if (e.cancelled) {
            ui.toast(`Operación cancelada (${plural(ok)} completado${ok === 1 ? "" : "s"}).`, "info", e.undoable ? { label: "Deshacer", run: () => void get().undo() } : undefined);
          } else {
            ui.toast(`${did} ${plural(ok)}${e.skipped ? ` · ${e.skipped} omitido${e.skipped === 1 ? "" : "s"}` : ""}`, "success", e.undoable ? { label: "Deshacer", run: () => void get().undo() } : undefined);
            // Sin errores: el trabajo se retira solo del panel.
            setTimeout(() => get().dismiss(key), 4000);
          }
        }
      });
      set((s) => ({ jobs: [...s.jobs, { key, kind: req.kind, title, status: "running", progress: null, conflict: null, result: null, handle }] }));
    },

    resolve(key, decision, all) {
      const j = get().jobs.find((x) => x.key === key);
      if (!j) return;
      patch(key, (x) => ({ ...x, conflict: null }));
      j.handle.resolve(decision, all);
    },

    cancel: (key) => get().jobs.find((j) => j.key === key)?.handle.cancel(),
    dismiss: (key) => set((s) => ({ jobs: s.jobs.filter((j) => j.key !== key) })),

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
