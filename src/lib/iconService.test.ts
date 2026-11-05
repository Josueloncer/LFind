import { describe, expect, it, vi } from "vitest";
import type { IconItem, IconsReply } from "./fs";
import { PER_FILE_EXTS, extOf, iconBucket, iconPx, sharedKey, typeKey } from "./iconKinds";
import { IconService } from "./iconService";
import type { IconTarget } from "./iconService";

const file = (name: string, size = 10, modified = 1): IconTarget => ({ path: `C:\\x\\${name}`, entry: { kind: "file", name, size, modified } });
const dir = (name: string, modified = 1): IconTarget => ({ path: `C:\\x\\${name}`, entry: { kind: "dir", name, size: 0, modified } });

/** Backend de mentira que se comporta como `icons.rs`: claves compartidas por tipo, claves propias para exe/carpetas. */
function fake(opts: { failExts?: string[]; customDirs?: string[] } = {}) {
  const calls: { items: IconItem[]; px: number; have: string[] }[] = [];
  const getIcons = vi.fn(async (items: IconItem[], px: number, have: string[]): Promise<IconsReply> => {
    calls.push({ items, px, have });
    const icons: Record<string, string> = {};
    const keys = items.map((it) => {
      const name = it.path.split("\\").pop() as string;
      let k: string;
      if (it.dir) k = opts.customDirs?.includes(name) ? `f-${name}-1-1-${px}-v1` : `d-${px}`;
      else {
        const ext = extOf(name);
        if (opts.failExts?.includes(ext)) return null;
        k = PER_FILE_EXTS.includes(ext) ? `f-${name}-${px}-v1` : typeKey(ext, false, px);
      }
      if (!have.includes(k)) icons[k] = `data:image/png;base64,${k}`;
      return k;
    });
    return { keys, icons };
  });
  return { calls, backend: { getIcons } };
}
const svc = (b: ReturnType<typeof fake>) => new IconService(b.backend, { batchDelayMs: 0, maxBatch: 256, maxInflight: 2, maxPerFile: 1500, maxNone: 5000, timeoutMs: 5000 });

describe("iconKinds (réplica de icons.rs)", () => {
  it("claves y tamaños idénticos a los de Rust", () => {
    expect(typeKey("docx", false, 64)).toBe("t-docx-64");
    expect(typeKey("", false, 32)).toBe("t-_-32");
    expect(typeKey("", true, 256)).toBe("d-256");
    expect([20, 40, 64, 96, 512].map(iconBucket)).toEqual([32, 48, 64, 128, 256]);
    expect(iconPx(22, 1)).toBe(32);
    expect(iconPx(22, 2)).toBe(48);
    expect(iconPx(128, 3)).toBe(256);
  });

  it("extensiones como `Path::extension` de Rust", () => {
    expect(extOf("Informe.DOCX")).toBe("docx");
    expect(extOf("a.tar.gz")).toBe("gz");
    expect(extOf(".gitignore")).toBe("");
    expect(extOf("LEEME")).toBe("");
    expect(extOf("fin.")).toBe("");
    expect(extOf("raro.ex e!")).toBe("");
  });

  it("solo los archivos con icono por tipo tienen clave compartida", () => {
    expect(sharedKey({ kind: "file", name: "a.docx" }, 64)).toBe("t-docx-64");
    for (const e of PER_FILE_EXTS) expect(sharedKey({ kind: "file", name: `a.${e}` }, 64)).toBeNull();
    expect(sharedKey({ kind: "file", name: "a.dll" }, 64)).toBe("t-dll-64");
    expect(sharedKey({ kind: "dir", name: "Fotos" }, 64)).toBeNull();
  });
});

