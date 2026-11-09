import { describe, expect, it, vi } from "vitest";
import { ThumbService } from "./thumbService";
import type { ThumbBackend } from "./thumbService";
import { isThumbEligible, thumbBucket, thumbKey } from "./thumbKinds";

/** Backend controlable: cada llamada queda pendiente hasta que el test la resuelve. */
function fake() {
  const calls: { path: string; px: number; epoch: number | null; ok(url?: string): void; err(e: unknown): void }[] = [];
  let live = 0;
  let peak = 0;
  const epochs: number[] = [];
  const backend: ThumbBackend = {
    getThumbnail: (path, px, epoch) =>
      new Promise((res, rej) => {
        live++;
        peak = Math.max(peak, live);
        calls.push({ path, px, epoch, ok: (u = `data:${path}`) => (live--, res({ dataUrl: u })), err: (e) => (live--, rej(e)) });
      }),
    setThumbEpoch: async (e) => void epochs.push(e),
  };
  return { backend, calls, epochs, peak: () => peak };
}
const tick = () => new Promise((r) => setTimeout(r, 0));
const svc = (f: ReturnType<typeof fake>, o = {}) => new ThumbService(f.backend, { maxInflight: 3, maxCache: 400, maxFailed: 100, timeoutMs: 30_000, ...o });

