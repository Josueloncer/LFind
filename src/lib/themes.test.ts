import { describe, expect, it } from "vitest";
import appCss from "../styles/app.css?raw";
import themesCss from "../styles/themes.css?raw";
import tokensCss from "../styles/tokens.css?raw";
import { THEMES, THEME_IDS, normalizeTheme, resolveTheme, schemeOf } from "./themes";

interface ThemeBlock {
  id: string;
  vars: Set<string>;
}

/**
 * Bloques `:root[data-theme="x"] { … }` de themes.css y las variables que declara cada uno. No depende del tipo de salto de línea
 * (LF, CRLF o CR), de la sangría ni de dónde se escriba la llave de cierre: los bloques no contienen `}` en su interior.
 */
function parseThemeBlocks(css: string): ThemeBlock[] {
  const blocks: ThemeBlock[] = [];
  for (const m of css.matchAll(/:root\[data-theme\s*=\s*["']?(\w+)["']?\]\s*\{([^}]*)\}/g)) {
    const vars = new Set<string>();
    for (const v of m[2].matchAll(/(?:^|[\s;{])(--[\w-]+)\s*:/g)) vars.add(v[1]);
    blocks.push({ id: m[1], vars });
  }
  return blocks;
}

const blocks = parseThemeBlocks(themesCss);

describe("temas", () => {
  it("hay un bloque CSS por cada tema salvo «system»", () => {
    expect(blocks.map((b) => b.id).sort()).toEqual(THEME_IDS.filter((t) => t !== "system").sort());
  });

  it("el analizador de themes.css admite LF, CRLF y CR con el mismo resultado", () => {
    const summary = (css: string) => parseThemeBlocks(css).map((b) => `${b.id}:${[...b.vars].sort().join(",")}`);
    const lf = themesCss.replace(/\r\n?/g, "\n");
    expect(summary(lf)).toHaveLength(5);
    expect(summary(lf.replace(/\n/g, "\r\n"))).toEqual(summary(lf));
    expect(summary(lf.replace(/\n/g, "\r"))).toEqual(summary(lf));
    expect(summary(":root[data-theme=\"dark\"] {\r\n  color-scheme: dark;\r\n  --a: 1;\r\n  --b-c: var(--a);\r\n}\r\n")).toEqual(["dark:--a,--b-c"]);
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
