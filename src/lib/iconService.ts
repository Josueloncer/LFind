import { fs } from "./fs";
import type { IconItem, IconsReply } from "./fs";
import { sharedKey } from "./iconKinds";
import type { Entry } from "./fs";

export interface IconBackend {
  getIcons(items: IconItem[], px: number, have: string[]): Promise<IconsReply>;
}

export interface IconTarget {
  path: string;
  entry: Pick<Entry, "kind" | "name" | "size" | "modified">;
}

export interface IconOptions {
  /** Espera para juntar solicitudes en un único lote (ms). */
  batchDelayMs: number;
  /** Máximo de elementos por lote (el backend rechaza más de 512). */
  maxBatch: number;
  maxInflight: number;
  /** Iconos propios de archivo/carpeta que se conservan en memoria. */
  maxPerFile: number;
  maxNone: number;
  timeoutMs: number;
}

interface Waiter {
  id: string; // identifica el elemento (ruta + tamaño + fecha + px)
  path: string;
  dir: boolean;
  px: number;
  shared: string | null;
  subs: number;
  settle(url: string | null): void;
  promise: Promise<string | null>;
}

export interface IconHandle {
  promise: Promise<string | null>;
  release(): void;
}

/**
 * Cola de iconos del sistema, pensada para NO hacer una llamada por archivo:
 *  - los iconos por tipo (`t-docx-64`) se guardan por clave: mil .docx visibles = una extracción, una vez por sesión;
 *  - las solicitudes que llegan juntas (una pantalla de elementos) salen en UN solo lote hacia el backend;
 *  - solo se piden elementos montados: lo que sale de pantalla antes de salir el lote se retira;
 *  - al cambiar de carpeta (`newEpoch`) se descarta lo que aún no salió;
 *  - se envía `have` (claves compartidas ya conocidas) para no recibir bytes repetidos;
 *  - memoria acotada (LRU) y memoria de «sin icono» para no reintentar en cada desplazamiento.
 * El `id` de un elemento incluye su fecha y tamaño: si el archivo cambia, se vuelve a preguntar (y el backend, con su
 * propia clave de ruta + fecha + tamaño, vuelve a extraer).
 */
export class IconService {
  private urls = new Map<string, string>(); // clave del backend → data URL
  private shared = new Set<string>(); // claves compartidas (nunca se expulsan: son pocas)
  private perFile = new Map<string, string>(); // id de elemento → clave (solo con icono propio o de carpeta)
  private none = new Map<string, true>(); // ids sin icono del sistema
  private noneShared = new Set<string>(); // tipos para los que el sistema no tiene icono (no se reintentan)
  private pending = new Map<string, Waiter>();
  private queue: Waiter[] = [];
  private timer: ReturnType<typeof setTimeout> | null = null;
  private inflight = 0;
  private gen = 0;
  private listeners = new Set<() => void>();

  constructor(private backend: IconBackend, private opts: IconOptions = { batchDelayMs: 24, maxBatch: 256, maxInflight: 2, maxPerFile: 1500, maxNone: 5000, timeoutMs: 20_000 }) {}

  static id(t: IconTarget, px: number): string {
    return `${px}|${t.path}|${t.entry.size}|${t.entry.modified ?? 0}`;
  }

  /** Icono ya conocido: `string` = listo, `null` = el sistema no tiene (usar glifo), `undefined` = aún no se sabe. */
  peek(t: IconTarget, px: number): string | null | undefined {
    const sk = sharedKey(t.entry, px);
    if (sk) {
      const u = this.urls.get(sk);
      if (u !== undefined) return u;
    }
    const id = IconService.id(t, px);
    const k = this.perFile.get(id);
    if (k !== undefined) {
      const u = this.urls.get(k);
      if (u !== undefined) {
        this.perFile.delete(id); // se marca como usado recientemente
        this.perFile.set(id, k);
        return u;
      }
      this.perFile.delete(id); // el icono salió de la memoria: se vuelve a pedir
    }
    if (sk && this.noneShared.has(sk)) return null;
    if (this.none.has(id)) return null;
    return undefined;
  }

  request(t: IconTarget, px: number): IconHandle {
    const known = this.peek(t, px);
    if (known !== undefined) return { promise: Promise.resolve(known), release() {} };

    const id = IconService.id(t, px);
    let w = this.pending.get(id);
    if (!w) {
      let settle!: (u: string | null) => void;
      const promise = new Promise<string | null>((r) => (settle = r));
      w = { id, path: t.path, dir: t.entry.kind === "dir", px, shared: sharedKey(t.entry, px), subs: 0, settle, promise };
      this.pending.set(id, w);
      this.queue.push(w);
      this.schedule();
    }
    w.subs++;
    const r = w;
    let released = false;
    return {
      promise: r.promise,
      release: () => {
        if (released) return;
        released = true;
        r.subs--;
        if (r.subs <= 0 && this.queue.includes(r)) {
          this.queue = this.queue.filter((q) => q !== r); // nadie lo espera y aún no salió: se retira
          this.pending.delete(r.id);
          r.settle(null);
        }
      },
    };
  }