describe("IconService", () => {
  it("cientos de archivos del mismo tipo se resuelven con UN elemento en UNA llamada", async () => {
    const f = fake();
    const s = svc(f);
    const hs = Array.from({ length: 300 }, (_, i) => s.request(file(`f${i}.docx`, 10, i), 32));
    const hz = Array.from({ length: 40 }, (_, i) => s.request(file(`g${i}.zip`), 32));
    const all = await Promise.all([...hs, ...hz].map((h) => h.promise));
    expect(f.calls).toHaveLength(1);
    expect(f.calls[0].items).toHaveLength(2);
    expect(all.every((u) => typeof u === "string")).toBe(true);
    expect(all[0]).toBe("data:image/png;base64,t-docx-32");
    // Un archivo nuevo del mismo tipo ya no necesita al backend.
    expect(s.peek(file("nuevo.docx"), 32)).toBe("data:image/png;base64,t-docx-32");
    await s.request(file("otro.docx"), 32).promise;
    expect(f.calls).toHaveLength(1);
  });

  it("las solicitudes que llegan juntas salen en un solo lote; EXE y carpetas llevan su propia clave", async () => {
    const f = fake({ customDirs: ["Fotos"] });
    const s = svc(f);
    const r = await Promise.all([file("a.exe"), file("b.exe"), file("c.lnk"), dir("Fotos"), dir("Normal"), dir("Otra"), file("d.pdf")].map((t) => s.request(t, 48).promise));
    expect(f.calls).toHaveLength(1);
    expect(f.calls[0].items.map((i) => i.path.split("\\").pop())).toEqual(["a.exe", "b.exe", "c.lnk", "Fotos", "Normal", "Otra", "d.pdf"]);
    expect(r.every((u) => typeof u === "string")).toBe(true);
    expect(r[0]).not.toBe(r[1]); // cada EXE conserva su icono
    expect(r[4]).toBe(r[5]); // carpetas normales: el mismo icono genérico
    expect(r[3]).not.toBe(r[4]); // carpeta personalizada: el suyo
  });

  it("un archivo que cambia (fecha o tamaño) vuelve a preguntar; si no cambia, usa la memoria", async () => {
    const f = fake();
    const s = svc(f);
    await s.request(file("app.exe", 100, 5), 64).promise;
    expect(s.peek(file("app.exe", 100, 5), 64)).toBeTypeOf("string");
    await s.request(file("app.exe", 100, 5), 64).promise;
    expect(f.calls).toHaveLength(1);
    expect(s.peek(file("app.exe", 100, 6), 64)).toBeUndefined();
    await s.request(file("app.exe", 100, 6), 64).promise;
    expect(f.calls).toHaveLength(2);
  });

  it("sin icono del sistema: se recuerda (peek = null) y no se reintenta", async () => {
    const f = fake({ failExts: ["zzz"] });
    const s = svc(f);
    expect(await s.request(file("a.zzz"), 32).promise).toBeNull();
    expect(s.peek(file("b.zzz"), 32)).toBeNull();
    expect(await s.request(file("c.zzz"), 32).promise).toBeNull();
    expect(f.calls).toHaveLength(1);
  });

  it("lo que sale de pantalla antes de enviarse no llega al backend", async () => {
    const f = fake();
    const s = svc(f);
    const a = s.request(file("a.exe"), 32);
    const b = s.request(file("b.exe"), 32);
    a.release();
    await b.promise;
    expect(f.calls[0].items).toHaveLength(1);
    expect(f.calls[0].items[0].path).toBe("C:\\x\\b.exe");
  });

  it("cambiar de carpeta descarta la cola que no salió", async () => {
    const f = fake();
    const s = svc(f);
    const p = s.request(file("a.exe"), 32).promise;
    s.newEpoch();
    expect(await p).toBeNull();
    expect(f.calls).toHaveLength(0);
  });

  it("envía las claves compartidas que ya tiene, para no recibir bytes repetidos", async () => {
    const f = fake();
    const s = svc(f);
    await s.request(file("a.docx"), 32).promise;
    await s.request(file("b.exe"), 32).promise;
    expect(f.calls[1].have).toContain("t-docx-32");
    expect(f.calls[1].have.every((k) => k.endsWith("-32"))).toBe(true);
  });

  it("un fallo del backend no se memoriza (se podrá reintentar) y no bloquea la cola", async () => {
    const getIcons = vi.fn().mockRejectedValueOnce(new Error("boom")).mockResolvedValue({ keys: ["t-txt-32"], icons: { "t-txt-32": "data:x" } });
    const s = new IconService({ getIcons }, { batchDelayMs: 0, maxBatch: 256, maxInflight: 1, maxPerFile: 10, maxNone: 10, timeoutMs: 5000 });
    expect(await s.request(file("a.txt"), 32).promise).toBeNull();
    expect(s.peek(file("a.txt"), 32)).toBeUndefined();
    expect(await s.request(file("a.txt"), 32).promise).toBe("data:x");
  });

  it("clear() vacía la memoria y avisa a los componentes visibles", async () => {
    const f = fake();
    const s = svc(f);
    await s.request(file("a.docx"), 32).promise;
    const seen = vi.fn();
    s.onClear(seen);
    s.clear();
    expect(seen).toHaveBeenCalledTimes(1);
    expect(s.peek(file("a.docx"), 32)).toBeUndefined();
    await s.request(file("a.docx"), 32).promise;
    expect(f.calls).toHaveLength(2);
  });

  it("la memoria por archivo está acotada", async () => {
    const f = fake();
    const s = new IconService(f.backend, { batchDelayMs: 0, maxBatch: 256, maxInflight: 2, maxPerFile: 5, maxNone: 5, timeoutMs: 5000 });
    await Promise.all(Array.from({ length: 20 }, (_, i) => s.request(file(`p${i}.exe`), 32).promise));
    expect(s.stats().perFile).toBeLessThanOrEqual(5);
  });
});
