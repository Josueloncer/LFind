import { describe, expect, it } from "vitest";
import { crumbsFor, driveLabel, outcomeFor } from "./addressBar";
import { COMPUTER } from "./path";

describe("barra de dirección: migas", () => {
  it("Windows: Mi equipo › Disco local (C:) › … con la ruta de cada segmento", () => {
    const c = crumbsFor(String.raw`C:\Users\Usuario\Downloads`);
    expect(c.map((x) => x.label)).toEqual(["Mi equipo", "Disco local (C:)", "Users", "Usuario", "Downloads"]);
    expect(c.map((x) => x.path)).toEqual([COMPUTER, "C:\\", String.raw`C:\Users`, String.raw`C:\Users\Usuario`, String.raw`C:\Users\Usuario\Downloads`]);
  });
  it("usa la etiqueta del volumen y su tipo", () => {
    expect(driveLabel("d", { label: "Datos", kind: "fixed" })).toBe("Datos (D:)");
    expect(driveLabel("e", { label: null, kind: "removable" })).toBe("Disco extraíble (E:)");
    expect(crumbsFor(String.raw`D:\x`, { ["D:\\"]: { label: "Datos", kind: "fixed" } })[1].label).toBe("Datos (D:)");
  });
  it("raíz de unidad: solo Mi equipo › Disco local", () => {
    expect(crumbsFor("C:\\").map((x) => x.label)).toEqual(["Mi equipo", "Disco local (C:)"]);
  });
  it("red: el primer segmento es \\\\servidor\\recurso", () => {
    const c = crumbsFor(String.raw`\\srv\share\a\b`);
    expect(c.map((x) => x.label)).toEqual([String.raw`\\srv\share`, "a", "b"]);
    expect(c[0].path).toBe(String.raw`\\srv\share`);
    expect(c[2].path).toBe(String.raw`\\srv\share\a\b`);
  });
  it("POSIX y vacío", () => {
    expect(crumbsFor("/usr/lib").map((x) => x.label)).toEqual(["/", "usr", "lib"]);
    expect(crumbsFor("")).toEqual([]);
  });
});

describe("barra de dirección: resultado de Enter", () => {
  it("misma carpeta → solo recarga (sin ensuciar el historial)", () => {
    expect(outcomeFor(String.raw`C:\Users`, { type: "dir", path: "c:\\users\\" })).toEqual({ action: "reload" });
  });
  it("otra carpeta → navegar", () => {
    expect(outcomeFor(String.raw`C:\a`, { type: "dir", path: String.raw`D:\b` })).toEqual({ action: "navigate", path: String.raw`D:\b`, note: undefined });
  });
  it("un archivo → abre su carpeta y lo avisa", () => {
    const o = outcomeFor(String.raw`C:\a`, { type: "file", dir: String.raw`C:\a`, file: "x.txt" });
    expect(o).toMatchObject({ action: "navigate", path: String.raw`C:\a` });
    expect((o as { note: string }).note).toContain("x.txt");
  });
});
