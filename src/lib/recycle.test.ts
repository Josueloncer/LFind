import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { confirmText, filterRecycle, recycleBytes, sortRecycle } from "./recycle";
import type { RecycleItem } from "./fs";

const it0 = (name: string, original: string, deleted: number, size: number | null = 10): RecycleItem => ({ id: name, name, original, deleted, size, entries: null });
const list = [it0("Informe final.pdf", "C:\\Users\\ana\\Documentos\\Informe final.pdf", 5), it0("fotos", "D:\\Música y fotos\\fotos", 9, null)];

describe("Papelera", () => {
  it("busca por nombre y ubicación, sin acentos", () => {
    expect(filterRecycle(list, "INFORME").map((x) => x.name)).toEqual(["Informe final.pdf"]);
    expect(filterRecycle(list, "musica").map((x) => x.name)).toEqual(["fotos"]);
    expect(filterRecycle(list, "documentos final")).toHaveLength(1);
    expect(filterRecycle(list, "")).toHaveLength(2);
  });
  it("lo más reciente primero, sin alterar la lista", () => {
    expect(sortRecycle(list).map((x) => x.deleted)).toEqual([9, 5]);
    expect(list[0].deleted).toBe(5);
  });
  it("suma lo que tiene tamaño conocido", () => {
    expect(recycleBytes(list)).toBe(10);
  });
  it("avisa de que no se puede deshacer", () => {
    expect(confirmText("purge", 3).detail).toMatch(/no se puede deshacer/);
    expect(confirmText("empty", 1).detail).toMatch(/1 elemento\. /);
  });
});

describe("listas desplegables", () => {
  const src = (f: string) => readFileSync(new URL(`../components/${f}`, import.meta.url), "utf8");
  it("ningún componente usa el desplegable del navegador", () => {
    for (const f of ["SearchView.tsx", "StorageView.tsx", "AppearancePanel.tsx", "DuplicatesView.tsx", "RecycleView.tsx"]) expect(src(f), f).not.toMatch(/<select\b/);
  });
});
