import { beforeEach, describe, expect, it, vi } from "vitest";
import type { OpEvent, OpRequest } from "../lib/fs";

// Backend simulado: cada operación queda en marcha hasta que el test la termina.
const started: { req: OpRequest; emit(e: OpEvent): void; cancel: ReturnType<typeof vi.fn>; resolve: ReturnType<typeof vi.fn> }[] = [];
const undoLast = vi.fn();
vi.mock("../lib/fs", () => ({
  errMsg: (e: unknown) => String(e),
  fs: {
    startOp: (req: OpRequest, on: (e: OpEvent) => void) => {
      const h = { req, emit: on, cancel: vi.fn(), resolve: vi.fn() };
      started.push(h);
      return { cancel: h.cancel, resolve: h.resolve };
    },
    undoLast: () => undoLast(),
  },
}));

import { MAX_PARALLEL, failedSources, jobState, pathKey, signature, useOps } from "./ops";
import { useUi } from "./ui";

const req = (name: string, o: Partial<OpRequest> = {}): OpRequest => ({ kind: "copy", sources: [`C:\\a\\${name}`], dest: "C:\\b", ...o });
const done = (o: Partial<Extract<OpEvent, { type: "done" }>> = {}): OpEvent => ({ type: "done", completed: 1, skipped: 0, errors: [], cancelled: false, undoable: false, ...o });
const jobs = () => useOps.getState().jobs;
const lastToast = () => { const t = useUi.getState().toasts; return t[t.length - 1]; };
const st = (i: number) => jobState(jobs()[i]);

beforeEach(() => {
  started.length = 0;
  undoLast.mockReset();
  useOps.setState({ jobs: [], version: 0, hidden: new Set<string>() });
  useUi.setState({ toasts: [] });
});

describe("cola de operaciones", () => {
  it("solo corren MAX_PARALLEL a la vez; el resto espera y avanza en orden", () => {
    for (const n of ["1", "2", "3", "4"]) useOps.getState().start(req(n));
    expect(MAX_PARALLEL).toBe(2);
    expect(started).toHaveLength(2);
    expect([0, 1, 2, 3].map(st)).toEqual(["running", "running", "queued", "queued"]);
    started[0].emit(done());
    expect(started).toHaveLength(3);
    expect(started[2].req.sources[0]).toContain("3"); // FIFO
    expect([0, 1, 2, 3].map(st)).toEqual(["completed", "running", "running", "queued"]);
    started[1].emit(done());
    started[2].emit(done());
    started[3].emit(done());
    expect([0, 1, 2, 3].map(st)).toEqual(["completed", "completed", "completed", "completed"]);
  });

  it("no lanza dos veces la misma operación (aunque cambie el orden o las mayúsculas)", () => {
    const a = useOps.getState().start(req("x", { sources: ["C:\\a\\1", "C:\\a\\2"] }));
    const b = useOps.getState().start(req("x", { sources: ["c:\\A\\2", "C:\\a\\1"] }));
    expect(a).toBe(true);
    expect(b).toBe(false);
    expect(jobs()).toHaveLength(1);
    expect(useUi.getState().toasts[0].text).toMatch(/ya está en curso/);
    // La misma operación con otro destino o tipo no es un duplicado.
    expect(useOps.getState().start(req("x", { sources: ["C:\\a\\1", "C:\\a\\2"], dest: "C:\\c" }))).toBe(true);
    expect(useOps.getState().start(req("x", { sources: ["C:\\a\\1", "C:\\a\\2"], kind: "move" }))).toBe(true);
    // Una vez terminada, se puede repetir.
    started[0].emit(done());
    expect(useOps.getState().start(req("x", { sources: ["C:\\a\\1", "C:\\a\\2"] }))).toBe(true);
    expect(signature(req("a"))).toBe(signature(req("A")));
  });

  it("cancelar una operación en cola no llega nunca al backend y la cola sigue", () => {
    for (const n of ["1", "2", "3", "4"]) useOps.getState().start(req(n));
    useOps.getState().cancel(jobs()[2].key); // la 3.ª, en cola
    expect(st(2)).toBe("cancelled");
    expect(started).toHaveLength(2);
    started[0].emit(done());
    expect(started).toHaveLength(3);
    expect(started[2].req.sources[0]).toContain("4"); // se saltó la cancelada
  });

  it("cancelar una en curso pide la cancelación al backend y cambia de estado cuando este responde", () => {
    useOps.getState().start(req("1"));
    useOps.getState().cancel(jobs()[0].key);
    expect(started[0].cancel).toHaveBeenCalledTimes(1);
    expect(st(0)).toBe("running"); // aún no: la operación termina entre elementos
    started[0].emit(done({ cancelled: true, completed: 3, undoable: true }));
    expect(st(0)).toBe("cancelled");
    expect(lastToast()?.action?.label).toBe("Deshacer"); // lo ya copiado se puede deshacer
  });
});

