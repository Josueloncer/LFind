// Regresión (act. 04): el nombre no debe superponerse a miniaturas verticales.
// Causa: `.thumb` era `display:grid` y su imagen usaba `max-height:100%`, un porcentaje cíclico que el navegador
// resolvía contra el contenido (la propia imagen) en vez de contra la caja → la imagen se salía y tapaba el nombre.
// El arreglo se verificó en Chromium con 4 proporciones × 8 tamaños (16/32 mal → 0/32); aquí se protege que no vuelva.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./app.css", import.meta.url), "utf8");
const rule = (sel: string) => {
  const m = css.match(new RegExp(`(?:^|\\n)\\s*${sel.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`));
  return m ? m[1] : "";
};

describe("miniaturas: la caja de la imagen", () => {
  it(".thumb es flex (no grid) y recorta lo que sobre", () => {
    const r = rule(".thumb");
    expect(/display:\s*flex/.test(r)).toBe(true);
    expect(/display:\s*grid/.test(r)).toBe(false);
    expect(/overflow:\s*hidden/.test(r)).toBe(true);
    expect(/min-width:\s*0/.test(r) && /min-height:\s*0/.test(r)).toBe(true);
  });
  it("la imagen conserva su proporción dentro de la caja", () => {
    const r = rule(".thumb img");
    expect(/max-width:\s*100%/.test(r) && /max-height:\s*100%/.test(r)).toBe(true);
    expect(/object-fit:\s*contain/.test(r)).toBe(true);
    expect(/width:\s*auto/.test(r) && /height:\s*auto/.test(r)).toBe(true);
  });
  it("los nombres largos no ensanchan la celda", () => {
    const r = rule(".gname");
    expect(/min-width:\s*0/.test(r)).toBe(true);
    expect(/overflow-wrap:\s*anywhere/.test(r)).toBe(true);
  });
});
