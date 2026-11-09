import { describe, expect, it } from "vitest";
import { bakedSide } from "./backgroundImage";
import { DEFAULT_APPEARANCE, LIMITS, appearanceVars, contrastRatio, isDefaultAppearance, legibilityIssues, normalizeAppearance, normalizeThemes, onColor, rgbOf, rowDelta, diffVars, FONTS, MANAGED_VARS, customFont, fontChoices, fontLabel, fontStack } from "./appearance";
import { fitWithin } from "./backgroundImage";

const base = (n: string) => (n.startsWith("--r-") ? 8 : n.startsWith("--sp-") ? 10 : n.startsWith("--icon") ? 16 : 13);

describe("normalizar", () => {
  it("un dato dañado vuelve a los valores por defecto", () => {
    for (const bad of [null, 3, "x", [], { blur: "mucho", colors: 4, backgrounds: [], density: "enorme", font: "inventada" }]) expect(normalizeAppearance(bad)).toEqual(DEFAULT_APPEARANCE);
  });
  it("recorta al rango permitido, también la opacidad mínima que protege la lectura", () => {
    const a = normalizeAppearance({ surface: 5, blur: 999, radius: -4 });
    expect([a.surface, a.blur, a.radius]).toEqual([LIMITS.surface[0], LIMITS.blur[1], 0]);
  });
  it("solo acepta colores hexadecimales e identificadores de imagen válidos", () => {
    const a = normalizeAppearance({ colors: { accent: "#AABBCC", text: "url(x)" }, backgrounds: { general: "0123456789abcdef", main: "../../secreto" } });
    expect(a.colors.accent).toBe("#aabbcc");
    expect(a.colors.text).toBeNull();
    expect([a.backgrounds.general, a.backgrounds.main]).toEqual(["0123456789abcdef", null]);
  });
  it("temas guardados: descarta los inválidos y los repetidos", () => {
    const t = normalizeThemes([{ id: "a", name: " Mío ", appearance: { blur: 3 } }, { id: "a", name: "otro" }, { id: "b", name: "  " }, 7]);
    expect(t.length).toBe(1);
    expect([t[0].name, t[0].appearance.blur]).toEqual(["Mío", 3]);
    expect(normalizeThemes("no")).toEqual([]);
  });
});

describe("variables CSS", () => {
  it("por defecto no toca nada del tema", () => {
    const v = appearanceVars(DEFAULT_APPEARANCE, base);
    expect(Object.keys(v).filter((k) => !k.startsWith("--z-")).sort()).toEqual(["--cz-blur", "--cz-contrast", "--cz-intensity", "--cz-surface"]);
    expect(isDefaultAppearance(DEFAULT_APPEARANCE)).toBe(true);
  });
  it("escala sobre las medidas del tema vigente", () => {
    const v = appearanceVars({ ...DEFAULT_APPEARANCE, radius: 150, fontScale: 110, sidebarWidth: 300 }, base);
    expect(v["--r-s"]).toBe("12px");
    expect(v["--fs-m"]).toBe("14.3px");
    expect(v["--sidebar-w-open"]).toBe("300px");
  });
  it("el acento deriva su texto y su selección", () => {
    const v = appearanceVars({ ...DEFAULT_APPEARANCE, colors: { ...DEFAULT_APPEARANCE.colors, accent: "#ffcc00" } }, base);
    expect(v["--accent-rgb"]).toBe("255, 204, 0");
    expect(v["--on-accent"]).toBe("#0f1115");
    expect(v["--sel"]).toContain("255, 204, 0");
  });
  it("una selección propia manda sobre la derivada del acento", () => {
    const v = appearanceVars({ ...DEFAULT_APPEARANCE, colors: { ...DEFAULT_APPEARANCE.colors, accent: "#ffcc00", selection: "#00ff00" } }, base);
    expect(v["--sel"]).toContain("0, 255, 0");
  });
  it("densidad", () => {
    expect([rowDelta("compact"), rowDelta("normal"), rowDelta("comfortable")]).toEqual([-4, 0, 6]);
    expect(appearanceVars({ ...DEFAULT_APPEARANCE, density: "compact" }, base)["--cz-row"]).toBe("-4px");
  });
});

describe("legibilidad", () => {
  it("calcula el contraste como WCAG", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 1);
    expect(contrastRatio("#777777", "#777777")).toBeCloseTo(1, 5);
    expect(rgbOf("#102030")).toEqual([16, 32, 48]);
    expect(onColor("#111111")).toBe("#ffffff");
  });
  it("avisa de un texto ilegible y no molesta si se lee bien", () => {
    const a = { ...DEFAULT_APPEARANCE, colors: { ...DEFAULT_APPEARANCE.colors, text: "#333333" } };
    expect(legibilityIssues(a, "#222222").map((i) => i.key)).toEqual(["text"]);
    expect(legibilityIssues(a, "#ffffff")).toEqual([]);
  });
});