describe("estados finales", () => {
  const err = { path: "C:\\a\\x", message: "Acceso denegado", kind: "denied" as const };
  it("completado, con errores, fallido y cancelado", () => {
    expect(jobState({ status: "done", result: { completed: 2, skipped: 0, errors: [], cancelled: false, undoable: false } })).toBe("completed");
    expect(jobState({ status: "done", result: { completed: 2, skipped: 0, errors: [err], cancelled: false, undoable: false } })).toBe("partial");
    expect(jobState({ status: "done", result: { completed: 0, skipped: 0, errors: [err], cancelled: false, undoable: false } })).toBe("failed");
    expect(jobState({ status: "done", result: { completed: 1, skipped: 0, errors: [err], cancelled: true, undoable: false } })).toBe("cancelled");
    expect(jobState({ status: "done", result: null })).toBe("failed");
  });

  it("un error se comunica, la tarea se queda en el panel y la vista se actualiza", () => {
    useOps.getState().start(req("1"));
    const v = useOps.getState().version;
    started[0].emit(done({ completed: 0, errors: [err] }));
    expect(st(0)).toBe("failed");
    expect(useOps.getState().version).toBe(v + 1); // se recargan las carpetas abiertas
    expect(lastToast()).toMatchObject({ tone: "error" });
    expect(lastToast()?.text).toContain("Acceso denegado");
    expect(jobs()).toHaveLength(1); // los errores no se retiran solos
  });

  it("los progresos y conflictos llegan al trabajo correcto y la respuesta va al backend", () => {
    useOps.getState().start(req("1"));
    useOps.getState().start(req("2"));
    started[1].emit({ type: "progress", bytesDone: 5, bytesTotal: 10, filesDone: 1, filesTotal: 2, current: "f.txt" });
    expect(jobs()[1].progress?.bytesDone).toBe(5);
    expect(jobs()[0].progress).toBeNull();
    const info = { path: "C:\\b\\f.txt", size: 1, modified: 1, isDir: false };
    started[1].emit({ type: "conflict", src: info, dst: info });
    expect(jobs()[1].conflict).not.toBeNull();
    useOps.getState().resolve(jobs()[1].key, "replace", true);
    expect(started[1].resolve).toHaveBeenCalledWith("replace", true);
    expect(jobs()[1].conflict).toBeNull();
  });

  it("descartar solo retira trabajos terminados", () => {
    useOps.getState().start(req("1"));
    useOps.getState().dismiss(jobs()[0].key);
    expect(jobs()).toHaveLength(1); // sigue en curso
    started[0].emit(done({ errors: [err] }));
    useOps.getState().dismiss(jobs()[0].key);
    expect(jobs()).toHaveLength(0);
  });

  it("deshacer actualiza la vista y avisa de los errores", async () => {
    undoLast.mockResolvedValueOnce({ description: "copia", restored: 2, errors: [] });
    await useOps.getState().undo();
    expect(useOps.getState().version).toBe(1);
    expect(lastToast()?.text).toContain("Deshecho");
    undoLast.mockRejectedValueOnce("No hay nada que deshacer.");
    await useOps.getState().undo();
    expect(lastToast()).toMatchObject({ tone: "error", text: "No hay nada que deshacer." });
  });
});

