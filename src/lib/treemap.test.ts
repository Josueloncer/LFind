import { describe, expect, it } from "vitest";
import { squarify } from "./treemap";

describe("squarify", () => {
  const items = [6, 6, 4, 3, 2, 2, 1].map((value, i) => ({ value, i }));
  const out = squarify(items, 600, 400);

  it("el área de cada bloque es proporcional a su valor y cubre todo", () => {
    const total = items.reduce((a, b) => a + b.value, 0);
    for (const r of out) expect(r.w * r.h).toBeCloseTo((r.value / total) * 600 * 400, 3);
    expect(out.reduce((a, r) => a + r.w * r.h, 0)).toBeCloseTo(240000, 3);
  });
  it("no se sale del área ni se solapa", () => {
    for (const r of out) {
      expect(r.x).toBeGreaterThanOrEqual(-1e-6);
      expect(r.y).toBeGreaterThanOrEqual(-1e-6);
      expect(r.x + r.w).toBeLessThanOrEqual(600 + 1e-6);
      expect(r.y + r.h).toBeLessThanOrEqual(400 + 1e-6);
    }
    for (const a of out) for (const b of out) {
      if (a === b) continue;
      const overlap = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x) > 1e-6 && Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y) > 1e-6;
      expect(overlap).toBe(false);
    }
  });
  it("ignora valores nulos y áreas vacías", () => {
    expect(squarify([{ value: 0 }, { value: -2 }], 10, 10)).toEqual([]);
    expect(squarify([{ value: 1 }], 0, 10)).toEqual([]);
    expect(squarify([], 10, 10)).toEqual([]);
  });
  it("un solo elemento ocupa todo y la entrada no se altera", () => {
    const src = [{ value: 1 }, { value: 5 }];
    expect(squarify([{ value: 3 }], 50, 20)[0]).toMatchObject({ x: 0, y: 0, w: 50, h: 20 });
    squarify(src, 10, 10);
    expect(src[0].value).toBe(1);
  });
});
