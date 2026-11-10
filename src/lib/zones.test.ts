import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { DEFAULT_APPEARANCE, LIMITS, appearanceVars, clampSidebarWidth, isDefaultAppearance, normalizeAppearance, normalizeThemes, sidebarWidthAt } from "./appearance";

const IMG = "0123456789abcdef";
const zone = (blur: number, transparency: number, blurOn = true) => ({ blurOn, blur, transparency });

describe("vuelta al desenfoque conjunto", () => {
  it("quien personalizó por zonas conserva lo que veía en el contenido", () => {
    const a = normalizeAppearance({ blur: 14, surface: 82, backgrounds: { general: IMG }, zones: { sidebar: zone(40, 9), main: zone(22, 12), secondary: zone(3, 30), floating: zone(16, 20) } });
    expect([a.blur, a.surface, a.backgrounds.general]).toEqual([22, 88, IMG]);
  });
  it("sin desenfoque en el contenido, sin desenfoque conjunto", () => {
    expect(normalizeAppearance({ blur: 14, zones: { main: zone(30, 18, false) } }).blur).toBe(0);
  });
  it("no deja ningún valor de las zonas", () => {
    const a = normalizeAppearance({ zones: { main: zone(20, 10) } });
    expect("zones" in a).toBe(false);
    expect(Object.keys(appearanceVars(a, () => 10)).some((k) => k.startsWith("--z-"))).toBe(false);
  });
  it("corrige valores dañados o fuera de límites", () => {
    expect(normalizeAppearance({ zones: { main: { blur: 999, transparency: 90 } } })).toMatchObject({ blur: 40, surface: 68 });
    expect(normalizeAppearance({ blur: 20, surface: 75, zones: { main: 7 } })).toMatchObject({ blur: 20, surface: 75 });
  });
  it("migrar dos veces da lo mismo", () => {
    const once = normalizeAppearance({ zones: { main: zone(22, 12) } });
    expect(normalizeAppearance(JSON.parse(JSON.stringify(once)))).toEqual(once);
  });
  it("los temas guardados también se migran", () => {
    const [t] = normalizeThemes([{ id: "a", name: "Mío", appearance: { zones: { main: zone(9, 4) } } }]);
    expect([t.appearance.blur, t.appearance.surface]).toEqual([9, 96]);
  });
  it("sin datos sigue siendo la apariencia original", () => {
    expect(isDefaultAppearance(normalizeAppearance({}))).toBe(true);
  });
  it("ya no hay capas, filtros ni controles por zona", () => {
    const css = readFileSync(new URL("../styles/app.css", import.meta.url), "utf8");
    expect(css).not.toMatch(/data-z-|--z-(sidebar|main|secondary|floating)|backdrop-filter/);
    expect(readFileSync(new URL("../components/AppearancePanel.tsx", import.meta.url), "utf8")).not.toMatch(/ZONE|zones/);
  });
});

describe("ancho de la barra lateral", () => {
  const [lo, hi] = LIMITS.sidebarWidth;
  it("respeta los límites", () => {
    expect(clampSidebarWidth(10, 1600)).toBe(lo);
    expect(clampSidebarWidth(9000, 1600)).toBe(hi);
    expect(clampSidebarWidth(301.4, 1600)).toBe(301);
  });
  it("siempre deja sitio al contenido", () => {
    expect(clampSidebarWidth(480, 800)).toBe(380);
  });
  it("en una ventana mínima no baja del ancho mínimo", () => {
    expect(clampSidebarWidth(300, 500)).toBe(lo);
  });
  it("mide desde el borde en el que esté la barra", () => {
    expect(sidebarWidthAt(300, "left", 1600)).toBe(300);
    expect(sidebarWidthAt(1300, "right", 1600)).toBe(300);
  });
  it("un ancho guardado fuera de los límites se corrige", () => {
    expect(normalizeAppearance({ sidebarWidth: 9000 }).sidebarWidth).toBe(hi);
  });
  it("no cambia sin una imagen ni un valor de apariencia", () => {
    expect(DEFAULT_APPEARANCE.sidebarWidth).toBe(240);
  });
});
