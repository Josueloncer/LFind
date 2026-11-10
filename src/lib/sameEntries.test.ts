import { describe, expect, it } from "vitest";
import { sameEntries } from "./sameEntries";
import type { Entry } from "./fs";

const e = (name: string, size = 1) => ({ name, kind: "file", size, modified: 5, created: 5, hidden: false }) as unknown as Entry;

describe("comparar dos lecturas de una carpeta", () => {
  it("iguales aunque sean listas distintas", () => {
    expect(sameEntries([e("a"), e("b")], [e("a"), e("b")])).toBe(true);
  });
  it("detecta un cambio de tamaño", () => {
    expect(sameEntries([e("a", 1)], [e("a", 2)])).toBe(false);
  });
  it("detecta altas, bajas y cambios de orden", () => {
    expect(sameEntries([e("a")], [e("a"), e("b")])).toBe(false);
    expect(sameEntries([e("a"), e("b")], [e("b"), e("a")])).toBe(false);
  });
});
