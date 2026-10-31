import { describe, expect, it } from "vitest";
import type { Entry } from "./fs";
import { buildLayout } from "./layout";
import { DRAG_THRESHOLD, MarqueeSession, combineSelection, edgeScroll, hitTest, makeGeometry, namesOf, rectFrom, sameSet } from "./marquee";
import { DETAILS_H, HEADER_H, LIST, TILE } from "./viewGeometry";
import { iconGeometry } from "./view";
import type { ViewMode } from "./fs";

const files = (n: number, dirs = 0): Entry[] =>
  Array.from({ length: n }, (_, i) => ({ name: `${i < dirs ? "dir" : "f"}${i}`, kind: i < dirs ? "dir" : "file", size: 1, modified: 1, created: 1, hidden: false, ext: i < dirs ? null : "txt", link: false }) as Entry);

function geo(mode: ViewMode, n: number, o: { iconSize?: number; width?: number; group?: boolean; itemH?: number; perRow?: number; rpc?: number } = {}) {
  const width = o.width ?? 900;
  const iconSize = o.iconSize ?? 96;
  const per = o.perRow ?? (mode === "tiles" ? Math.floor((width - 16) / TILE.w) : mode === "icons" ? Math.floor((width - 16) / iconGeometry(iconSize).cellW) : 1);
  const h = o.itemH ?? (mode === "details" ? DETAILS_H : mode === "tiles" ? TILE.h : mode === "icons" ? iconGeometry(iconSize).cellH : LIST.h);
  const entries = files(n, o.group ? 3 : 0);
  const layout = buildLayout(entries, o.group ? "type" : "none", mode === "list" ? 1 : per, h, HEADER_H, 0);
  return { g: makeGeometry({ mode, iconSize, width, layout, rpc: o.rpc ?? 10 }), layout };
}

describe("selección por rectángulo: qué elementos toca", () => {
  it("detalles: filas completamente y parcialmente cubiertas", () => {
    const { g } = geo("details", 100);
    // filas 2..4 (y de 64 a 160): el rectángulo toca por dentro la 2.ª y la 5.ª (parcial)
    expect(hitTest({ x: 20, y: 70, w: 100, h: 80 }, g)).toEqual([2, 3, 4]);
    expect(hitTest({ x: 20, y: 70, w: 100, h: 95 }, g)).toEqual([2, 3, 4, 5]);
  });

  it("iconos: varias filas y columnas, también carpetas y archivos mezclados", () => {
    const { g, layout } = geo("icons", 60, { iconSize: 96 });
    const cell = iconGeometry(96);
    // Rectángulo que abarca las 3 primeras columnas de las 2 primeras filas
    const r = { x: 10, y: 10, w: cell.cellW * 3 - 30, h: cell.cellH * 2 - 30 };
    const perRow = Math.floor((900 - 16) / cell.cellW);
    const idx = hitTest(r, g);
    expect(idx.sort((a, b) => a - b)).toEqual([0, 1, 2, perRow, perRow + 1, perRow + 2]);
    expect(namesOf(layout.flat, idx).size).toBe(6);
  });

  it("un rectángulo que solo roza el espacio entre elementos no selecciona nada", () => {
    const { g } = geo("icons", 40, { iconSize: 96 });
    const cell = iconGeometry(96);
    const gapX = 8 + cell.cellW - 2; // hueco entre la 1.ª y la 2.ª celda
    expect(hitTest({ x: gapX, y: 30, w: 3, h: 20 }, g)).toEqual([]);
    expect(hitTest({ x: 899, y: 5, w: 1, h: 1 }, g)).toEqual([]); // margen derecho vacío
  });

  it("mosaicos, contenido, iconos pequeños en línea y lista", () => {
    const t = geo("tiles", 30);
    expect(hitTest({ x: 20, y: 10, w: 30, h: 30 }, t.g)).toEqual([0]);
    expect(hitTest({ x: 20, y: 10, w: 600, h: 30 }, t.g).sort((a, b) => a - b)).toEqual([0, 1, 2]);
    const c = geo("content", 30, { itemH: 60 });
    expect(hitTest({ x: 30, y: 100, w: 5, h: 5 }, c.g)).toEqual([1]);
    const s = geo("icons", 50, { iconSize: 32 });
    expect(hitTest({ x: 10, y: 5, w: 20, h: 20 }, s.g)).toEqual([0]);
    const l = geo("list", 45, { rpc: 10 });
    // columna 0 = elementos 0..9, columna 1 = 10..19
    expect(hitTest({ x: 20, y: 40, w: 10, h: 40 }, l.g)).toEqual([1, 2]);
    expect(hitTest({ x: 20, y: 40, w: 10, h: 60 }, l.g)).toEqual([1, 2, 3]);
    expect(hitTest({ x: LIST.w + 20, y: 5, w: 10, h: 10 }, l.g)).toEqual([10]);
    expect(hitTest({ x: 20, y: 5, w: LIST.w * 2, h: 10 }, l.g).sort((a, b) => a - b)).toEqual([0, 10, 20]);
  });

  it("con grupos: las cabeceras no son elementos y los índices siguen el orden agrupado", () => {
    const { g, layout } = geo("details", 20, { group: true });
    // fila 0 = cabecera «Carpetas» (38 px), luego las 3 carpetas, otra cabecera…
    expect(g.rows[0].kind).toBe("header");
    expect(hitTest({ x: 20, y: 0, w: 50, h: 30 }, g)).toEqual([]);
    const idx = hitTest({ x: 20, y: HEADER_H + 5, w: 50, h: 3 * DETAILS_H }, g);
    expect(idx).toEqual([0, 1, 2]);
    expect(layout.flat.slice(0, 3).every((e) => e.kind === "dir")).toBe(true);
    // un rectángulo que atraviesa una cabecera salta sobre ella
    const across = hitTest({ x: 20, y: HEADER_H + 2 * DETAILS_H + 5, w: 50, h: 120 }, g);
    expect(across[0]).toBe(2);
    expect(across).toContain(3);
  });

  it("listas enormes: una selección pequeña no recorre todo ni falla", () => {
    const { g } = geo("details", 200_000);
    const t = performance.now();
    const idx = hitTest({ x: 20, y: 150_000 * DETAILS_H, w: 50, h: 5 * DETAILS_H }, g);
    expect(idx.length).toBeGreaterThanOrEqual(5);
    expect(idx[0]).toBeGreaterThanOrEqual(150_000);
    expect(performance.now() - t).toBeLessThan(250);
  });

  it("rectángulos vacíos o carpeta vacía", () => {
    expect(hitTest({ x: 0, y: 0, w: 10, h: 10 }, geo("details", 0).g)).toEqual([]);
    expect(hitTest({ x: 5000, y: 5000, w: 10, h: 10 }, geo("details", 5).g)).toEqual([]);
  });
});

