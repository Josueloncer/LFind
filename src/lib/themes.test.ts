import { describe, expect, it } from "vitest";
import appCss from "../styles/app.css?raw";
import themesCss from "../styles/themes.css?raw";
import tokensCss from "../styles/tokens.css?raw";
import { THEMES, THEME_IDS, normalizeTheme, resolveTheme, schemeOf } from "./themes";

const blocks = [...themesCss.matchAll(/:root\[data-theme="(\w+)"\] \{([\s\S]*?)\n\}/g)].map((m) => ({ id: m[1], vars: new Set([...m[2].matchAll(/^\s*(--[\w-]+):/gm)].map((v) => v[1])) }));

describe("temas", () => {
  it("hay un bloque CSS por cada tema salvo «system»", () => {
    expect(blocks.map((b) => b.id).sort()).toEqual(THEME_IDS.filter((t) => t !== "system").sort());
  });

  it("todos los temas definen exactamente las mismas variables", () => {
    const ref = blocks[0].vars;
    expect(ref.size).toBeGreaterThan(50);
    for (const b of blocks) {
      expect([...ref].filter((v) => !b.vars.has(v)), `${b.id}: faltan`).toEqual([]);
      expect([...b.vars].filter((v) => !ref.has(v)), `${b.id}: sobran`).toEqual([]);
    }
  });

  it("toda variable usada en app.css está definida en tokens.css, themes.css o el propio app.css", () => {
    const defined = new Set([...tokensCss.matchAll(/^\s*(--[\w-]+):/gm), ...themesCss.matchAll(/^\s*(--[\w-]+):/gm), ...appCss.matchAll(/(--[\w-]+):/g)].map((m) => m[1]));
    const used = [...appCss.matchAll(/var\((--[\w-]+)/g)].map((m) => m[1]);
    expect(used.filter((u) => !defined.has(u) && u !== "--dcols")).toEqual([]);
  });

  it("app.css no contiene colores ni sombras escritos a mano", () => {
    const noMask = appCss.replace(/mask-image:[^;]*;/g, "").replace(/-webkit-mask-image:[^;]*;/g, "");
    expect(noMask.match(/#[0-9a-fA-F]{3,8}\b/g)).toBeNull();
    expect(noMask.match(/rgba?\((?!var)/g)).toBeNull();
  });

  it("system se resuelve según el sistema; el resto no cambia", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    for (const t of THEME_IDS.filter((x) => x !== "system")) expect(resolveTheme(t, true)).toBe(t);
  });

  it("valores desconocidos caen a «system»", () => {
    expect(normalizeTheme("tema-del-futuro")).toBe("system");
    expect(normalizeTheme(undefined)).toBe("system");
    expect(normalizeTheme("hacker")).toBe("hacker");
  });

  it("esquema de la ventana nativa", () => {
    expect(schemeOf("system")).toBeNull();
    expect(schemeOf("cyberpunk")).toBe("dark");
    expect(schemeOf("hacker")).toBe("dark");
    expect(schemeOf("minimal")).toBe("light");
    expect(THEMES).toHaveLength(6);
  });
});
