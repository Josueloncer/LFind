import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

const load = vi.fn<(id: string) => Promise<string | null>>();
const bake = vi.fn<(url: string, blur: number, contrast: number, side: number) => Promise<string>>();
vi.mock("../lib/fs", () => ({ fs: { available: true, backgroundLoad: (id: string) => load(id) } }));
vi.mock("../lib/backgroundImage", () => ({ bakeBackground: (u: string, b: number, c: number, s: number) => bake(u, b, c, s) }));
vi.stubGlobal("window", { innerWidth: 1200, innerHeight: 800 });

import { resetBackgroundsForTest, useBackgrounds } from "./backgrounds";

const flush = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); await new Promise((r) => setTimeout(r)); };
const bg = () => useBackgrounds.getState();

beforeEach(() => {
  resetBackgroundsForTest();
  load.mockReset();
  bake.mockReset();
  load.mockImplementation(async (id) => `data:image/webp;base64,${id}`);
  bake.mockImplementation(async (u, b, c) => `${u}|${b}|${c}`);
});

describe("fondos en memoria", () => {
  it("no pide dos veces la misma imagen mientras se está cargando", async () => {
    for (let i = 0; i < 20; i++) bg().ensure(["a"]);
    expect(load).toHaveBeenCalledTimes(1);
    await flush();
    bg().ensure(["a"]);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("una imagen que dejó de usarse mientras llegaba no se queda en memoria", async () => {
    bg().ensure(["a"]);
    bg().ensure([]);
    await flush();
    expect(bg().urls).toEqual({});
  });

  it("al dejar de usar una imagen se libera también la preparada", async () => {
    bg().ensure(["a", "b"]);
    await flush();
    bg().bake(14, 100);
    await flush();
    bg().ensure(["b"]);
    expect([Object.keys(bg().urls), Object.keys(bg().baked)]).toEqual([["b"], ["b"]]);
  });

  it("muchos cambios seguidos del control preparan la imagen como mucho dos veces, y gana el último", async () => {
    bg().ensure(["a"]);
    await flush();
    for (let b = 1; b <= 30; b++) bg().bake(b, 100);
    await flush();
    expect(bake.mock.calls.length).toBeLessThanOrEqual(2);
    expect(bg().baked.a).toContain("|30|100");
  });

  it("no repite el trabajo si los ajustes no cambian", async () => {
    bg().ensure(["a"]);
    await flush();
    bg().bake(14, 100);
    await flush();
    bg().bake(14, 100);
    await flush();
    expect(bake).toHaveBeenCalledTimes(1);
  });

  it("usa el tamaño de la ventana y solo repite el trabajo si cambia de forma apreciable", async () => {
    bg().ensure(["a"]);
    await flush();
    bg().bake(14, 100);
    await flush();
    expect(bake.mock.calls[0][3]).toBe(1200);
    vi.stubGlobal("window", { innerWidth: 1250, innerHeight: 800 });
    bg().bake(14, 100);
    await flush();
    expect(bake).toHaveBeenCalledTimes(1);
    vi.stubGlobal("window", { innerWidth: 1900, innerHeight: 1000 });
    bg().bake(14, 100);
    await flush();
    expect(bake).toHaveBeenCalledTimes(2);
    expect(bake.mock.calls[1][3]).toBe(1900);
    vi.stubGlobal("window", { innerWidth: 1200, innerHeight: 800 });
  });

  it("si no se puede preparar, muestra la original y recurre al filtro en vivo", async () => {
    bake.mockRejectedValue(new Error("sin filtros"));
    bg().ensure(["a"]);
    await flush();
    bg().bake(14, 100);
    await flush();
    expect([bg().baked.a, bg().liveFilter]).toEqual(["data:image/webp;base64,a", true]);
  });
});

describe("hoja de estilos de los fondos", () => {
  const css = readFileSync(new URL("../styles/app.css", import.meta.url), "utf8");
  it("solo usa el filtro a pantalla completa como último recurso", () => {
    const withFilter = css.split("\n").filter((l) => /filter:\s*blur\(var\(--cz-blur\)\)/.test(l));
    expect(withFilter).toHaveLength(1);
    expect(withFilter[0]).toContain("html[data-bg-filter]");
  });
});
