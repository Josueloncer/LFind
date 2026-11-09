import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/fs", async (orig) => {
  const real = await orig<typeof import("../lib/fs")>();
  return { ...real, fs: { available: true, updateCheck: vi.fn(), updateDownload: vi.fn(), updateCancel: vi.fn().mockResolvedValue(undefined), updateRestart: vi.fn(), appVersion: vi.fn().mockResolvedValue("0.1.5"), savePrefs: vi.fn().mockResolvedValue(undefined) } };
});

import { fs } from "../lib/fs";
import type { FsProvider, UpdateEvent } from "../lib/fs";
import { usePrefs } from "./prefs";
import { useUi } from "./ui";
import { AUTO_CHECK_EVERY_MS, shouldAutoCheck, useUpdate } from "./update";

const api = vi.mocked(fs as FsProvider);
const info = { version: "0.1.9", notes: "Novedades", size: 1000 };
const phase = () => useUpdate.getState().state.phase;

beforeEach(() => {
  vi.clearAllMocks();
  useUpdate.setState({ state: { phase: "idle" }, installed: "" });
  usePrefs.setState({ autoUpdateCheck: true, lastUpdateCheck: 0 });
  useUi.setState({ toasts: [], dialog: null });
});
afterEach(() => vi.useRealTimers());

describe("comprobación manual", () => {
  it("sin novedades", async () => {
    api.updateCheck.mockResolvedValue(null);
    await useUpdate.getState().check();
    expect(phase()).toBe("latest");
  });
  it("con una versión nueva", async () => {
    api.updateCheck.mockResolvedValue(info);
    await useUpdate.getState().check();
    expect(useUpdate.getState().state).toEqual({ phase: "available", info });
  });
  it("un error no deja la interfaz colgada y explica el motivo", async () => {
    api.updateCheck.mockRejectedValue({ kind: "offline", message: "Sin conexión." });
    await useUpdate.getState().check();
    expect(useUpdate.getState().state).toMatchObject({ phase: "error", message: "No se pudo comprobar si hay actualizaciones. Sin conexión." });
  });
  it("no interrumpe una descarga en curso", async () => {
    useUpdate.setState({ state: { phase: "downloading", info, done: 1, total: 2 } });
    await useUpdate.getState().check();
    expect(api.updateCheck).not.toHaveBeenCalled();
  });
});

describe("comprobación automática", () => {
  it("respeta la preferencia y el intervalo", () => {
    expect(shouldAutoCheck(false, 0, 1e12)).toBe(false);
    expect(shouldAutoCheck(true, 1e12, 1e12 + AUTO_CHECK_EVERY_MS - 1)).toBe(false);
    expect(shouldAutoCheck(true, 1e12, 1e12 + AUTO_CHECK_EVERY_MS)).toBe(true);
  });
  it("avisa con discreción y nunca descarga", async () => {
    api.updateCheck.mockResolvedValue(info);
    await useUpdate.getState().autoCheck();
    expect(phase()).toBe("available");
    expect(useUi.getState().toasts).toHaveLength(1);
    expect(api.updateDownload).not.toHaveBeenCalled();
    expect(usePrefs.getState().lastUpdateCheck).toBeGreaterThan(0);
  });
  it("un fallo es silencioso", async () => {
    api.updateCheck.mockRejectedValue({ kind: "offline", message: "x" });
    await useUpdate.getState().autoCheck();
    expect(phase()).toBe("idle");
    expect(useUi.getState().toasts).toHaveLength(0);
  });
  it("desactivada no consulta", async () => {
    usePrefs.setState({ autoUpdateCheck: false });
    await useUpdate.getState().autoCheck();
    expect(api.updateCheck).not.toHaveBeenCalled();
  });
});

describe("descarga e instalación", () => {
  const available = () => useUpdate.setState({ state: { phase: "available", info } });

  it("muestra el progreso real, verifica y queda lista sin instalar", async () => {
    available();
    let emit!: (e: UpdateEvent) => void;
    let finish!: () => void;
    api.updateDownload.mockImplementation((cb) => ((emit = cb), new Promise<void>((r) => (finish = r))));
    const p = useUpdate.getState().download();
    emit({ type: "progress", done: 250, total: 1000 });
    expect(useUpdate.getState().state).toMatchObject({ phase: "downloading", done: 250, total: 1000 });
    emit({ type: "verifying" });
    expect(phase()).toBe("verifying");
    finish();
    await p;
    expect(phase()).toBe("ready");
    expect(api.updateRestart).not.toHaveBeenCalled();
  });

  it("cancelar vuelve a poder actualizar", async () => {
    available();
    api.updateDownload.mockRejectedValue({ kind: "cancelled", message: "Descarga cancelada." });
    await useUpdate.getState().download();
    expect(phase()).toBe("available");
  });

  it("una firma inválida se informa y no se puede reiniciar", async () => {
    available();
    api.updateDownload.mockRejectedValue({ kind: "signature", message: "Firma no válida." });
    await useUpdate.getState().download();
    expect(useUpdate.getState().state).toMatchObject({ phase: "error", message: "Firma no válida." });
    await useUpdate.getState().restart();
    expect(api.updateRestart).not.toHaveBeenCalled();
  });

  it("si la instalación no arranca, LFind sigue y se puede reintentar", async () => {
    useUpdate.setState({ state: { phase: "ready", info } });
    api.updateRestart.mockRejectedValue({ kind: "install", message: "No se pudo." });
    await useUpdate.getState().restart();
    expect(phase()).toBe("ready");
    expect(useUi.getState().toasts[0].text).toBe("No se pudo.");
  });
});
