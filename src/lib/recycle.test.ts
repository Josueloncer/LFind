import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { confirmText, filterRecycle, progressDetail, progressValue, purgeSummary, recycleBytes, sortRecycle } from "./recycle";
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

describe("resultado de eliminar", () => {
  const failed = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `${i}`, name: `f${i}`, reason: "en uso" }));
  it("solo es limpio si se borró todo", () => {
    expect(purgeSummary({ deleted: 3, failed: [], notAttempted: 0, cancelled: false }).clean).toBe(true);
    expect(purgeSummary({ deleted: 2, failed: failed(1), notAttempted: 0, cancelled: false }).clean).toBe(false);
    expect(purgeSummary({ deleted: 3, failed: [], notAttempted: 0, cancelled: true }).clean).toBe(false);
  });
  it("dice cuántos quedan cuando Windows no deja borrar", () => {
    const s = purgeSummary({ deleted: 1, failed: failed(2), notAttempted: 0, cancelled: false });
    expect(s.title).toBe("No se pudo eliminar todo");
    expect(s.detail).toBe("Se eliminaron 1 elemento. 2 elementos siguen en la Papelera:");
  });
  it("al cancelar cuenta lo hecho y lo pendiente", () => {
    const s = purgeSummary({ deleted: 4, failed: [], notAttempted: 1, cancelled: true });
    expect(s.title).toBe("Eliminación cancelada");
    expect(s.detail).toBe("Se eliminaron 4 elementos. 1 elemento sigue en la Papelera.");
  });
  it("la pantalla no da por eliminado nada sin mirar el resultado", () => {
    const src = readFileSync(new URL("../components/RecycleView.tsx", import.meta.url), "utf8");
    expect(src).toMatch(/purgeSummary\(r\)\.clean/);
  });
});

describe("progreso real", () => {
  const p = (o: object) => ({ done: 2, total: 5, files: 0, filesTotal: null, current: "c", whole: false, ...o });
  it("una carpeta a medias cuenta la parte que lleva", () => {
    expect(progressValue(p({ files: 50, filesTotal: 200 }))).toBeCloseTo(2.25);
    expect(progressValue(p({ files: 999, filesTotal: 200 }))).toBe(3);
  });
  it("sin cifra total solo cuenta elementos terminados", () => {
    expect(progressValue(p({ files: 50 }))).toBe(2);
  });
  it("muestra cuánto lleva la carpeta en curso", () => {
    expect(progressDetail(p({ whole: true, files: 1500, filesTotal: 2000 }))).toMatch(/^c · 1500 de 2000 archivos$/);
    expect(progressDetail(p({ whole: false, files: 3 }))).toBe("c");
  });
});

describe("listas desplegables", () => {
  const src = (f: string) => readFileSync(new URL(`../components/${f}`, import.meta.url), "utf8");
  it("ningún componente usa el desplegable del navegador", () => {
    for (const f of ["SearchView.tsx", "StorageView.tsx", "AppearancePanel.tsx", "DuplicatesView.tsx", "RecycleView.tsx"]) expect(src(f), f).not.toMatch(/<select\b/);
  });
});