describe("eliminación optimista", () => {
  const del = (...names: string[]) => useOps.getState().start({ kind: "trash", sources: names.map((n) => `C:\\a\\${n}`) });
  const hid = (n: string) => useOps.getState().hidden.has(pathKey(`C:\\a\\${n}`));

  it("el elemento se oculta en cuanto se confirma, antes de que Rust responda; copiar no oculta nada", () => {
    del("x", "y");
    expect(started).toHaveLength(1);
    expect(hid("x") && hid("y")).toBe(true);
    expect(useUi.getState().toasts).toHaveLength(0); // y ningún mensaje de éxito antes de tiempo
    useOps.getState().start(req("z"));
    expect(hid("z")).toBe(false);
  });
  it("una operación duplicada rechazada no oculta nada nuevo", () => {
    del("x");
    useOps.setState({ hidden: new Set<string>() });
    expect(del("x")).toBe(false);
    expect(hid("x")).toBe(false);
  });
  it("éxito: sigue oculto hasta que la recarga confirma que ya no está; el aviso sale al terminar", () => {
    del("x");
    started[0].emit(done());
    expect(lastToast().tone).toBe("success");
    expect(hid("x")).toBe(true);
    useOps.getState().settle(new Set([pathKey("C:\\a\\x")])); // la recarga aún lo ve (lectura anterior): sigue oculto
    expect(hid("x")).toBe(true);
    useOps.getState().settle(new Set()); // la recarga ya no lo ve
    expect(hid("x")).toBe(false);
  });
  it("mientras el trabajo sigue en curso, la recarga no lo deja reaparecer", () => {
    del("x");
    useOps.getState().settle(new Set());
    expect(hid("x")).toBe(true);
  });
  it("error: el elemento fallido vuelve a verse al instante y los demás siguen ocultos", () => {
    del("x", "y");
    started[0].emit(done({ completed: 1, errors: [{ path: "C:\\a\\y", message: "Acceso denegado", kind: "permission" } as never] }));
    expect(hid("y")).toBe(false);
    expect(hid("x")).toBe(true);
    expect(lastToast().tone).toBe("error");
  });
  it("error dentro de una carpeta: se restaura la carpeta entera", () => {
    expect(failedSources(["C:\\a\\d", "C:\\a\\dd"], [{ path: "C:\\a\\d\\sub\\f.txt" }])).toEqual(["C:\\a\\d"]);
    expect(failedSources(["C:\\a\\d", "C:\\a\\e"], [{ path: "" }])).toEqual(["C:\\a\\d", "C:\\a\\e"]);
  });
  it("cancelar en cola restaura al instante; cancelar en curso restaura cuando el backend responde", () => {
    for (const n of ["1", "2"]) useOps.getState().start(req(n));
    del("x"); // en cola (ya hay 2 en curso)
    expect(jobs()[2].status).toBe("queued");
    useOps.getState().cancel(jobs()[2].key);
    expect(hid("x")).toBe(false);
    del("w", "v"); // 3.er trabajo: sigue en cola hasta que haya hueco
    started[0].emit(done()); // termina la 1.ª → arranca «w, v»
    expect(jobs().find((j) => j.kind === "trash" && j.status === "running")).toBeTruthy();
    const key = jobs().find((j) => j.kind === "trash" && j.status === "running")!.key;
    useOps.getState().cancel(key);
    expect(hid("w") && hid("v")).toBe(true); // aún no hay respuesta del backend
    started[started.length - 1].emit(done({ completed: 0, cancelled: true }));
    expect(hid("w") || hid("v")).toBe(false);
  });
  it("deshacer restaura todo lo oculto", () => {
    useOps.setState({ hidden: new Set([pathKey("C:\\a\\q")]) });
    undoLast.mockResolvedValue({ restored: 1, errors: [], description: "x" });
    return useOps.getState().undo().then(() => expect(useOps.getState().hidden.size).toBe(0));
  });
  it("pathKey ignora mayúsculas, barras y barra final", () => {
    expect(pathKey("c:/A/B/")).toBe(pathKey("C:\\a\\b"));
  });
});