describe("ThumbService (cola de miniaturas de la interfaz)", () => {
  it("nunca supera la concurrencia máxima y atiende en orden de llegada", async () => {
    const f = fake();
    const s = svc(f);
    const hs = Array.from({ length: 10 }, (_, i) => s.request(`k${i}`, `p${i}`, 128));
    await tick();
    expect(f.calls.map((c) => c.path)).toEqual(["p0", "p1", "p2"]);
    for (let i = 0; i < 10; i++) {
      f.calls[i].ok();
      await tick();
    }
    const out = await Promise.all(hs.map((h) => h.promise));
    expect(out.every((o) => o.ok)).toBe(true);
    expect(f.peak()).toBe(3);
    expect(f.calls.map((c) => c.path)).toEqual(Array.from({ length: 10 }, (_, i) => `p${i}`));
  });

  it("deduplica: la misma miniatura pedida varias veces es una sola llamada", async () => {
    const f = fake();
    const s = svc(f);
    const a = s.request("k", "p", 128);
    const b = s.request("k", "p", 128);
    await tick();
    expect(f.calls).toHaveLength(1);
    f.calls[0].ok("data:x");
    expect(await a.promise).toEqual({ ok: true, url: "data:x" });
    expect(await b.promise).toEqual({ ok: true, url: "data:x" });
    expect(s.request("k", "p", 128).promise).resolves.toEqual({ ok: true, url: "data:x" }); // y luego sale de memoria
    await tick();
    expect(f.calls).toHaveLength(1);
  });

  it("si un elemento sale de pantalla antes de pedirse, no se pide nunca", async () => {
    const f = fake();
    const s = svc(f, { maxInflight: 1 });
    s.request("busy", "busy", 128); // ocupa el único hueco
    const scrolledAway = s.request("gone", "gone", 128);
    const stays = s.request("stays", "stays", 128);
    await tick();
    scrolledAway.release();
    expect(await scrolledAway.promise).toEqual({ ok: false, kind: "cancelled" });
    f.calls[0].ok();
    await tick();
    expect(f.calls.map((c) => c.path)).toEqual(["busy", "stays"]);
    f.calls[1].ok();
    expect((await stays.promise).ok).toBe(true);
  });

  it("con dos interesados, uno que se va no cancela la solicitud", async () => {
    const f = fake();
    const s = svc(f, { maxInflight: 1 });
    s.request("busy", "busy", 128);
    const a = s.request("k", "p", 128);
    const b = s.request("k", "p", 128);
    a.release();
    f.calls[0].ok();
    await tick();
    expect(f.calls.map((c) => c.path)).toEqual(["busy", "p"]);
    f.calls[1].ok();
    expect((await b.promise).ok).toBe(true);
  });

  it("cambiar de carpeta descarta la cola, avisa al backend y las nuevas llevan la época nueva", async () => {
    const f = fake();
    const s = svc(f, { maxInflight: 1 });
    const running = s.request("a", "a", 128);
    const queued = [s.request("b", "b", 128), s.request("c", "c", 128)];
    await tick();
    expect(f.calls[0].epoch).toBe(0);
    const e = s.newEpoch();
    expect(e).toBe(1);
    expect(f.epochs).toEqual([1]);
    for (const q of queued) expect(await q.promise).toEqual({ ok: false, kind: "cancelled" });
    const fresh = s.request("d", "d", 128);
    f.calls[0].ok(); // la solicitud en vuelo de la carpeta anterior termina
    await tick();
    expect(f.calls.map((c) => c.path)).toEqual(["a", "d"]); // b y c nunca se pidieron
    expect(f.calls[1].epoch).toBe(1);
    f.calls[1].ok();
    expect((await fresh.promise).ok).toBe(true);
    // El resultado tardío sigue siendo válido y queda en caché por si se vuelve a esa carpeta.
    expect((await running.promise).ok).toBe(true);
    expect(s.cached("a")).toBe("data:a");
  });

  it("la vista previa pasa por delante de la cola y vaciar la caché avisa a los observadores", async () => {
    const f = fake();
    const s = svc(f, { maxInflight: 1 });
    s.request("busy", "busy", 64);
    s.request("a", "a", 64);
    s.request("preview", "preview", 512, true);
    await tick();
    f.calls[0].ok();
    await tick();
    expect(f.calls.map((c) => c.path)).toEqual(["busy", "preview"]);
    let cleared = 0;
    const off = s.onClear(() => cleared++);
    f.calls[1].ok();
    await tick();
    s.clear();
    expect(cleared).toBe(1);
    expect(s.cached("preview")).toBeUndefined();
    off();
    s.clear();
    expect(cleared).toBe(1);
  });

  it("un backend que no responde no bloquea la cola para siempre (tiempo límite)", async () => {
    vi.useFakeTimers();
    try {
      const f = fake();
      const s = svc(f, { maxInflight: 1, timeoutMs: 1000 });
      const stuck = s.request("stuck", "stuck", 64);
      const next = s.request("next", "next", 64);
      await vi.advanceTimersByTimeAsync(0);
      expect(f.calls.map((c) => c.path)).toEqual(["stuck"]);
      await vi.advanceTimersByTimeAsync(1001);
      expect(await stuck.promise).toEqual({ ok: false, kind: "failed" });
      expect(f.calls.map((c) => c.path)).toEqual(["stuck", "next"]); // el hueco se liberó
      f.calls[0].ok(); // la respuesta tardía de «stuck» se ignora sin romper la cuenta
      f.calls[1].ok();
      expect((await next.promise).ok).toBe(true);
      await vi.advanceTimersByTimeAsync(0);
      expect(s.stats().inflight).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("reset() libera la cola y los huecos aunque haya solicitudes en vuelo", async () => {
    const f = fake();
    const s = svc(f, { maxInflight: 2 });
    for (const k of ["a", "b", "c", "d"]) s.request(k, k, 64);
    await tick();
    expect(s.stats()).toMatchObject({ inflight: 2, queued: 2 });
    s.reset();
    expect(s.stats()).toMatchObject({ inflight: 0, queued: 0, cached: 0 });
    s.request("e", "e", 64);
    await tick();
    expect(f.calls[f.calls.length - 1].path).toBe("e");
    f.calls[0].ok(); // respuestas de antes del reset: no alteran la cuenta
    f.calls[1].ok();
    await tick();
    expect(s.stats().inflight).toBe(1);
  });

  it("la caché en memoria es LRU y está acotada", async () => {
    const f = fake();
    const s = svc(f, { maxCache: 3 });
    for (const k of ["a", "b", "c"]) {
      s.request(k, k, 64);
      await tick();
      f.calls[f.calls.length - 1].ok();
      await tick();
    }
    expect(s.cached("a")).toBeDefined(); // «a» pasa a ser la más reciente
    s.request("d", "d", 64);
    await tick();
    f.calls[f.calls.length - 1].ok();
    await tick();
    expect(s.cached("b")).toBeUndefined(); // la menos usada
    expect(["a", "c", "d"].every((k) => s.cached(k) !== undefined)).toBe(true);
    expect(s.stats().cached).toBe(3);
  });

  it("formatos no soportados o dañados se recuerdan, no bloquean la cola y no se repiten", async () => {
    const f = fake();
    const s = svc(f, { maxInflight: 1 });
    const bad = s.request("bad", "bad", 64);
    const good = s.request("good", "good", 64);
    await tick();
    f.calls[0].err({ kind: "unsupported", message: "sin proveedor" });
    expect(await bad.promise).toEqual({ ok: false, kind: "unsupported" });
    await tick();
    f.calls[1].ok();
    expect((await good.promise).ok).toBe(true);
    expect(await s.request("bad", "bad", 64).promise).toEqual({ ok: false, kind: "unsupported" });
    expect(f.calls).toHaveLength(2); // «bad» no se volvió a pedir
    // Error inesperado (rechazo sin formato) = fallo controlado.
    const weird = s.request("w", "w", 64);
    await tick();
    f.calls[2].err(new Error("boom"));
    expect(await weird.promise).toEqual({ ok: false, kind: "failed" });
  });

  it("un descarte del backend no se toma por un fallo del archivo", async () => {
    const f = fake();
    const s = svc(f);
    const r = s.request("k", "p", 64);
    await tick();
    f.calls[0].err({ kind: "cancelled", message: "" });
    expect(await r.promise).toEqual({ ok: false, kind: "cancelled" });
    expect(s.failure("k")).toBeUndefined();
    s.request("k", "p", 64); // se puede volver a pedir
    await tick();
    expect(f.calls).toHaveLength(2);
  });

  it("la memoria de fallos también está acotada", async () => {
    const f = fake();
    const s = svc(f, { maxFailed: 2, maxInflight: 1 });
    for (const k of ["a", "b", "c"]) {
      s.request(k, k, 64);
      await tick();
      f.calls[f.calls.length - 1].err({ kind: "failed" });
      await tick();
    }
    expect(s.stats().failed).toBe(2);
    expect(s.failure("a")).toBeUndefined();
  });
});

describe("criterios de qué pedir", () => {
  const e = (o: object) => ({ kind: "file" as const, ext: "png", hidden: false, size: 10, ...o });
  it("solo archivos visibles, no vacíos y con extensión conocida", () => {
    expect(isThumbEligible(e({}))).toBe(true);
    expect(isThumbEligible(e({ kind: "dir" }))).toBe(false);
    expect(isThumbEligible(e({ hidden: true }))).toBe(false);
    expect(isThumbEligible(e({ size: 0 }))).toBe(false);
    expect(isThumbEligible(e({ ext: "exe" }))).toBe(false);
    expect(isThumbEligible(e({ ext: null }))).toBe(false);
    for (const x of ["psd", "ai", "pdf", "svg", "webp", "mp4"]) expect(isThumbEligible(e({ ext: x }))).toBe(true);
  });

  it("el tamaño pedido crece con el icono, con pocos escalones y sin pasarse", () => {
    expect(thumbBucket(48)).toBe(64);
    expect(thumbBucket(64)).toBe(64);
    expect(thumbBucket(96)).toBe(128);
    expect(thumbBucket(96, 1.5)).toBe(256);
    expect(thumbBucket(256)).toBe(256);
    expect(thumbBucket(256, 2)).toBe(512);
    expect(thumbBucket(900, 2)).toBe(512);
  });
});

describe("archivos en la nube", () => {
  const f = { kind: "file", ext: "jpg", hidden: false, size: 10, modified: 5 } as const;
  it("también piden miniatura: el backend solo mira la caché de Windows", () => {
    expect(isThumbEligible({ ...f, cloud: true } as never)).toBe(true);
  });
  it("al descargarse cambia la clave, y un fallo anterior deja de contar", () => {
    expect(thumbKey("C:\\a.jpg", 64, { ...f, cloud: true })).not.toBe(thumbKey("C:\\a.jpg", 64, { ...f, cloud: false }));
    expect(thumbKey("C:\\a.jpg", 64, { ...f, cloud: false })).toBe("C:\\a.jpg|64|5|10");
  });
});
