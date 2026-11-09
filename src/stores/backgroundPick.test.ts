import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

const backgroundImport = vi.fn<(path: string) => Promise<string>>();
vi.mock("../lib/fs", async (orig) => ({ ...(await orig<typeof import("../lib/fs")>()), fs: { available: false, backgroundImport: (p: string) => backgroundImport(p) } }));

import { DEFAULT_APPEARANCE } from "../lib/appearance";
import { chooseBackground, isBackgroundImage } from "./backgroundPick";
import { useCustomize } from "./customize";
import { usePrefs } from "./prefs";
import { useUi } from "./ui";

const setAppearance = vi.fn();
const lastToast = () => useUi.getState().toasts.at(-1)?.text ?? "";

beforeEach(() => {
  backgroundImport.mockReset();
  setAppearance.mockReset();
  useUi.setState({ toasts: [] });
  useCustomize.setState({ picking: "main", pickBusy: false });
  usePrefs.setState({ appearance: DEFAULT_APPEARANCE, setAppearance });
});

describe("elegir un fondo desde el explorador", () => {
  it("reconoce las imágenes admitidas", () => {
    for (const p of ["C:\\a.PNG", "b.jpg", "c.jpeg", "d.webp", "e.bmp", "f.gif", "g.tif"]) expect(isBackgroundImage(p)).toBe(true);
    for (const p of ["a.txt", "b.exe", "png"]) expect(isBackgroundImage(p)).toBe(false);
  });

  it("aplica la imagen a la zona indicada y termina el modo", async () => {
    backgroundImport.mockResolvedValue("0123456789abcdef");
    await chooseBackground("C:\\fotos\\a.jpg");
    expect(backgroundImport).toHaveBeenCalledWith("C:\\fotos\\a.jpg");
    expect(setAppearance).toHaveBeenCalledWith({ backgrounds: { ...DEFAULT_APPEARANCE.backgrounds, main: "0123456789abcdef" } });
    expect(useCustomize.getState().picking).toBeNull();
  });

  it("si no es una imagen, avisa y deja elegir otra sin abrir nada", async () => {
    await chooseBackground("C:\\nota.txt");
    expect(backgroundImport).not.toHaveBeenCalled();
    expect(useCustomize.getState().picking).toBe("main");
    expect(lastToast()).toMatch(/PNG/);
  });

  it("nunca abre una imagen que solo está en la nube", async () => {
    await chooseBackground("C:\\OneDrive\\a.jpg", true);
    expect(backgroundImport).not.toHaveBeenCalled();
    expect(lastToast()).toMatch(/nube/);
    expect(useCustomize.getState().picking).toBe("main");
  });

  it("si falla, lo cuenta y sigue en el modo", async () => {
    backgroundImport.mockRejectedValue("Imagen dañada");
    await chooseBackground("C:\\a.png");
    expect(lastToast()).toBe("Imagen dañada");
    expect([useCustomize.getState().picking, useCustomize.getState().pickBusy]).toEqual(["main", false]);
  });

  it("fuera del modo no hace nada, y cerrar el panel lo termina", async () => {
    useCustomize.getState().closeAppearance();
    expect(useCustomize.getState().picking).toBeNull();
    await chooseBackground("C:\\a.png");
    expect(backgroundImport).not.toHaveBeenCalled();
  });
});

describe("sin selectores duplicados", () => {
  const src = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8");
  it("el panel no abre el diálogo de archivos de Windows", () => {
    const panel = src("../components/AppearancePanel.tsx");
    expect(panel).not.toMatch(/type="file"/);
    expect(panel).toMatch(/startPick\(slot\)/);
  });
  it("abrir un archivo en el explorador pasa por la elección de fondo", () => {
    expect(src("../hooks/useFileActions.ts")).toMatch(/picking\) void chooseBackground/);
  });
});

describe("esquinas redondeadas con imagen de fondo", () => {
  const css = readFileSync(new URL("../styles/app.css", import.meta.url), "utf8");
  it("ninguna zona entre paneles deja ver la imagen sin velo", () => {
    expect(css).toMatch(/html\[data-bg-general\] \.panes::before, html\[data-bg-general\] \.pane-split::after \{[^}]*radial-gradient\(circle at 100% 100%, transparent calc\(var\(--r-l\) - 0\.5px\), var\(--cz-veil\) var\(--r-l\)\)/);
    expect(css).toMatch(/html\[data-bg-general\] \.pane-split \{ background: var\(--cz-veil\); \}/);
  });
});