describe("imagen de fondo", () => {
  it("reduce sin deformar y nunca amplía", () => {
    expect(fitWithin(4000, 2000)).toEqual([1920, 960]);
    expect(fitWithin(1000, 3840)).toEqual([500, 1920]);
    expect(fitWithin(800, 600)).toEqual([800, 600]);
  });
});

describe("escrituras en la raíz", () => {
  it("solo se tocan las variables que cambian", () => {
    expect(diffVars({ "--a": "1", "--b": "2" }, { "--a": "1", "--b": "3", "--c": "4" })).toEqual({ set: [["--b", "3"], ["--c", "4"]], remove: [] });
    expect(diffVars({ "--a": "1", "--b": "2" }, { "--a": "1" })).toEqual({ set: [], remove: ["--b"] });
    expect(diffVars({ "--a": "1" }, { "--a": "1" })).toEqual({ set: [], remove: [] });
  });
});

describe("tamaño de la imagen preparada", () => {
  it("cuanto más desenfoque, menos píxeles, con límites", () => {
    expect(bakedSide(14, 1920)).toBe(343);
    expect(bakedSide(40, 1920)).toBe(320);
    expect(bakedSide(2, 1920)).toBe(1920);
    expect(bakedSide(0, 3840)).toBe(1920);
    expect(bakedSide(14, 3840)).toBeLessThan(700);
  });
});

describe("tipografías instaladas", () => {
  it("guarda y restaura cualquier familia", () => {
    const id = customFont("Bahnschrift Light");
    const a = normalizeAppearance({ font: id });
    expect(a.font).toBe(id);
    expect(fontLabel(a.font)).toBe("Bahnschrift Light");
    expect(appearanceVars(a, () => 10)["--font"]).toBe('"Bahnschrift Light", "Segoe UI", system-ui, sans-serif');
  });

  it("conserva las opciones predefinidas ya guardadas", () => {
    expect(normalizeAppearance({ font: "arial" }).font).toBe("arial");
    expect(fontLabel("arial")).toBe("Arial");
    expect(normalizeAppearance({ font: "theme" }).font).toBe("theme");
  });

  it("descarta lo que rompería el CSS o no existe", () => {
    for (const bad of ['custom:a"; color: red', "custom:a\\b", "custom:", "custom:   ", "inventada", 7]) expect(normalizeAppearance({ font: bad }).font).toBe("theme");
    expect(fontStack('custom:x"y')).toBeNull();
  });

  it("el selector muestra la del tema y las instaladas, y filtra sin distinguir mayúsculas", () => {
    const all = fontChoices(["Arial", "Bahnschrift", "Wingdings"]);
    expect(all.map((f) => f.label)).toEqual(["La del tema", "Arial", "Bahnschrift", "Wingdings"]);
    expect(all[0].stack).toBeNull();
    expect(all[2].id).toBe("custom:Bahnschrift");
    expect(fontChoices(["Arial", "Bahnschrift"], " BAHN").map((f) => f.label)).toEqual(["Bahnschrift"]);
  });

  it("si no se pudieron consultar, ofrece las predefinidas", () => {
    expect(fontChoices([]).map((f) => f.id)).toEqual(FONTS.map((f) => f.id));
  });
});

describe("color de las carpetas", () => {
  const paint = (folder: string | null) => appearanceVars({ ...DEFAULT_APPEARANCE, colors: { ...DEFAULT_APPEARANCE.colors, folder } }, () => 10);

  it("sin elegir, manda el tema", () => {
    for (const k of Object.keys(paint(null))) expect(k).not.toMatch(/fold|cat-folder/);
  });

  it("colorea las dos caras del icono con el tono elegido", () => {
    const v = paint("#d9622b");
    expect(v["--fold-front-b"]).toBe("#d9622b");
    for (const k of ["--fold-front-a", "--fold-back-a", "--fold-back-b"]) expect(v[k]).toContain("#d9622b");
    expect(v["--cat-folder"]).toBe("#d9622b");
  });

  it("se guarda, y los datos antiguos o dañados vuelven al tema", () => {
    expect(normalizeAppearance({ colors: { folder: "#D9622B" } }).colors.folder).toBe("#d9622b");
    expect(normalizeAppearance({ colors: { accent: "#112233" } }).colors.folder).toBeNull();
    expect(normalizeAppearance({ colors: { folder: "naranja" } }).colors.folder).toBeNull();
  });

  it("al cambiar de tema se pueden retirar todas sus variables", () => {
    for (const k of Object.keys(paint("#d9622b"))) if (/fold/.test(k)) expect(MANAGED_VARS).toContain(k);
  });
});