describe("selección por rectángulo: clic frente a arrastre", () => {
  it("un clic o un temblor pequeño nunca se convierte en arrastre", () => {
    const s = new MarqueeSession({ x: 100, y: 100 });
    expect(s.update({ x: 100, y: 100 })).toBe(false);
    expect(s.update({ x: 102, y: 102 })).toBe(false);
    expect(s.update({ x: 100 + DRAG_THRESHOLD - 1, y: 100 })).toBe(false);
    expect(s.active).toBe(false);
  });

  it("al superar el umbral es arrastre y ya no vuelve a ser clic", () => {
    const s = new MarqueeSession({ x: 100, y: 100 });
    expect(s.update({ x: 100, y: 100 + DRAG_THRESHOLD })).toBe(true);
    expect(s.update({ x: 100, y: 100 })).toBe(true); // volver al punto inicial no lo cancela
  });
});

describe("selección por rectángulo: teclas y límites", () => {
  const base = new Set(["a", "b"]);
  const hit = new Set(["b", "c"]);
  it("sin teclas reemplaza; Mayús añade; Ctrl invierte", () => {
    expect([...combineSelection(base, hit, { ctrl: false, shift: false })].sort()).toEqual(["b", "c"]);
    expect([...combineSelection(base, hit, { ctrl: false, shift: true })].sort()).toEqual(["a", "b", "c"]);
    expect([...combineSelection(base, hit, { ctrl: true, shift: false })].sort()).toEqual(["a", "c"]);
  });
  it("no modifica los conjuntos originales", () => {
    combineSelection(base, hit, { ctrl: true, shift: false });
    expect([...base]).toEqual(["a", "b"]);
    expect([...hit]).toEqual(["b", "c"]);
  });
  it("el rectángulo se normaliza y se recorta al contenido (no sale de la zona de archivos)", () => {
    expect(rectFrom({ x: 300, y: 200 }, { x: 100, y: 50 }, { w: 900, h: 600 })).toEqual({ x: 100, y: 50, w: 200, h: 150 });
    expect(rectFrom({ x: 100, y: 100 }, { x: -50, y: 9999 }, { w: 900, h: 600 })).toEqual({ x: 0, y: 100, w: 100, h: 500 });
    expect(rectFrom({ x: 5000, y: 5000 }, { x: 5001, y: 5001 }, { w: 900, h: 600 })).toEqual({ x: 900, y: 600, w: 0, h: 0 });
  });
  it("sameSet", () => {
    expect(sameSet(new Set(["a", "b"]), new Set(["b", "a"]))).toBe(true);
    expect(sameSet(new Set(["a"]), new Set(["a", "b"]))).toBe(false);
    expect(sameSet(new Set(["a", "c"]), new Set(["a", "b"]))).toBe(false);
  });
  it("desplazamiento automático solo cerca de los bordes y proporcional a la cercanía", () => {
    expect(edgeScroll(300, 600)).toBe(0);
    expect(edgeScroll(2, 600)).toBeLessThan(0);
    expect(edgeScroll(598, 600)).toBeGreaterThan(0);
    expect(Math.abs(edgeScroll(1, 600))).toBeGreaterThan(Math.abs(edgeScroll(25, 600)));
    expect(Math.abs(edgeScroll(-500, 600))).toBeLessThanOrEqual(22);
  });
});
