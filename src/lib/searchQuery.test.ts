import { describe, expect, it } from "vitest";
import type { IndexStatus, SearchHit } from "./fs/types";
import { DEFAULT_FILTERS, activeFilterCount, appendUnique, buildQuery, dateRange, describeIndex, extractOperators, hasCriteria, hitToEntry, invalidSizes, parseExts, parseSize, startOfDay } from "./searchQuery";
import type { SearchFilters } from "./searchQuery";

const NOW = new Date(2026, 10, 7, 15, 30, 0).getTime(); // 7-nov-2026 15:30 (hora local)
const DAY = 86_400_000;
const F = (p: Partial<SearchFilters> = {}): SearchFilters => ({ ...DEFAULT_FILTERS, ...p });
const q = (text: string, p: Partial<SearchFilters> = {}) => buildQuery(text, F(p), { now: NOW });
const hit = (path: string, p: Partial<SearchHit> = {}): SearchHit => ({ path, name: path.split(/[\\/]/).pop() ?? path, ext: "", isDir: false, size: 0, modified: null, created: null, ...p });

describe("tamaños", () => {
  it("unidades binarias, coma decimal y espacios", () => {
    expect(parseSize("1gb")).toEqual({ ok: true, bytes: 1024 ** 3 });
    expect(parseSize(" 1,5 GB ")).toEqual({ ok: true, bytes: Math.floor(1.5 * 1024 ** 3) });
    expect(parseSize("500k")).toEqual({ ok: true, bytes: 512_000 });
    expect(parseSize("2 MB")).toEqual({ ok: true, bytes: 2 * 1024 ** 2 });
    expect(parseSize("2048")).toEqual({ ok: true, bytes: 2048 });
    expect(parseSize("1.5tb")).toEqual({ ok: true, bytes: Math.floor(1.5 * 1024 ** 4) });
  });
  it("vacío = sin límite; basura = inválido", () => {
    expect(parseSize("")).toEqual({ ok: true, bytes: null });
    expect(parseSize("   ")).toEqual({ ok: true, bytes: null });
    for (const bad of ["abc", "5xb", "-3", "1..5gb", "gb", "1 2 3 gb x"]) expect(parseSize(bad).ok, bad).toBe(false);
    expect(invalidSizes({ minSize: "x", maxSize: "1gb" })).toEqual({ min: true, max: false });
  });
});

describe("extensiones y operadores", () => {
  it("parseExts limpia puntos y comodines y no repite", () => {
    expect(parseExts("psd, .PNG;*.jpg  psd")).toEqual(["psd", "png", "jpg"]);
    expect(parseExts("")).toEqual([]);
  });
  it("ext: y *.ext salen del texto; lo demás sigue siendo nombre", () => {
    expect(extractOperators("informe ext:psd *.PNG final")).toEqual({ text: "informe final", exts: ["psd", "png"] });
    expect(extractOperators("ext:pdf,docx")).toEqual({ text: "", exts: ["pdf", "docx"] });
    expect(extractOperators("ext: foo*.")).toEqual({ text: "ext: foo*.", exts: [] });
  });
});

describe("construcción de la consulta (ejemplos de uso)", () => {
  it("buscar un archivo por nombre sin conocer su ubicación", () => {
    const r = q("  Factura 2026 ");
    expect(r).toMatchObject({ text: "Factura 2026", exact: false, exts: [], category: null, kind: "any", drives: [], folder: null, sort: "relevance", offset: 0, limit: 200 });
  });
  it("todos los PSD", () => {
    expect(q("", { exts: "psd" }).exts).toEqual(["psd"]);
    expect(q("*.psd").exts).toEqual(["psd"]);
    expect(q("ext:psd logo")).toMatchObject({ text: "logo", exts: ["psd"] });
    expect(q("", { category: "design" }).category).toBe("design");
  });
  it("imágenes PNG: extensión y categoría se combinan", () => {
    expect(q("", { exts: "png", category: "image" })).toMatchObject({ exts: ["png"], category: "image" });
  });
  it("archivos mayores de 1 GB", () => {
    expect(q("", { minSize: "1 GB", kind: "file" })).toMatchObject({ minSize: 1024 ** 3, maxSize: null, kind: "file" });
  });
  it("PDF modificados durante el último mes", () => {
    const r = q("", { exts: "pdf", datePreset: "30d" });
    expect(r.modified).toEqual({ after: NOW - 30 * DAY, before: null });
    expect(r.created).toEqual({ after: null, before: null });
    expect(r.accessed).toEqual({ after: null, before: null });
  });
  it("carpetas con determinado nombre", () => {
    expect(q("proyecto", { kind: "dir" })).toMatchObject({ text: "proyecto", kind: "dir" });
  });
  it("el campo de fecha decide qué columna se filtra", () => {
    expect(q("", { dateField: "created", datePreset: "7d" }).created.after).toBe(NOW - 7 * DAY);
    expect(q("", { dateField: "accessed", datePreset: "today" }).accessed.after).toBe(startOfDay(NOW));
    expect(q("", { dateField: "accessed", datePreset: "today" }).modified.after).toBeNull();
  });
  it("unidad y carpeta (con comillas pegadas del Explorador)", () => {
    expect(q("", { drive: "D:\\" }).drives).toEqual(["D:\\"]);
    expect(q("", { folder: '  "C:\\Users\\Ana\\Docs" ' }).folder).toBe("C:\\Users\\Ana\\Docs");
    expect(q("", { folder: "   " }).folder).toBeNull();
  });
  it("coincidencia exacta y orden se transmiten", () => {
    expect(q("informe.pdf", { exact: true, sort: "size", desc: true })).toMatchObject({ exact: true, sort: "size", desc: true });
  });
  it("mínimo mayor que máximo se intercambia; tamaños ilegibles no se aplican", () => {
    expect(q("", { minSize: "2gb", maxSize: "1gb" })).toMatchObject({ minSize: 1024 ** 3, maxSize: 2 * 1024 ** 3 });
    expect(q("", { minSize: "basura", maxSize: "1mb" })).toMatchObject({ minSize: null, maxSize: 1024 ** 2 });
  });
  it("la paginación la fija quien llama", () => {
    expect(buildQuery("x", F(), { now: NOW, limit: 50, offset: 100 })).toMatchObject({ limit: 50, offset: 100 });
  });
});

