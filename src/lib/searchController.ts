// Motor de la interfaz de búsqueda: espera (debounce), descarta respuestas de consultas anteriores, pagina y se refresca en silencio.
// Sin React ni IPC (recibe la función que consulta): se prueba aislado con temporizadores falsos (`searchController.test.ts`).
//
// Cancelación en dos niveles: aquí, una consulta nueva invalida a la anterior (su respuesta se ignora aunque llegue); en Rust,
// `index_search` activa la bandera de cancelación de la consulta anterior y SQLite la interrumpe a media ejecución.

import type { SearchHit, SearchPage, SearchQuery } from "./fs/types";
import { PAGE_SIZE, appendUnique } from "./searchQuery";

export interface SearchSnapshot {
  /** Hay una consulta vigente (con texto o filtros). */
  active: boolean;
  hits: SearchHit[];
  more: boolean;
  /** Hay una consulta en curso; los resultados anteriores se conservan hasta que llegan los nuevos (sin parpadeo). */
  loading: boolean;
  loadingMore: boolean;
  error: string | null;
  elapsedMs: number | null;
}

export const EMPTY_SNAPSHOT: SearchSnapshot = { active: false, hits: [], more: false, loading: false, loadingMore: false, error: null, elapsedMs: null };

export type Searcher = (q: SearchQuery) => Promise<SearchPage>;

export interface ControllerOpts {
  debounceMs?: number;
  pageSize?: number;
  /** Tope de resultados que se cargan en memoria de la interfaz; el resto se alcanza refinando la búsqueda. */
  maxLoaded?: number;
}

const msg = (e: unknown): string => (typeof e === "string" ? e : e instanceof Error ? e.message : (e as { message?: string } | null)?.message ?? String(e));

export class SearchController {
  private snap: SearchSnapshot = EMPTY_SNAPSHOT;
  private query: SearchQuery | null = null;
  private key = "";
  private seq = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private nextOffset = 0;
  private disposed = false;
  private readonly debounceMs: number;
  private readonly pageSize: number;
  private readonly maxLoaded: number;

  constructor(private readonly searcher: Searcher, private readonly onChange: (s: SearchSnapshot) => void, opts: ControllerOpts = {}) {
    this.debounceMs = opts.debounceMs ?? 120;
    this.pageSize = opts.pageSize ?? PAGE_SIZE;
    this.maxLoaded = opts.maxLoaded ?? 5000;
  }

  get snapshot(): SearchSnapshot {
    return this.snap;
  }

  private set(p: Partial<SearchSnapshot>) {
    if (this.disposed) return;
    this.snap = { ...this.snap, ...p };
    this.onChange(this.snap);
  }

  private clearTimer() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  /** Establece la consulta vigente (`null` = sin búsqueda). Una igual a la vigente no hace nada. `offset` y `limit` los decide el controlador. */
  setQuery(q: SearchQuery | null, opts: { immediate?: boolean } = {}) {
    if (this.disposed) return;
    if (q === null) {
      this.clearTimer();
      this.seq++;
      this.query = null;
      this.key = "";
      this.nextOffset = 0;
      this.set(EMPTY_SNAPSHOT);
      return;
    }
    const base = { ...q, offset: 0, limit: this.pageSize };
    const key = JSON.stringify(base);
    if (key === this.key) return;
    this.key = key;
    this.query = base;
    this.seq++; // lo que estuviera en vuelo queda obsoleto
    this.clearTimer();
    this.set({ active: true, loading: true, loadingMore: false, error: null });
    if (opts.immediate) void this.run(this.seq, false, 0);
    else {
      const id = this.seq;
      this.timer = setTimeout(() => ((this.timer = null), void this.run(id, false, 0)), this.debounceMs);
    }
  }

  private async run(id: number, silent: boolean, retries: number): Promise<void> {
    const q = this.query;
    if (!q || id !== this.seq) return;
    try {
      const page = await this.searcher(q);
      if (id !== this.seq || this.disposed) return; // llegó tarde: ya hay otra consulta vigente
      if (page.cancelled) {
        // Cancelada por una consulta ajena a este controlador (p. ej. otra ventana): se repite una vez.
        if (retries < 1) return void this.run(id, silent, retries + 1);
        return this.set({ loading: false });
      }
      this.nextOffset = page.hits.length;
      this.set({ hits: page.hits, more: page.more && page.hits.length < this.maxLoaded, loading: false, error: null, elapsedMs: page.elapsedMs });
    } catch (e) {
      if (id !== this.seq || this.disposed) return;
      if (silent) return this.set({ loading: false }); // un refresco en silencio que falla no borra lo que ya se ve
      this.set({ hits: [], more: false, loading: false, error: msg(e), elapsedMs: null });
    }
  }

  /** Repite la consulta vigente sin tocar lo que se ve (mientras se indexa aparecen resultados nuevos). No hace nada si ya se paginó. */
  refresh() {
    if (this.disposed || !this.query || this.snap.loading || this.snap.loadingMore || this.timer !== null) return;
    if (this.snap.hits.length > this.pageSize) return; // se ha paginado: refrescar movería la lista bajo el usuario
    this.seq++;
    void this.run(this.seq, true, 0);
  }

  /** Pide la página siguiente. */
  async loadMore(): Promise<void> {
    const q = this.query;
    const s = this.snap;
    if (this.disposed || !q || !s.more || s.loading || s.loadingMore || s.hits.length >= this.maxLoaded) return;
    const id = this.seq;
    this.set({ loadingMore: true });
    try {
      const page = await this.searcher({ ...q, offset: this.nextOffset });
      if (id !== this.seq || this.disposed) return;
      if (page.cancelled) return this.set({ loadingMore: false });
      this.nextOffset += page.hits.length;
      const hits = appendUnique(this.snap.hits, page.hits);
      this.set({ hits, more: page.more && hits.length < this.maxLoaded, loadingMore: false, elapsedMs: page.elapsedMs });
    } catch (e) {
      if (id !== this.seq || this.disposed) return;
      this.set({ loadingMore: false, error: msg(e) });
    }
  }

  dispose() {
    this.disposed = true;
    this.clearTimer();
    this.seq++;
  }
}
