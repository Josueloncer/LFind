import { fitColumns } from "./view";
import { describe, expect, it } from "vitest";
import { ICON_100, LADDER_MAX, MAX_FRAME_RATIO, MAX_ICON, MIN_ICON, iconCells, iconGeometry, iconPercent, isWheelNotch, ladderAt, ladderOfIcon, ladderPos, nextExactSize, wheelUnits, zoomStep, zoomTo } from "./view";

const icons = (iconSize: number) => ({ viewMode: "icons" as const, iconSize });
/** Un gesto: cada evento es un fotograma. */
const gesture = (start: number, deltas: number[], pinch: boolean) => deltas.reduce((s, d) => nextExactSize(s, wheelUnits(d, 0, pinch)), start);

describe("zoom continuo", () => {
  it("un punto de la rueda cambia el tamaño de forma gradual, no a saltos", () => {
    const a = Math.round(nextExactSize(96, wheelUnits(-100, 0, false)));
    expect(a).toBeGreaterThan(96);
    expect(a).toBeLessThan(115);
    expect(nextExactSize(96, wheelUnits(100, 0, false))).toBeLessThan(96);
    expect(wheelUnits(-3, 1, false)).toBe(wheelUnits(-99, 0, false)); // rueda por líneas
  });
  it("un gesto lento del trackpad avanza en proporción, no un píxel por fotograma", () => {
    // 60 fotogramas con 0,5 de desplazamiento cada uno: 30 unidades en total, un 3,5 %.
    const slow = gesture(96, Array(60).fill(-0.5), false);
    expect(slow).toBeGreaterThan(96);
    expect(slow).toBeLessThan(100);
    // El mismo recorrido en 6 fotogramas da el mismo tamaño: no depende de cuántos eventos lleguen.
    expect(gesture(96, Array(6).fill(-5), false)).toBeCloseTo(slow, 6);
  });
  it("pinzar sigue a los dedos: pinzar al doble duplica los iconos", () => {
    // Chromium: deltaY = 100·ln(1/escala). Pinzar ×2 en 20 eventos.
    const step = (-100 * Math.log(2)) / 20;
    expect(gesture(64, Array(20).fill(step), true)).toBeCloseTo(128, 4);
    expect(gesture(128, Array(20).fill(-step), true)).toBeCloseTo(64, 4);
  });
  it("un evento brusco no da un salto", () => {
    expect(nextExactSize(96, wheelUnits(-5000, 0, false))).toBeCloseTo(96 * MAX_FRAME_RATIO, 6);
    expect(nextExactSize(96, wheelUnits(-60, 0, true))).toBeCloseTo(96 * MAX_FRAME_RATIO, 6);
  });
  it("distingue la rueda del trackpad", () => {
    expect(isWheelNotch(100, 0)).toBe(true);
    expect(isWheelNotch(-120, 0)).toBe(true);
    expect(isWheelNotch(3, 1)).toBe(true);
    expect(isWheelNotch(-2.5, 0)).toBe(false);
    expect(isWheelNotch(-14, 0)).toBe(false);
  });
  it("se detiene en los límites", () => {
    expect(nextExactSize(MAX_ICON, wheelUnits(-500, 0, false))).toBe(MAX_ICON);
    expect(nextExactSize(MIN_ICON + 1, wheelUnits(500, 0, false))).toBe(MIN_ICON);
    expect(zoomTo(96.6)).toEqual(icons(97));
    expect(zoomTo(1000).iconSize).toBe(MAX_ICON);
  });
  it("hay más de 11 tamaños distintos", () => {
    const seen = new Set<number>();
    for (let v = 2; v <= LADDER_MAX; v += 0.02) seen.add(ladderAt(v, 64).iconSize);
    expect(seen.size).toBeGreaterThan(150);
  });
  it("el control y el tamaño son coherentes", () => {
    for (const s of [32, 41, 64, 96, 137, 200, 256]) expect(ladderAt(ladderOfIcon(s), 64)).toEqual(icons(s));
    expect(ladderPos(icons(MIN_ICON))).toBe(2);
    expect(ladderPos(icons(MAX_ICON))).toBeCloseTo(LADDER_MAX);
  });
  it("muestra el porcentaje", () => {
    expect(iconPercent(ICON_100)).toBe(100);
    expect(iconPercent(48)).toBe(50);
    expect(iconPercent(MAX_ICON)).toBe(267);
  });
  it("los extremos del control siguen siendo detalles y lista", () => {
    expect(ladderAt(0, 64).viewMode).toBe("details");
    expect(ladderAt(1, 64).viewMode).toBe("list");
    expect(ladderAt(-5, 64).viewMode).toBe("details");
    expect(ladderAt(99, 64).iconSize).toBe(MAX_ICON);
  });
});

