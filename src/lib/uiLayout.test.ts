import { describe, expect, it } from "vitest";
import { DEFAULT_LAYOUT } from "./fs/types";
import { applyOrder, moveByKey, moveKey } from "./reorder";
import { SECTION_IDS, TOOLS, isDefaultLayout, locationOrder, needsSeparator, normalizeLayout, sectionOrder, toggleTool, toolOrder, toolZones } from "./uiLayout";

describe("applyOrder", () => {
  const items = ["a", "b", "c", "d"];
  it("respeta lo guardado y deja lo nuevo después, en su orden", () => {
    expect(applyOrder(items, (x) => x, ["c", "a"])).toEqual(["c", "a", "b", "d"]);
  });
  it("sin orden guardado no cambia nada y no altera la entrada", () => {
    const copy = [...items];
    expect(applyOrder(items, (x) => x, [])).toEqual(items);
    applyOrder(items, (x) => x, ["d"]);
    expect(items).toEqual(copy);
  });
  it("ignora claves que ya no existen y repetidas", () => {
    expect(applyOrder(items, (x) => x, ["z", "b", "b", "a"])).toEqual(["b", "a", "c", "d"]);
  });
});

describe("moveByKey", () => {
  const l = ["a", "b", "c"];
  it("mueve delante de otro o al final", () => {
    expect(moveKey(l, "c", "a")).toEqual(["c", "a", "b"]);
    expect(moveKey(l, "a", null)).toEqual(["b", "c", "a"]);
    expect(moveKey(l, "a", "c")).toEqual(["b", "a", "c"]);
  });
  it("sobre sí mismo, a su sitio o a un destino inexistente no cambia la lista", () => {
    expect(moveByKey(l, (x) => x, "a", "a")).toBe(l);
    expect(moveByKey(l, (x) => x, "a", "b")).toBe(l);
    expect(moveByKey(l, (x) => x, "c", null)).toBe(l);
    expect(moveByKey(l, (x) => x, "a", "z")).toBe(l);
    expect(moveByKey(l, (x) => x, "z", "a")).toBe(l);
  });
  it("conserva los objetos: no se duplica ni se pierde nada", () => {
    const o = [{ k: "1", at: 5 }, { k: "2", at: 9 }, { k: "3", at: 1 }];
    const out = moveByKey(o, (x) => x.k, "3", "1");
    expect(out.map((x) => x.k)).toEqual(["3", "1", "2"]);
    expect(out[0]).toBe(o[2]);
    expect(out.length).toBe(3);
  });
});

describe("distribución", () => {
  it("por defecto mantiene la disposición original", () => {
    expect(sectionOrder(DEFAULT_LAYOUT)).toEqual([...SECTION_IDS]);
    expect(toolOrder(DEFAULT_LAYOUT)).toEqual(TOOLS.map((t) => t.id));
    expect(isDefaultLayout(DEFAULT_LAYOUT)).toBe(true);
  });
  it("un dato dañado se convierte en la distribución por defecto", () => {
    for (const bad of [null, 7, "x", [], { sections: 3, toolbar: "a", hiddenTools: { a: 1 } }]) expect(normalizeLayout(bad)).toEqual(DEFAULT_LAYOUT);
  });
  it("descarta tipos erróneos, repetidos y herramientas que no se pueden ocultar", () => {
    const l = normalizeLayout({ sections: ["computer", 3, "computer"], hiddenTools: ["new", "filter", "address", "inventada"] });
    expect(l.sections).toEqual(["computer"]);
    expect(l.hiddenTools).toEqual(["new"]);
  });
  it("una sección desconocida se ignora y una nueva aparece igualmente", () => {
    expect(sectionOrder({ ...DEFAULT_LAYOUT, sections: ["futura", "storage"] })).toEqual(["storage", "locations", "recents", "computer"]);
  });
  it("mover herramientas de lado cambia su zona", () => {
    const order = moveKey(toolOrder(DEFAULT_LAYOUT), "sort", "panel");
    const { left, right } = toolZones(order);
    expect(left[0]).toBe("sort");
    expect(right).not.toContain("sort");
  });
  it("ocultar y mostrar; el filtro y la barra de dirección no se ocultan", () => {
    const off = toggleTool(DEFAULT_LAYOUT, "new");
    expect(off.hiddenTools).toEqual(["new"]);
    expect(toggleTool(off, "new").hiddenTools).toEqual([]);
    expect(toggleTool(DEFAULT_LAYOUT, "filter")).toBe(DEFAULT_LAYOUT);
    expect(toggleTool(DEFAULT_LAYOUT, "address")).toBe(DEFAULT_LAYOUT);
  });
  it("separadores solo entre grupos distintos", () => {
    expect(needsSeparator("back", "forward")).toBe(false);
    expect(needsSeparator("view", "hidden")).toBe(true);
  });
});

describe("orden de Ubicaciones", () => {
  it("sin orden guardado: fijadas arriba y luego el sistema", () => {
    expect(locationOrder(["p1", "p2"], ["inicio", "docs"], [])).toEqual(["p1", "p2", "inicio", "docs"]);
  });
  it("una sola carpeta fijada se puede colocar entre ubicaciones del sistema", () => {
    const ids = locationOrder(["p"], ["a", "b", "c"], []);
    const moved = moveKey(ids, "p", "c");
    expect(moved).toEqual(["a", "b", "p", "c"]);
    expect(locationOrder(["p"], ["a", "b", "c"], moved)).toEqual(moved);
  });
  it("un orden guardado solo con ubicaciones del sistema deja la carpeta fijada arriba", () => {
    expect(locationOrder(["p"], ["a", "b"], ["b", "a"])).toEqual(["p", "b", "a"]);
  });
  it("una carpeta fijada nueva aparece arriba; una ubicación nueva, al final; lo que ya no existe se ignora", () => {
    expect(locationOrder(["nueva", "p"], ["a"], ["a", "p", "fantasma"])).toEqual(["nueva", "a", "p"]);
    expect(locationOrder(["p"], ["a", "b"], ["p", "a"])).toEqual(["p", "a", "b"]);
  });
  it("no duplica ni pierde elementos", () => {
    const out = locationOrder(["p", "q"], ["a", "b"], ["b", "b", "q"]);
    expect([...out].sort()).toEqual(["a", "b", "p", "q"]);
  });
});
