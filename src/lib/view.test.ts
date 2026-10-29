import { describe, expect, it } from "vitest";
import { ICON_STEPS, iconGeometry, zoomStep } from "./view";

describe("zoomStep (Ctrl+rueda)", () => {
  it("sube por los tamaños de icono hasta el máximo y se detiene", () => {
    let v = { viewMode: "icons" as const, iconSize: 64 };
    const seen: number[] = [];
    for (let i = 0; i < 12; i++) {
      v = zoomStep(v, 1) as typeof v;
      seen.push(v.iconSize);
    }
    expect(seen.slice(0, 4)).toEqual([80, 96, 128, 160]);
    expect(seen[seen.length - 1]).toBe(256);
    expect(v.viewMode).toBe("icons");
  });

  it("al reducir pasa de iconos pequeños a lista y a detalles", () => {
    let v = zoomStep({ viewMode: "icons", iconSize: 48 }, -1);
    expect(v).toEqual({ viewMode: "icons", iconSize: 32 });
    v = zoomStep(v, -1);
    expect(v.viewMode).toBe("list");
    v = zoomStep(v, -1);
    expect(v.viewMode).toBe("details");
    expect(zoomStep(v, -1).viewMode).toBe("details");
  });

  it("al ampliar desde detalles se sube a lista y luego a iconos de 32 px", () => {
    const l = zoomStep({ viewMode: "details", iconSize: 64 }, 1);
    expect(l.viewMode).toBe("list");
    expect(zoomStep(l, 1)).toEqual({ viewMode: "icons", iconSize: 32 });
  });

  it("tamaños arbitrarios se ajustan al peldaño más cercano; mosaicos y contenido tienen salida", () => {
    expect(zoomStep({ viewMode: "icons", iconSize: 100 }, 1).iconSize).toBe(128);
    expect(zoomStep({ viewMode: "tiles", iconSize: 64 }, 1).viewMode).toBe("icons");
    expect(zoomStep({ viewMode: "content", iconSize: 64 }, 1).viewMode).toBe("list");
  });

  it("la geometría crece con el tamaño y los pequeños van en línea", () => {
    expect(iconGeometry(32).inline).toBe(true);
    expect(iconGeometry(96).inline).toBe(false);
    const widths = ICON_STEPS.filter((s) => !iconGeometry(s).inline).map((s) => iconGeometry(s).cellW);
    expect(widths).toEqual([...widths].sort((a, b) => a - b));
  });
});

import { LADDER_MAX, ladderAt, ladderPos } from "./view";
describe("escalera (control deslizante)", () => {
  it("ladderAt y ladderPos son inversas", () => {
    for (let i = 0; i <= LADDER_MAX; i++) expect(ladderPos(ladderAt(i, 64))).toBe(i);
    expect(ladderAt(-5, 64).viewMode).toBe("details");
    expect(ladderAt(99, 64).iconSize).toBe(256);
  });
});
