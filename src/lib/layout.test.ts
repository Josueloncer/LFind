import { describe, expect, it } from "vitest";
import type { Entry } from "./fs";
import { groupOf } from "./group";
import { buildLayout } from "./layout";
import { MAX_FOLDER_VIEWS, effectiveView, folderKey, withFolderView, withRecent } from "./viewState";
import { canDropInto, dropOperation } from "./dnd";
import { isInside, rootOf } from "./path";

const e = (name: string, o: Partial<Entry> = {}): Entry => ({ name, kind: "file", size: 100, modified: 1, created: 1, hidden: false, ext: name.split(".")[1] ?? null, link: false, ...o });
const NOW = new Date(2026, 9, 28, 15, 0, 0).getTime();
const DAY = 86_400_000;

describe("agrupación", () => {
  it("por tipo: carpetas primero y un grupo por tipo", () => {
    const l = buildLayout([e("a.txt"), e("b.png"), e("c", { kind: "dir", ext: null }), e("d.txt")], "type", 3, 30, 36, NOW);
    const heads = l.rows.filter((r) => r.kind === "header").map((r) => (r.kind === "header" ? r.label : ""));
    expect(heads[0]).toBe("Carpetas");
    expect(heads).toHaveLength(3);
    expect(l.flat[0].name).toBe("c");
    expect(l.flat.filter((x) => x.name.endsWith(".txt")).length).toBe(2);
  });

  it("por fecha: hoy, ayer, esta semana… en orden cronológico", () => {
    const l = buildLayout(
      [e("viejo", { modified: NOW - 400 * DAY }), e("hoy", { modified: NOW - 1000 }), e("ayer", { modified: NOW - DAY }), e("nulo", { modified: null })],
      "modified", 1, 30, 36, NOW,
    );
    expect(l.flat.map((x) => x.name)).toEqual(["hoy", "ayer", "viejo", "nulo"]);
    expect(groupOf(e("x", { modified: NOW - 3 * DAY }), "modified", NOW).label).toBe("Esta semana");
  });

  it("por tamaño y por nombre", () => {
    expect(groupOf(e("x", { size: 0 }), "size", NOW).label).toBe("Vacío");
    expect(groupOf(e("x", { size: 5_000 }), "size", NOW).label).toMatch(/Diminuto/);
    expect(groupOf(e("x", { size: 200 * 1024 * 1024 }), "size", NOW).label).toMatch(/Gigantesco/);
    expect(groupOf(e("árbol"), "name", NOW).label).toBe("Á");
    expect(groupOf(e("7zip"), "name", NOW).label).toBe("0–9");
    expect(groupOf(e("_x"), "name", NOW).label).toBe("Otros");
  });

  it("sin agrupar no hay cabeceras y las filas se trocean en `perRow`", () => {
    const l = buildLayout([1, 2, 3, 4, 5, 6, 7].map((i) => e(`f${i}`)), "none", 3, 100, 36, NOW);
    expect(l.rows.map((r) => (r.kind === "items" ? r.to - r.from : 0))).toEqual([3, 3, 1]);
    expect(l.itemRow).toEqual([0, 0, 0, 1, 1, 1, 2]);
  });

  it("cada grupo empieza una fila nueva y itemRow apunta a la fila correcta", () => {
    const l = buildLayout([e("a.txt"), e("b.txt"), e("c.png")], "type", 2, 100, 36, NOW);
    expect(l.rows.map((r) => r.kind)).toEqual(["header", "items", "header", "items"]);
    expect(l.rows[l.itemRow[2]]).toMatchObject({ kind: "items", from: 2, to: 3 });
    expect(buildLayout([], "type", 3, 30, 36, NOW).rows).toEqual([]);
  });
});

describe("vista por carpeta", () => {
  const base = { viewMode: "details" as const, iconSize: 64, sortKey: "name" as const, sortDir: "asc" as const, groupBy: "none" as const, folderViews: [] };

  it("usa lo predeterminado y solo recuerda lo que se cambia en esa carpeta", () => {
    const fv = withFolderView(base, "C:\\Fotos", { viewMode: "icons", iconSize: 128 });
    const p = { ...base, folderViews: fv };
    expect(effectiveView(p, "c:\\fotos\\")).toMatchObject({ viewMode: "icons", iconSize: 128, sortKey: "name" });
    expect(effectiveView(p, "C:\\Otra").viewMode).toBe("details");
  });

  it("actualizar una carpeta no duplica y conserva el resto de campos", () => {
    let fv = withFolderView(base, "C:\\A", { sortKey: "size" });
    fv = withFolderView({ ...base, folderViews: fv }, "C:\\A", { sortDir: "desc" });
    expect(fv).toHaveLength(1);
    expect(fv[0]).toMatchObject({ sortKey: "size", sortDir: "desc" });
  });

  it("se limita a las más recientes", () => {
    let fv: ReturnType<typeof withFolderView> = [];
    for (let i = 0; i < MAX_FOLDER_VIEWS + 20; i++) fv = withFolderView({ ...base, folderViews: fv }, `C:\\d${i}`, { iconSize: 80 });
    expect(fv).toHaveLength(MAX_FOLDER_VIEWS);
    expect(fv[fv.length - 1].path).toBe(`C:\\d${MAX_FOLDER_VIEWS + 19}`);
  });

  it("recientes: sin duplicados, más nuevo primero, tope", () => {
    let r: string[] = [];
    for (const p of ["C:\\a", "C:\\b", "c:\\A\\"]) r = withRecent(r, p);
    expect(r).toEqual(["c:\\A\\", "C:\\b"]);
    for (let i = 0; i < 40; i++) r = withRecent(r, `C:\\x${i}`);
    expect(r).toHaveLength(15);
    expect(folderKey("C:\\Dir\\")).toBe("c:\\dir");
  });
});

describe("arrastrar y soltar", () => {
  it("copia entre unidades y mueve dentro de la misma; Ctrl/Mayús lo fuerzan", () => {
    expect(dropOperation(["C:\\a\\f"], "C:\\b", { ctrlKey: false, shiftKey: false })).toBe("move");
    expect(dropOperation(["C:\\a\\f"], "D:\\b", { ctrlKey: false, shiftKey: false })).toBe("copy");
    expect(dropOperation(["C:\\a\\f"], "C:\\b", { ctrlKey: true, shiftKey: false })).toBe("copy");
    expect(dropOperation(["C:\\a\\f"], "D:\\b", { ctrlKey: false, shiftKey: true })).toBe("move");
  });

  it("no se puede soltar sobre sí mismo, dentro de sí mismo ni en su carpeta actual", () => {
    expect(canDropInto(["C:\\a\\f"], "C:\\a\\f")).toBe(false);
    expect(canDropInto(["C:\\a"], "C:\\a\\sub")).toBe(false);
    expect(canDropInto(["C:\\a\\f.txt"], "C:\\a")).toBe(false);
    expect(canDropInto(["C:\\a\\f.txt"], "C:\\b")).toBe(true);
    expect(canDropInto([], "C:\\b")).toBe(false);
  });

  it("rootOf e isInside", () => {
    expect(rootOf("C:\\x\\y")).toBe("c:");
    expect(rootOf("\\\\srv\\share\\d")).toBe("\\\\srv\\share");
    expect(rootOf("/home/a")).toBe("/");
    expect(isInside("C:\\a", "c:\\A\\b")).toBe(true);
    expect(isInside("C:\\a", "C:\\ab")).toBe(false);
  });
});
