import { describe, expect, it } from "vitest";
import type { Entry, SortDir, SortKey } from "./fs";
import { applyPermutation, sortEntries, toRequest } from "./sort";
import { handleSortRequest } from "./sortCore";
import { defaultSortDir, nextSort } from "./viewState";

describe("orden por fecha y sentido por defecto", () => {
  const e = (name: string, modified: number | null, kind: "file" | "dir" = "file") => ({ name, kind, size: 0, modified, created: modified, hidden: false, ext: null, link: false });
  const names = (l: { name: string }[]) => l.map((x) => x.name);
  const list = [e("viejo", 100), e("sin", null), e("nuevo", 300), e("medio", 200), e("carpeta", 1, "dir")];

  it("más recientes primero al descender, con carpetas y archivos mezclados y sin fecha al final", () => {
    expect(names(sortEntries(list, "modified", "desc"))).toEqual(["nuevo", "medio", "viejo", "carpeta", "sin"]);
    expect(names(sortEntries(list, "created", "desc"))).toEqual(["nuevo", "medio", "viejo", "carpeta", "sin"]);
  });
  it("al invertir, lo que no tiene fecha sigue al final", () => {
    expect(names(sortEntries(list, "modified", "asc"))).toEqual(["carpeta", "viejo", "medio", "nuevo", "sin"]);
  });
  it("respeta estrictamente las fechas: carpeta, archivo, carpeta, archivo", () => {
    const mix = [e("D", 10), e("C", 30, "dir"), e("B", 40), e("A", 50, "dir")];
    expect(names(sortEntries(mix, "modified", "desc"))).toEqual(["A", "B", "C", "D"]);
    expect(names(sortEntries(mix, "modified", "asc"))).toEqual(["D", "C", "B", "A"]);
  });
  it("con la misma fecha desempata por nombre en ambos sentidos y no altera la lista original", () => {
    const tie = [e("b", 5), e("a", 5, "dir"), e("c", 5)];
    const copy = [...tie];
    expect(names(sortEntries(tie, "modified", "desc"))).toEqual(["a", "b", "c"]);
    expect(names(sortEntries(tie, "modified", "asc"))).toEqual(["a", "b", "c"]);
    expect(tie).toEqual(copy);
  });
  it("con nombre y tamaño las carpetas siguen delante", () => {
    expect(names(sortEntries(list, "name", "asc"))[0]).toBe("carpeta");
    expect(names(sortEntries(list, "size", "desc"))[0]).toBe("carpeta");
  });
  it("repetir el criterio alterna el sentido indefinidamente; otro criterio usa su sentido natural", () => {
    let v = nextSort({ sortKey: "name", sortDir: "asc" }, "modified");
    expect(v).toEqual({ sortKey: "modified", sortDir: "desc" });
    v = nextSort(v, "modified");
    expect(v.sortDir).toBe("asc");
    v = nextSort(v, "modified");
    expect(v.sortDir).toBe("desc");
    expect(nextSort(v, "name")).toEqual({ sortKey: "name", sortDir: "asc" });
  });
  it("no depende del orden de llegada", () => {
    const rev = [...list].reverse();
    expect(names(sortEntries(rev, "modified", "desc"))).toEqual(names(sortEntries(list, "modified", "desc")));
  });
  it("cada columna tiene su sentido natural", () => {
    expect([defaultSortDir("modified"), defaultSortDir("created"), defaultSortDir("size"), defaultSortDir("name"), defaultSortDir("kind")]).toEqual(["desc", "desc", "desc", "asc", "asc"]);
  });
});

// Datos pseudoaleatorios deterministas (sin dependencias).
function lcg(seed: number) {
  let s = seed >>> 0;
  return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 2 ** 32);
}
const NAMES = ["árbol", "zorro", "Ñandú", "archivo", "Archivo", "a10", "a2", "_oculto", "1era", "informe final", "ñu"];
const EXTS = ["txt", "pdf", "jpg", null, "zip"];

function make(n: number): Entry[] {
  const r = lcg(42);
  return Array.from({ length: n }, (_, i) => {
    const ext = EXTS[Math.floor(r() * EXTS.length)];
    const dir = r() < 0.2;
    return {
      name: `${NAMES[Math.floor(r() * NAMES.length)]}_${i}${!dir && ext ? "." + ext : ""}`,
      kind: dir ? "dir" : "file",
      size: Math.floor(r() * 1e6),
      modified: r() < 0.1 ? null : Math.floor(r() * 1e12),
      created: r() < 0.1 ? null : Math.floor(r() * 1e12),
      hidden: false,
      ext: dir ? null : ext,
      link: false,
    } as Entry;
  });
}

describe("ordenación por Worker == ordenación síncrona", () => {
  const list = make(500);
  const cases: [SortKey, SortDir][] = [
    ["name", "asc"], ["name", "desc"], ["size", "asc"], ["size", "desc"],
    ["modified", "asc"], ["modified", "desc"], ["created", "asc"], ["created", "desc"], ["kind", "asc"], ["kind", "desc"],
  ];
  for (const [key, dir] of cases) {
    it(`${key} ${dir}`, () => {
      const viaWorker = applyPermutation(list, handleSortRequest(toRequest(1, list, key, dir)));
      expect(viaWorker.map((e) => e.name)).toEqual(sortEntries(list, key, dir).map((e) => e.name));
    });
  }

  it("con tamaño, las carpetas van primero", () => {
    const out = sortEntries(list, "size", "desc");
    const firstFile = out.findIndex((e) => e.kind !== "dir");
    expect(out.slice(firstFile).every((e) => e.kind !== "dir")).toBe(true);
  });

  it("acentos en español: 'árbol' no queda después de 'zorro'", () => {
    const l = ["zorro", "árbol", "arbol2"].map((name) => ({ name, kind: "file", size: 0, modified: 0, created: 0, hidden: false, ext: null, link: false }) as Entry);
    expect(sortEntries(l, "name", "asc").map((e) => e.name)).toEqual(["árbol", "arbol2", "zorro"]);
  });

  it("lista vacía y de un elemento", () => {
    expect(handleSortRequest(toRequest(1, [], "name", "asc")).length).toBe(0);
    expect(handleSortRequest(toRequest(1, make(1), "name", "asc"))).toEqual(new Uint32Array([0]));
  });
});
