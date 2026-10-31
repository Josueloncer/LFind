import { fs } from "./fs";
import type { ThumbFailKind, ThumbReply } from "./fs";

export type ThumbOutcome = { ok: true; url: string } | { ok: false; kind: ThumbFailKind };

export interface ThumbBackend {
  getThumbnail(path: string, px: number, epoch: number | null): Promise<ThumbReply>;
  setThumbEpoch(epoch: number): Promise<void>;
}

interface Req {
  key: string;
  path: string;
  px: number;
  epoch: number;
  /** Cuántos componentes esperan este resultado. Si el último se va antes de que salga la solicitud, se retira. */
  subs: number;
  queued: boolean;
  /** Generación del servicio en que salió: tras un `reset()` sus respuestas tardías ya no cuentan. */
  gen: number;
  promise: Promise<ThumbOutcome>;
  settle(o: ThumbOutcome): void;
}

export interface ThumbHandle {
  promise: Promise<ThumbOutcome>;
  release(): void;
}

export interface ThumbOptions {
  /** Llamadas simultáneas al backend (el backend aplica además su propio límite). */
  maxInflight: number;
  /** Miniaturas (data URLs) que se conservan en memoria. */
  maxCache: number;
  maxFailed: number;
  /** Si el backend no responde en este tiempo, se libera el hueco y se da por fallida (nunca debe bloquear la cola). */
  timeoutMs: number;
}

/**
 * Cola de miniaturas del lado de la interfaz:
 *  - solo se pide lo que hay en pantalla: al desmontarse un elemento, su solicitud pendiente se retira;
 *  - concurrencia limitada y deduplicación por clave;
 *  - `newEpoch()` (al cambiar de carpeta) descarta toda la cola y avisa al backend para que abandone
 *    lo que aún no haya empezado;
 *  - caché LRU acotada en memoria y memoria de fallos para no reintentar en cada desplazamiento.
 */
export class ThumbService {
  private epoch = 0;
  private gen = 0;
  private queue: Req[] = [];
  private inflight = 0;
  private pending = new Map<string, Req>();
  private cache = new Map<string, string>();
  private failed = new Map<string, ThumbFailKind>();
  private listeners = new Set<() => void>();

  constructor(private backend: ThumbBackend, private opts: ThumbOptions = { maxInflight: 4, maxCache: 400, maxFailed: 3000, timeoutMs: 30_000 }) {}

  currentEpoch() {
    return this.epoch;
  }

  /** Nueva carpeta: lo que estaba en cola ya no interesa. */
  newEpoch(): number {
    this.epoch++;
    const dropped = this.queue;
    this.queue = [];
    for (const r of dropped) {
      this.pending.delete(r.key);
      r.queued = false;
      r.settle({ ok: false, kind: "cancelled" });
    }
    void Promise.resolve(this.backend.setThumbEpoch(this.epoch)).catch(() => {});
    return this.epoch;
  }

  cached(key: string): string | undefined {
    const v = this.cache.get(key);
    if (v !== undefined) {
      this.cache.delete(key); // se marca como usada recientemente
      this.cache.set(key, v);
    }
    return v;
  }

  failure(key: string): ThumbFailKind | undefined {
    return this.failed.get(key);
  }

  clear() {
    this.cache.clear();
    this.failed.clear();
    this.listeners.forEach((l) => l()); // los elementos visibles vuelven a pedir su miniatura
  }

  onClear(l: () => void): () => void {
    this.listeners.add(l);
    return () => void this.listeners.delete(l);
  }

  /** Descarta todo: cola, solicitudes en vuelo, caché y fallos (las respuestas tardías se ignoran). */
  reset() {
    this.gen++;
    for (const r of this.queue) r.settle({ ok: false, kind: "cancelled" });
    this.queue = [];
    this.pending.clear();
    this.inflight = 0;
    this.clear();
  }

  stats() {
    return { queued: this.queue.length, inflight: this.inflight, cached: this.cache.size, failed: this.failed.size };
  }

  /** `front`: salta la cola (vista previa del elemento seleccionado). */
  request(key: string, path: string, px: number, front = false): ThumbHandle {
    const hit = this.cached(key);
    if (hit !== undefined) return { promise: Promise.resolve({ ok: true, url: hit }), release() {} };
    const known = this.failed.get(key);
    if (known) return { promise: Promise.resolve({ ok: false, kind: known }), release() {} };

    let req = this.pending.get(key);
    if (!req) {
      let settle!: (o: ThumbOutcome) => void;
      const promise = new Promise<ThumbOutcome>((r) => (settle = r));
      req = { key, path, px, epoch: this.epoch, subs: 0, queued: true, gen: this.gen, promise, settle };
      this.pending.set(key, req);
      front ? this.queue.unshift(req) : this.queue.push(req);
      this.pump();
    }
    req.subs++;
    const r = req;
    let released = false;
    return {
      promise: r.promise,
      release: () => {
        if (released) return;
        released = true;
        r.subs--;
        if (r.subs <= 0 && r.queued) {
          // Nadie lo espera y aún no salió hacia el backend: se retira de la cola.
          this.queue = this.queue.filter((q) => q !== r);
          this.pending.delete(r.key);
          r.queued = false;
          r.settle({ ok: false, kind: "cancelled" });
        }
      },
    };
  }

  private remember(key: string, kind: ThumbFailKind) {
    this.failed.set(key, kind);
    if (this.failed.size > this.opts.maxFailed) this.failed.delete(this.failed.keys().next().value as string);
  }

  private pump() {
    while (this.inflight < this.opts.maxInflight && this.queue.length) {
      const r = this.queue.shift() as Req;
      r.queued = false;
      r.gen = this.gen;
      this.inflight++;
      let freed = false; // el hueco se libera una sola vez (por respuesta o por tiempo límite)
      const free = () => {
        if (freed) return;
        freed = true;
        if (r.gen === this.gen) this.inflight--;
        this.pump();
      };
      const timer = setTimeout(() => {
        this.pending.delete(r.key);
        this.remember(r.key, "failed");
        r.settle({ ok: false, kind: "failed" });
        free();
      }, this.opts.timeoutMs);
      this.backend
        .getThumbnail(r.path, r.px, r.epoch)
        .then((reply) => {
          clearTimeout(timer);
          if (r.gen !== this.gen) return; // llegó tras un reset(): se ignora
          this.pending.delete(r.key); // ya no está pendiente: una petición nueva idéntica reutiliza la caché o se repite
          // El resultado es válido aunque la carpeta ya haya cambiado: se guarda por si se vuelve a ella.
          this.cache.set(r.key, reply.dataUrl);
          if (this.cache.size > this.opts.maxCache) this.cache.delete(this.cache.keys().next().value as string);
          r.settle({ ok: true, url: reply.dataUrl });
        })
        .catch((err: unknown) => {
          clearTimeout(timer);
          if (r.gen !== this.gen) return;
          this.pending.delete(r.key);
          const k = (err as { kind?: string } | null)?.kind;
          const kind: ThumbFailKind = k === "unsupported" || k === "toolarge" || k === "cancelled" ? k : "failed";
          if (kind !== "cancelled") this.remember(r.key, kind); // un descarte no es un fallo del archivo
          r.settle({ ok: false, kind });
        })
        .finally(free);
    }
  }
}

/** Instancia de la aplicación. */
export const thumbService = new ThumbService({
  getThumbnail: (p, px, e) => fs.getThumbnail(p, px, e),
  setThumbEpoch: (e) => fs.setThumbEpoch(e),
});
