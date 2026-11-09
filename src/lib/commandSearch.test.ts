import { describe, expect, it, vi } from "vitest";
import { MAX_RESULTS, looksLikePath, prepare, searchCommands } from "./commandSearch";
import type { Command } from "./commandSearch";
import { allCommands, queryCommands, registerCommands, usePalette } from "../stores/palette";

const cmd = (id: string, title: string, o: Partial<Command> = {}): Command => ({ id, title, section: "S", run: () => {}, ...o });
const all = prepare([cmd("a", "Nueva pestaña"), cmd("b", "Cerrar pestaña"), cmd("c", "Vista: Detalles"), cmd("d", "Personalización", { keywords: "fondo colores" }), cmd("e", "Actualizar carpeta")]);
const ids = (q: string, recent: string[] = []) => searchCommands(all, q, recent).map((c) => c.id);

describe("búsqueda de comandos", () => {
  it("no distingue acentos ni mayúsculas", () => {
    expect(ids("PESTANA")).toEqual(["a", "b"]);
    expect(ids("personalizacion")).toEqual(["d"]);
  });
  it("el principio del título gana a lo demás", () => {
    expect(ids("c")[0]).toBe("b");
  });
  it("encuentra por palabras clave", () => {
    expect(ids("fondo")).toEqual(["d"]);
  });
  it("todas las palabras deben aparecer", () => {
    expect(ids("nueva pest")).toEqual(["a"]);
    expect(ids("nueva carpeta")).toEqual([]);
  });
  it("sin texto, primero lo reciente", () => {
    expect(ids("", ["e", "c"]).slice(0, 3)).toEqual(["e", "c", "a"]);
  });
  it("limita los resultados", () => {
    const many = prepare(Array.from({ length: 500 }, (_, i) => cmd(`x${i}`, `Comando ${i}`)));
    expect(searchCommands(many, "comando")).toHaveLength(MAX_RESULTS);
  });
  it("reconoce rutas", () => {
    for (const p of ["C:\\", "d:/x", "\\\\srv\\a", "C:"]) expect(looksLikePath(p)).toBe(true);
    for (const p of ["vista", "nueva c:"]) expect(looksLikePath(p)).toBe(false);
  });
  it("10 000 comandos responden al instante", () => {
    const many = prepare(Array.from({ length: 10_000 }, (_, i) => cmd(`x${i}`, `Carpeta número ${i}`, { subtitle: "C:\\Users\\ana\\Documentos" })));
    const t = performance.now();
    for (const q of ["c", "ca", "car", "carp", "carpeta 99"]) searchCommands(many, q);
    expect(performance.now() - t).toBeLessThan(200);
  });
});

describe("registro de comandos", () => {
  it("cualquier módulo puede añadir comandos y retirarlos", () => {
    const off = registerCommands({ all: () => [cmd("ext", "Extra")], forQuery: (q) => (q === "x" ? [cmd("dyn", "Dinámico")] : []) });
    expect(allCommands().some((c) => c.id === "ext")).toBe(true);
    expect(queryCommands("x").map((c) => c.id)).toContain("dyn");
    off();
    expect(allCommands().some((c) => c.id === "ext")).toBe(false);
  });
  it("no consulta a los proveedores hasta que se abre", () => {
    const fn = vi.fn(() => []);
    const off = registerCommands({ all: fn });
    usePalette.getState().toggle();
    usePalette.getState().toggle();
    expect(fn).not.toHaveBeenCalled();
    off();
  });
  it("recuerda lo usado sin repetir y con límite", () => {
    usePalette.setState({ recent: [] });
    for (const id of ["a", "b", "a", "c", "d", "e", "f", "g", "h", "i"]) usePalette.getState().used(id);
    expect(usePalette.getState().recent[0]).toBe("i");
    expect(usePalette.getState().recent).toHaveLength(8);
  });
});
