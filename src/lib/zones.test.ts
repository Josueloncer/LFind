import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { DEFAULT_APPEARANCE, LIMITS, ZONES, appearanceVars, clampSidebarWidth, isDefaultAppearance, normalizeAppearance, sidebarWidthAt, zoneBakes, zoneLayer, zonesFrom } from "./appearance";
import type { Appearance, Zone } from "./appearance";

const IMG = "0123456789abcdef";
const OTHER = "fedcba9876543210";
const base = () => 10;
const withImage = (patch: Partial<Appearance> = {}): Appearance => ({ ...DEFAULT_APPEARANCE, backgrounds: { ...DEFAULT_APPEARANCE.backgrounds, general: IMG }, ...patch });
const set = (a: Appearance, z: Zone, patch: Partial<Appearance["zones"][Zone]>): Appearance => ({ ...a, zones: { ...a.zones, [z]: { ...a.zones[z], ...patch } } });

describe("zonas: preferencias", () => {
  it("quien no tenía zonas conserva exactamente lo que veía", () => {
    const a = normalizeAppearance({ blur: 20, surface: 70 });
    expect(a.zones.main).toEqual({ blurOn: true, blur: 20, transparency: 30 });
    expect(a.zones.secondary).toEqual({ blurOn: true, blur: 20, transparency: 30 });
    expect(a.zones.sidebar).toEqual({ blurOn: true, blur: 20, transparency: 15 });
    expect(a.zones.floating).toEqual({ blurOn: false, blur: 14, transparency: 0 });
  });
  it("sin datos sigue siendo la apariencia original", () => {
    expect(isDefaultAppearance(normalizeAppearance({}))).toBe(true);
  });
  it("cada zona se guarda y se recupera por separado", () => {
    const a = set(set(DEFAULT_APPEARANCE, "sidebar", { blurOn: false, blur: 33, transparency: 5 }), "secondary", { blur: 2, transparency: 25 });
    const back = normalizeAppearance(JSON.parse(JSON.stringify(a)));
    expect(back.zones).toEqual(a.zones);
    expect(back.zones.main).toEqual(DEFAULT_APPEARANCE.zones.main);
  });
  it("corrige cada valor dañado sin perder los demás", () => {
    const a = normalizeAppearance({ zones: { sidebar: { blurOn: "sí", blur: 999, transparency: 99 }, main: 7, floating: { transparency: 90 } } });
    expect(a.zones.sidebar).toEqual({ blurOn: true, blur: 40, transparency: 32 });
    expect(a.zones.main).toEqual(DEFAULT_APPEARANCE.zones.main);
    expect(a.zones.floating.transparency).toBe(25);
  });
});

describe("zonas: variables CSS", () => {
  it("cambiar una zona solo cambia sus variables", () => {
    const before = appearanceVars(DEFAULT_APPEARANCE, base);
    const after = appearanceVars(set(DEFAULT_APPEARANCE, "sidebar", { blur: 30, transparency: 2 }), base);
    const changed = Object.keys(after).filter((k) => after[k] !== before[k]).sort();
    expect(changed).toEqual(["--z-sidebar-blur", "--z-sidebar-surface"]);
    expect(after["--z-sidebar-blur"]).toBe("30px");
    expect(after["--z-sidebar-surface"]).toBe("98%");
  });
  it("con el desenfoque apagado la intensidad se conserva pero no se aplica", () => {
    const a = set(DEFAULT_APPEARANCE, "main", { blurOn: false, blur: 22 });
    expect(appearanceVars(a, base)["--z-main-blur"]).toBe("0px");
    expect(a.zones.main.blur).toBe(22);
  });
});

describe("zonas: capas", () => {
  it("sin imagen no hay ninguna capa", () => {
    for (const z of ZONES) expect(zoneLayer(DEFAULT_APPEARANCE, z)).toBeNull();
  });
  it("una zona con el desenfoque de la ventana no duplica la capa", () => {
    const a = withImage();
    expect(zoneLayer(a, "sidebar")).toBeNull();
    expect(zoneBakes(a)).toEqual([]);
  });
  it("una zona distinta usa una copia alineada con la ventana", () => {
    expect(zoneLayer(set(withImage(), "main", { blur: 30 }), "main")).toEqual({ id: IMG, blur: 30, fixed: true });
  });
  it("sin desenfoque usa la imagen nítida", () => {
    expect(zoneLayer(set(withImage(), "sidebar", { blurOn: false }), "sidebar")).toEqual({ id: IMG, blur: 0, fixed: true });
  });
  it("la imagen propia de la zona tiene preferencia y se ajusta a ella", () => {
    const a = withImage({ backgrounds: { ...DEFAULT_APPEARANCE.backgrounds, general: IMG, sidebar: OTHER } });
    expect(zoneLayer(a, "sidebar")).toEqual({ id: OTHER, blur: 14, fixed: false });
  });
  it("los dos paneles comparten la imagen de contenido, cada uno con su desenfoque", () => {
    const a = set(withImage({ backgrounds: { ...DEFAULT_APPEARANCE.backgrounds, main: OTHER } }), "secondary", { blur: 3 });
    expect(zoneLayer(a, "main")?.id).toBe(OTHER);
    expect(zoneLayer(a, "secondary")).toEqual({ id: OTHER, blur: 3, fixed: false });
  });
  it("lo flotante nunca lleva imagen preparada", () => {
    expect(zoneLayer(set(withImage(), "floating", { blurOn: true, blur: 30, transparency: 20 }), "floating")).toBeNull();
  });
  it("no prepara dos veces la misma imagen", () => {
    const a = set(set(withImage(), "main", { blur: 30 }), "secondary", { blur: 30 });
    expect(zoneBakes(a)).toEqual([{ id: IMG, blur: 30 }]);
  });
  it("los estilos calculan las zonas igual que antes los ajustes globales", () => {
    expect(zonesFrom(26, 70).main).toEqual({ blurOn: true, blur: 26, transparency: 30 });
  });
});

describe("zonas: hoja de estilos", () => {
  const css = readFileSync(new URL("../styles/app.css", import.meta.url), "utf8");
  it("el desenfoque en vivo queda reservado a lo flotante y solo si se pide", () => {
    const rules = css.split("\n").filter((l) => /backdrop-filter/.test(l) && !l.trim().startsWith("/*"));
    expect(rules).toHaveLength(1);
    expect(rules[0]).toContain('html[data-z-floating="blur"]');
  });
  it("el panel secundario tiene sus propios valores", () => {
    expect(css).toMatch(/main:not\(:first-of-type\) \{ background: color-mix\(in srgb, var\(--bg-content\) var\(--z-secondary-surface\)/);
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
});
