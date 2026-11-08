import { describe, expect, it } from "vitest";
import { MAX_CANVAS_PIXELS, MAX_ZOOM, canvasScale, clampPan, filePosition, fitScale, formatDuration, mergeFacts, neighborIndex, pageAtCenter, pdfDate, pdfScale, rotateCw, zoomAt } from "./viewerMath";
import type { ImageView } from "./viewerMath";

const isFile = (t: string) => t !== "dir";
const img = { w: 400, h: 200 };
const area = { w: 200, h: 200 };
const base: ImageView = { fit: true, scale: 1, x: 0, y: 0, rotation: 0 };

describe("navegación", () => {
  const l = ["dir", "a", "dir", "b", "c", "dir"];
  it("salta carpetas y se detiene en los extremos", () => {
    expect(neighborIndex(l, isFile, 1, 1)).toBe(3);
    expect(neighborIndex(l, isFile, 3, -1)).toBe(1);
    expect(neighborIndex(l, isFile, 1, -1)).toBeNull();
    expect(neighborIndex(l, isFile, 4, 1)).toBeNull();
  });
  it("cuenta solo archivos", () => {
    expect(filePosition(l, isFile, 3)).toEqual({ index: 2, total: 3 });
    expect(filePosition(l, isFile, 0)).toBeNull();
    expect(filePosition(l, isFile, -1)).toBeNull();
  });
});

describe("zoom de imagen", () => {
  it("ajusta sin ampliar lo pequeño y respeta la rotación", () => {
    expect(fitScale(img, area, 0)).toBe(0.5);
    expect(fitScale({ w: 16, h: 16 }, area, 0)).toBe(1);
    expect(fitScale({ w: 400, h: 100 }, { w: 100, h: 400 }, 90)).toBe(1);
  });
  it("mantiene fijo el punto bajo el cursor y limita la escala", () => {
    const big = { w: 1000, h: 1000 };
    const v = zoomAt({ ...base, scale: 1 }, 2, 50, 0, big, area);
    expect(v).toMatchObject({ fit: false, scale: 2, x: -50, y: 0 });
    expect(zoomAt(base, 1e9, 0, 0, big, area).scale).toBe(MAX_ZOOM);
  });
  it("no deja perder la imagen", () => {
    expect(clampPan({ ...base, x: 9999, y: -9999 }, img, area)).toMatchObject({ x: 100, y: 0 });
  });
  it("gira de 90 en 90", () => {
    expect(rotateCw(270)).toBe(0);
    expect(rotateCw(90)).toBe(180);
  });
});

describe("PDF", () => {
  const page = { w: 100, h: 200 };
  it("escala según el modo", () => {
    expect(pdfScale({ mode: "width" }, page, { w: 524, h: 300 })).toBe(5);
    expect(pdfScale({ mode: "page" }, page, { w: 524, h: 224 })).toBe(1);
    expect(pdfScale({ mode: "custom", scale: 1.5 }, page, area)).toBe(1.5);
  });
  it("limita los píxeles del lienzo", () => {
    expect(canvasScale(100, 100, 3)).toBe(2);
    expect(canvasScale(100, 100, 0.5)).toBe(1);
    const s = canvasScale(5000, 5000, 2);
    expect(5000 * 5000 * s * s).toBeLessThanOrEqual(MAX_CANVAS_PIXELS * 1.001);
  });
  it("identifica la página central", () => {
    const starts = [0, 100, 200];
    expect(pageAtCenter(starts, 0, 100)).toBe(1);
    expect(pageAtCenter(starts, 100, 100)).toBe(2);
    expect(pageAtCenter(starts, 500, 100)).toBe(3);
  });
});

describe("ficha", () => {
  it("formatea duraciones y fechas", () => {
    expect(formatDuration(65)).toBe("1:05");
    expect(formatDuration(3723)).toBe("1:02:03");
    expect(formatDuration(NaN)).toBeNull();
    expect(pdfDate("D:20240131120000+01'00'")).toBe("31/01/2024");
    expect(pdfDate("ayer")).toBeNull();
  });
  it("no inventa ni repite campos", () => {
    const a = { label: "Tamaño", value: "1 KB" };
    expect(mergeFacts([a, { label: "Autor", value: "  " }], [{ label: "Tamaño", value: "2 KB" }])).toEqual([a]);
  });
});