describe("zoomStep", () => {
  it("al reducir pasa de iconos pequeños a lista y a detalles", () => {
    let v = zoomStep(icons(MIN_ICON), -1);
    expect(v.viewMode).toBe("list");
    v = zoomStep(v, -1);
    expect(v.viewMode).toBe("details");
    expect(zoomStep(v, -1).viewMode).toBe("details");
  });
  it("al ampliar desde detalles se sube a lista y luego a iconos", () => {
    const l = zoomStep({ viewMode: "details", iconSize: 64 }, 1);
    expect(l.viewMode).toBe("list");
    expect(zoomStep(l, 1)).toEqual(icons(MIN_ICON));
  });
  it("mosaicos y contenido tienen salida", () => {
    expect(zoomStep({ viewMode: "tiles", iconSize: 64 }, 1).viewMode).toBe("icons");
    expect(zoomStep({ viewMode: "content", iconSize: 64 }, 1).viewMode).toBe("list");
  });
});

describe("geometría de iconos", () => {
  it("crece con el tamaño y los pequeños van en línea", () => {
    expect(iconGeometry(32).inline).toBe(true);
    expect(iconGeometry(96).inline).toBe(false);
    const widths = [40, 64, 96, 128, 200, 256].map((s) => iconGeometry(s).cellW);
    expect(widths).toEqual([...widths].sort((a, b) => a - b));
  });
  it("es más compacta que antes", () => {
    expect(iconGeometry(96).cellW).toBeLessThan(140);
    expect(iconGeometry(96).cellH).toBeLessThan(158);
  });
  it("deja sitio para el icono y dos líneas de nombre, también con letra grande", () => {
    for (const s of [40, 96, 256]) expect(iconGeometry(s).cellH - s).toBeGreaterThanOrEqual(42);
    expect(iconGeometry(96, 120).cellH).toBeGreaterThan(iconGeometry(96).cellH);
  });
  it("reparte el ancho disponible sin desbordar", () => {
    for (const w of [300, 640, 901, 1240, 1900]) for (const s of [40, 64, 96, 128, 256]) {
      const { perRow, cellW } = iconCells(s, w);
      expect(perRow * cellW).toBeLessThanOrEqual(w - 16 < iconGeometry(s).cellW ? cellW : w - 16);
      expect(cellW).toBeGreaterThanOrEqual(iconGeometry(s).cellW);
    }
  });
  it("con pocos elementos por fila, la celda no se deforma", () => {
    expect(iconCells(96, 1900).cellW).toBeLessThanOrEqual(Math.floor(iconGeometry(96).cellW * 1.3));
    expect(iconCells(256, 300).perRow).toBe(1);
  });
  it("sin medir todavía da al menos una fila", () => {
    expect(iconCells(96, 0).perRow).toBe(1);
  });
});

describe("columnas en paneles estrechos", () => {
  const w = (n: number) => n;
  it("retira las últimas hasta dejar sitio al nombre", () => {
    expect(fitColumns([100, 100, 100], w, 600)).toEqual([100, 100, 100]);
    expect(fitColumns([100, 100, 100], w, 420)).toEqual([100, 100]);
    expect(fitColumns([100, 100, 100], w, 200)).toEqual([]);
  });
  it("sin medir todavía no retira nada y no altera la entrada", () => {
    const c = [100, 100];
    expect(fitColumns(c, w, 0)).toEqual(c);
    fitColumns(c, w, 10);
    expect(c.length).toBe(2);
  });
});