describe("fechas", () => {
  it("rango personalizado: del inicio del primer día al final del último (hora local)", () => {
    const r = dateRange({ datePreset: "custom", dateFrom: "2026-10-01", dateTo: "2026-10-31" }, NOW)!;
    expect(r.after).toBe(new Date(2026, 9, 1, 0, 0, 0, 0).getTime());
    expect(r.before).toBe(new Date(2026, 9, 31, 23, 59, 59, 999).getTime());
  });
  it("un solo extremo, o ninguno válido", () => {
    expect(dateRange({ datePreset: "custom", dateFrom: "2026-10-01", dateTo: "" }, NOW)).toEqual({ after: new Date(2026, 9, 1).getTime(), before: null });
    expect(dateRange({ datePreset: "custom", dateFrom: "ayer", dateTo: "2026-13-45x" }, NOW)).toBeNull();
    expect(dateRange({ datePreset: "any", dateFrom: "2026-10-01", dateTo: "" }, NOW)).toBeNull();
  });
  it("hoy empieza a las 00:00", () => {
    expect(dateRange({ datePreset: "today", dateFrom: "", dateTo: "" }, NOW)).toEqual({ after: new Date(2026, 10, 7).getTime(), before: null });
  });
});

describe("criterios y filtros activos", () => {
  it("sin texto ni filtros no se consulta", () => {
    expect(hasCriteria("", F())).toBe(false);
    expect(hasCriteria("   ", F())).toBe(false);
    expect(hasCriteria("a", F())).toBe(true);
    expect(hasCriteria("", F({ exts: "psd" }))).toBe(true);
    expect(hasCriteria("", F({ minSize: "1gb" }))).toBe(true);
  });
  it("cuenta solo los filtros que se aplican de verdad", () => {
    expect(activeFilterCount(F())).toBe(0);
    expect(activeFilterCount(F({ exts: "png", category: "image", kind: "file", minSize: "1mb", maxSize: "x", datePreset: "7d", drive: "C:\\", folder: "D:\\x", exact: true }))).toBe(8);
    expect(activeFilterCount(F({ datePreset: "custom" }))).toBe(0);
  });
});

describe("resultados", () => {
  it("appendUnique no repite rutas y conserva el orden", () => {
    const a = [hit("C:\\a"), hit("C:\\b")];
    expect(appendUnique(a, [hit("C:\\b"), hit("C:\\c"), hit("C:\\c")]).map((h) => h.path)).toEqual(["C:\\a", "C:\\b", "C:\\c"]);
    expect(a).toHaveLength(2);
  });
  it("hitToEntry sirve para iconos y vista previa", () => {
    expect(hitToEntry(hit("C:\\x\\Foto.PNG", { ext: "png", size: 5, modified: 9 }))).toEqual({ name: "Foto.PNG", kind: "file", size: 5, modified: 9, created: null, hidden: false, ext: "png", link: false });
    expect(hitToEntry(hit("C:\\x", { isDir: true })).kind).toBe("dir");
    expect(hitToEntry(hit("C:\\x\\README")).ext).toBeNull();
  });
  it("estado del índice para la barra inferior", () => {
    const base: IndexStatus = { phase: "idle", currentRoot: null, scanned: 0, progress: null, roots: [], totalFiles: 10, totalDirs: 5, skipped: 0, issues: [], lastUpdate: null };
    expect(describeIndex(null)).toBe("Índice no disponible");
    expect(describeIndex(base)).toBe("Índice al día · 15 entradas");
    expect(describeIndex({ ...base, phase: "scanning", scanned: 1234, progress: 0.456 })).toBe("Indexando… 1234 entradas vistas · 46 % · 15 en el índice");
    expect(describeIndex({ ...base, phase: "scanning", scanned: 7 })).toBe("Indexando… 7 entradas vistas · 15 en el índice");
    expect(describeIndex({ ...base, phase: "paused" })).toBe("Indexado en pausa · 15 en el índice");
    const pending = { ...base, roots: [{ id: 1, path: "C:\\", state: "interrupted" as const, online: true, gen: 1, files: 0, dirs: 0, skipped: 0, lastComplete: null }] };
    expect(describeIndex(pending)).toBe("Índice incompleto · 15 entradas");
    expect(describeIndex({ ...pending, roots: [{ ...pending.roots[0], online: false }] })).toBe("Índice al día · 15 entradas");
  });
});
