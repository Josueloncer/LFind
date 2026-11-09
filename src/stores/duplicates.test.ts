import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DupEvent } from "../lib/fs";

const api = vi.hoisted(() => ({
  dupDefaultExclusions: vi.fn(async () => ["C:\\Windows"]),
  dupSummary: vi.fn(async () => null),
  dupGroups: vi.fn(async () => []),
  dupStart: vi.fn(async (_c: unknown, _e: (x: DupEvent) => void) => {}),
  dupResume: vi.fn(async (_e: (x: DupEvent) => void) => {}),
  dupPending: vi.fn<() => Promise<unknown>>(async () => null),
  dupDiscard: vi.fn(async () => {}),
  dupSuspend: vi.fn(async () => {}),
  dupCheckRemoval: vi.fn(),
  dupPrune: vi.fn(async () => {}),
  dupPause: vi.fn(async () => {}),
  dupCancel: vi.fn(async () => {}),
}));
vi.mock("../lib/fs", () => ({ errMsg: String, fs: { available: true, ...api } }));

import { DEFAULT_MAX_MBPS, stepPercent, useDuplicates } from "./duplicates";
import { useUi } from "./ui";

const s = () => useDuplicates.getState();

beforeEach(() => {
  for (const f of Object.values(api)) f.mockClear();
  useDuplicates.setState({ pending: null, maxMbps: DEFAULT_MAX_MBPS, roots: [], exclusions: "", progress: null, summary: null, groups: [], hasMore: false, loaded: false });
  useUi.setState({ toasts: [], dialog: null });
});

describe("detector de duplicados en la interfaz", () => {
  it("abrir la herramienta no analiza nada", async () => {
    s().open();
    await Promise.resolve();
    expect(api.dupStart).not.toHaveBeenCalled();
    expect(api.dupSummary).toHaveBeenCalledTimes(1);
  });
  it("abrir la herramienta nunca reanuda por su cuenta", async () => {
    api.dupPending.mockResolvedValueOnce({ roots: ["C:\\a"], step: 1, stepDone: 5, stepTotal: 10, files: 10, savedAt: 1 });
    s().open();
    await new Promise((r) => setTimeout(r));
    expect(s().pending?.roots).toEqual(["C:\\a"]);
    expect(api.dupResume).not.toHaveBeenCalled();
    expect(api.dupStart).not.toHaveBeenCalled();
  });
  it("reanudar y descartar solo actúan al pedirlo", () => {
    const pending = { roots: ["C:\\a"], step: 2, stepDone: 1, stepTotal: 2, files: 3, savedAt: 1 };
    useDuplicates.setState({ pending });
    s().resume();
    expect(api.dupResume).toHaveBeenCalledTimes(1);
    expect(s().progress).not.toBeNull();
    useDuplicates.setState({ pending, progress: null });
    s().discard();
    expect(api.dupDiscard).toHaveBeenCalledTimes(1);
    expect(s().pending).toBeNull();
  });
  it("al parar queda disponible para continuar", async () => {
    s().addRoot("C:\\a");
    s().start();
    api.dupPending.mockResolvedValueOnce({ roots: ["C:\\a"], step: 1, stepDone: 1, stepTotal: 2, files: 2, savedAt: 1 });
    api.dupStart.mock.calls[0][1]({ type: "done", outcome: "suspended", message: null, summary: null });
    await new Promise((r) => setTimeout(r));
    expect(s().progress).toBeNull();
    expect(s().pending).not.toBeNull();
  });
  it("el porcentaje es el de la etapa, y sin total no se inventa", () => {
    expect(stepPercent({ step: 2, stepDone: 50, stepTotal: 200 })).toBe(25);
    expect(stepPercent({ step: 0, stepDone: 50, stepTotal: 200 })).toBeNull();
    expect(stepPercent({ step: 1, stepDone: 0, stepTotal: 0 })).toBeNull();
  });
  it("sin carpetas no empieza", () => {
    s().start();
    expect(api.dupStart).not.toHaveBeenCalled();
    expect(useUi.getState().toasts[0].text).toMatch(/carpeta/);
  });
  it("envía la configuración y no lanza dos a la vez", () => {
    s().addRoot("C:\\a");
    s().addRoot("c:\\A");
    s().setOptions({ exclusions: "x\nx\ny", minKb: 2, maxMbps: 10 });
    s().start();
    s().start();
    expect(api.dupStart).toHaveBeenCalledTimes(1);
    expect(api.dupStart.mock.calls[0][0]).toEqual({ roots: ["C:\\a"], exclusions: ["x", "y"], minSize: 2048, maxMbps: 10 });
  });
  it("al terminar, limpia el progreso y lee los resultados", async () => {
    s().addRoot("C:\\a");
    s().start();
    const emit = api.dupStart.mock.calls[0][1];
    emit({ type: "done", outcome: "completed", message: null, summary: null });
    await Promise.resolve();
    expect(s().progress).toBeNull();
    expect(api.dupGroups).toHaveBeenCalled();
  });
  it("la Papelera pasa por la comprobación y por la confirmación", async () => {
    api.dupCheckRemoval.mockResolvedValue({ allowed: ["a"], blocked: [["b", "Se quedaría sin ninguna copia."]] });
    await s().trash(["a", "b"]);
    expect(useUi.getState().dialog).toEqual({ type: "delete", paths: ["a"], permanent: false });
    expect(useUi.getState().toasts[0].text).toMatch(/1 no se pueden/);
  });
  it("si no hay nada permitido, no pregunta", async () => {
    api.dupCheckRemoval.mockResolvedValue({ allowed: [], blocked: [["b", "x"]] });
    await s().trash(["b"]);
    expect(useUi.getState().dialog).toBeNull();
  });
});