  /** Carpeta nueva: lo que estaba en cola ya no interesa. */
  newEpoch() {
    const dropped = this.queue;
    this.queue = [];
    for (const w of dropped) {
      this.pending.delete(w.id);
      w.settle(null);
    }
  }

  /** Vacía todo (p. ej. tras «Borrar la caché»): los elementos visibles vuelven a pedir su icono. */
  clear() {
    this.gen++;
    this.newEpoch();
    this.urls.clear();
    this.shared.clear();
    this.perFile.clear();
    this.none.clear();
    this.noneShared.clear();
    this.inflight = 0;
    this.listeners.forEach((l) => l());
  }

  onClear(l: () => void): () => void {
    this.listeners.add(l);
    return () => void this.listeners.delete(l);
  }

  stats() {
    return { queued: this.queue.length, inflight: this.inflight, urls: this.urls.size, perFile: this.perFile.size, none: this.none.size };
  }

  private schedule() {
    if (this.timer || this.inflight >= this.opts.maxInflight) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      this.pump();
    }, this.opts.batchDelayMs);
  }

  private pump() {
    while (this.inflight < this.opts.maxInflight && this.queue.length) {
      // Un lote = un solo tamaño (px). El tope cuenta ELEMENTOS ENVIADOS: los archivos de un tipo ya incluido en el
      // lote no ocupan sitio (comparten el icono del representante).
      const px = this.queue[0].px;
      const batch: Waiter[] = [];
      const rest: Waiter[] = [];
      const seen = new Set<string>();
      let sent = 0;
      for (const w of this.queue) {
        if (w.px !== px) rest.push(w);
        else if (w.shared && seen.has(w.shared)) batch.push(w);
        else if (sent < this.opts.maxBatch) {
          batch.push(w);
          sent++;
          if (w.shared) seen.add(w.shared);
        } else rest.push(w);
      }
      this.queue = rest;
      this.send(batch, px);
    }
  }

  private send(batch: Waiter[], px: number) {
    // Los de clave compartida se agrupan: basta UN representante por tipo; el resto espera ese mismo resultado.
    const reps: Waiter[] = [];
    const byShared = new Map<string, Waiter[]>();
    for (const w of batch) {
      if (w.shared) {
        const g = byShared.get(w.shared);
        if (g) g.push(w);
        else {
          byShared.set(w.shared, [w]);
          reps.push(w);
        }
      } else reps.push(w);
    }
    const have = [...this.shared].filter((k) => k.endsWith(`-${px}`));
    const gen = this.gen;
    this.inflight++;
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      if (gen === this.gen) this.inflight--;
      if (this.queue.length) this.schedule();
    };
    const timer = setTimeout(() => {
      for (const w of batch) this.settleNone(w, false);
      finish();
    }, this.opts.timeoutMs);

    this.backend
      .getIcons(reps.map((w) => ({ path: w.path, dir: w.dir })), px, have)
      .then((reply) => {
        clearTimeout(timer);
        if (gen !== this.gen) return; // llegó tras un clear(): se ignora
        for (const [k, u] of Object.entries(reply.icons)) {
          this.urls.set(k, u);
          if (k.startsWith("t-") || k.startsWith("d-")) this.shared.add(k);
        }
        reps.forEach((w, i) => {
          const k = reply.keys[i] ?? null;
          const group = (w.shared && byShared.get(w.shared)) || [w];
          if (k === null || (this.urls.get(k) === undefined && !this.shared.has(k))) {
            for (const g of group) this.settleNone(g);
            return;
          }
          // Una clave compartida ya conocida por la interfaz (`have`) no trae bytes: se reutiliza la de memoria.
          const url = this.urls.get(k) ?? null;
          for (const g of group) {
            // Un archivo de tipo compartido se identifica por su clave de tipo; el resto, por su id.
            if (!(g.shared && g.shared === k)) this.rememberPerFile(g.id, k);
            this.pending.delete(g.id);
            g.settle(url);
          }
        });
      })
      .catch(() => {
        clearTimeout(timer);
        if (gen !== this.gen) return;
        for (const w of batch) this.settleNone(w, false);
      })
      .finally(finish);
  }

  private settleNone(w: Waiter, remember = true) {
    this.pending.delete(w.id);
    if (remember && w.shared) {
      this.noneShared.add(w.shared);
    } else if (remember) {
      this.none.set(w.id, true);
      if (this.none.size > this.opts.maxNone) this.none.delete(this.none.keys().next().value as string);
    }
    w.settle(null);
  }

  private rememberPerFile(id: string, key: string) {
    this.perFile.set(id, key);
    if (this.perFile.size > this.opts.maxPerFile) {
      const old = this.perFile.keys().next().value as string;
      const k = this.perFile.get(old) as string;
      this.perFile.delete(old);
      if (!this.shared.has(k)) this.urls.delete(k);
    }
  }
}

export const iconService = new IconService({ getIcons: (i, px, have) => fs.getIcons(i, px, have) });
