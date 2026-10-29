import { describe, expect, it } from "vitest";
import type { Entry } from "./fs";
import { sortEntries } from "./sort";

const f = (name: string, size = 0, modified: number | null = 0, kind: Entry["kind"] = "file"): Entry => ({
  name, kind, size, modified, created: modified, hidden: false, ext: name.includes(".") ? name.split(".").pop()! : null, link: false,
});

describe("sortEntries", () => {
  const list = [f("b.txt", 5), f("A10.txt", 1), f("a2.txt", 9), f("zeta", 0, 0, "dir"), f("alfa", 0, 0, "dir")];

  it("carpetas primero y orden natural por nombre", () => {
    expect(sortEntries(list, "name", "asc").map((e) => e.name)).toEqual(["alfa", "zeta", "a2.txt", "A10.txt", "b.txt"]);
  });
  it("descendente mantiene las carpetas arriba", () => {
    expect(sortEntries(list, "name", "desc").map((e) => e.name)).toEqual(["zeta", "alfa", "b.txt", "A10.txt", "a2.txt"]);
  });
  it("por tamaño", () => {
    expect(sortEntries(list, "size", "desc").filter((e) => e.kind === "file").map((e) => e.name)).toEqual(["a2.txt", "b.txt", "A10.txt"]);
  });
  it("no modifica la lista original", () => {
    const copy = [...list];
    sortEntries(list, "size", "asc");
    expect(list).toEqual(copy);
  });
});
