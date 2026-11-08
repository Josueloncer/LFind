import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SearchHit, SearchPage, SearchQuery } from "./fs/types";
import { SearchController } from "./searchController";
import type { SearchSnapshot } from "./searchController";
import { DEFAULT_FILTERS, buildQuery } from "./searchQuery";

const NOW = 1_800_000_000_000;
const Q = (text: string, extra: Partial<SearchQuery> = {}): SearchQuery => ({ ...buildQuery(text, DEFAULT_FILTERS, { now: NOW }), ...extra });
const H = (n: number): SearchHit => ({ path: `C:\\d\\f${n}.txt`, name: `f${n}.txt`, ext: "txt", isDir: false, size: n, modified: null, created: null });
const page = (hits: SearchHit[], more = false, extra: Partial<SearchPage> = {}): SearchPage => ({ hits, more, cancelled: false, elapsedMs: 3, ...extra });

/** Buscador falso: cada llamada queda pendiente hasta que el test la resuelva (permite simular respuestas en cualquier orden). */
function fakeSearcher() {
  const calls: { q: SearchQuery; resolve(p: SearchPage): void; reject(e: unknown): void }[] = [];
  const fn = (q: SearchQuery) => new Promise<SearchPage>((resolve, reject) => void calls.push({ q, resolve, reject }));
  return { fn, calls };
}
const flush = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};

let snaps: SearchSnapshot[];
const make = (s: ReturnType<typeof fakeSearcher>, opts = {}) => new SearchController(s.fn, (x) => snaps.push(x), opts);

beforeEach(() => {
  vi.useFakeTimers();
  snaps = [];
});
afterEach(() => vi.useRealTimers());

describe("SearchController", () => {
  it("debounce: varias pulsaciones seguidas = una sola consulta, la última", async () => {
    const s = fakeSearcher();
    const c = make(s);
    c.setQuery(Q("f"));
    vi.advanceTimersByTime(50);
    c.setQuery(Q("fa"));
    vi.advanceTimersByTime(50);
    c.setQuery(Q("fac"));
    expect(s.calls).toHaveLength(0);
    vi.advanceTimersByTime(119);
    expect(s.calls).toHaveLength(0);
    vi.advanceTimersByTime(1);
    expect(s.calls).toHaveLength(1);
    expect(s.calls[0].q).toMatchObject({ text: "fac", offset: 0, limit: 200 });
  });

  it("la misma consulta no se repite", () => {
    const s = fakeSearcher();
    const c = make(s);
    c.setQuery(Q("foto"));
    vi.advanceTimersByTime(200);
    c.setQuery(Q("foto"));
    vi.advanceTimersByTime(200);
    expect(s.calls).toHaveLength(1);
  });

  it("cancelación: la respuesta de una consulta anterior se ignora aunque llegue después", async () => {
    const s = fakeSearcher();
    const c = make(s);
    c.setQuery(Q("lenta"), { immediate: true });
    c.setQuery(Q("rapida"), { immediate: true });
    expect(s.calls.map((x) => x.q.text)).toEqual(["lenta", "rapida"]);
    s.calls[1].resolve(page([H(1)]));
    await flush();
    s.calls[0].resolve(page([H(2), H(3)])); // tarde
    await flush();
    expect(c.snapshot.hits.map((h) => h.name)).toEqual(["f1.txt"]);
    expect(c.snapshot.loading).toBe(false);
  });

  it("sin parpadeo: los resultados anteriores se conservan mientras llegan los nuevos", async () => {
    const s = fakeSearcher();
    const c = make(s);
    c.setQuery(Q("a"), { immediate: true });
    s.calls[0].resolve(page([H(1), H(2)]));
    await flush();
    c.setQuery(Q("ab"));
    expect(c.snapshot.loading).toBe(true);
    expect(c.snapshot.hits).toHaveLength(2);
    vi.advanceTimersByTime(120);
    s.calls[1].resolve(page([H(1)]));
    await flush();
    expect(c.snapshot).toMatchObject({ loading: false, hits: [H(1)], elapsedMs: 3 });
    // en ningún momento de la transición la lista quedó vacía
    expect(snaps.slice(1).every((x) => x.hits.length > 0)).toBe(true);
  });

  it("un error vacía la lista y se muestra; la siguiente consulta lo limpia", async () => {
    const s = fakeSearcher();
    const c = make(s);
    c.setQuery(Q("a"), { immediate: true });
    s.calls[0].reject("El índice de búsqueda no está disponible.");
    await flush();
    expect(c.snapshot).toMatchObject({ error: "El índice de búsqueda no está disponible.", hits: [], loading: false });
    c.setQuery(Q("b"), { immediate: true });
    expect(c.snapshot.error).toBeNull();
    s.calls[1].resolve(page([H(1)]));
    await flush();
    expect(c.snapshot.hits).toHaveLength(1);
  });

  it("errores con forma de objeto o Error", async () => {
    const s = fakeSearcher();
    const c = make(s);
    c.setQuery(Q("a"), { immediate: true });
    s.calls[0].reject({ kind: "other", message: "disco ocupado" });
    await flush();
    expect(c.snapshot.error).toBe("disco ocupado");
    c.setQuery(Q("b"), { immediate: true });
    s.calls[1].reject(new Error("boom"));
    await flush();
    expect(c.snapshot.error).toBe("boom");
  });

  it("setQuery(null) limpia todo e ignora lo que estaba en vuelo", async () => {
    const s = fakeSearcher();
    const c = make(s);
    c.setQuery(Q("a"), { immediate: true });
    c.setQuery(null);
    s.calls[0].resolve(page([H(1)]));
    await flush();
    expect(c.snapshot).toMatchObject({ active: false, hits: [], loading: false });
    c.setQuery(Q("x")); // la consulta pendiente por debounce también se cancela con null
    c.setQuery(null);
    vi.advanceTimersByTime(500);
    expect(s.calls).toHaveLength(1);
  });

  it("paginación: pide el siguiente offset, no repite rutas y se detiene sin más resultados", async () => {
    const s = fakeSearcher();
    const c = make(s, { pageSize: 3 });
    c.setQuery(Q("f"), { immediate: true });
    s.calls[0].resolve(page([H(1), H(2), H(3)], true));
    await flush();
    const p = c.loadMore();
    void c.loadMore(); // concurrente: se ignora
    expect(s.calls).toHaveLength(2);
    expect(s.calls[1].q).toMatchObject({ offset: 3, limit: 3 });
    s.calls[1].resolve(page([H(3), H(4), H(5)], true)); // H3 repetida (el índice cambió mientras tanto)
    await p;
    expect(c.snapshot.hits.map((h) => h.size)).toEqual([1, 2, 3, 4, 5]);
    const p2 = c.loadMore();
    expect(s.calls[2].q.offset).toBe(6); // se pidieron 6 filas en total: el offset cuenta filas pedidas, no únicas
    s.calls[2].resolve(page([H(7)], false));
    await p2;
    expect(c.snapshot.more).toBe(false);
    await c.loadMore();
    expect(s.calls).toHaveLength(3);
  });

  it("una consulta nueva durante loadMore descarta esa página", async () => {
    const s = fakeSearcher();
    const c = make(s, { pageSize: 2 });
    c.setQuery(Q("f"), { immediate: true });
    s.calls[0].resolve(page([H(1), H(2)], true));
    await flush();
    const p = c.loadMore();
    c.setQuery(Q("g"), { immediate: true });
    s.calls[1].resolve(page([H(8), H(9)], true)); // página de la consulta vieja
    await p;
    expect(c.snapshot.hits).toHaveLength(2); // los de la consulta vieja
    s.calls[2].resolve(page([H(50)]));
    await flush();
    expect(c.snapshot.hits.map((h) => h.size)).toEqual([50]);
    expect(c.snapshot.loadingMore).toBe(false);
  });

  it("grandes cantidades: se carga por páginas hasta el tope de memoria y no más", async () => {
    const TOTAL = 50_000;
    const searcher = async (q: SearchQuery): Promise<SearchPage> => {
      const hits: SearchHit[] = [];
      for (let i = q.offset; i < Math.min(q.offset + q.limit, TOTAL); i++) hits.push(H(i));
      return page(hits, q.offset + q.limit < TOTAL);
    };
    const c = new SearchController(searcher, () => {}, { pageSize: 200, maxLoaded: 1000 });
    c.setQuery(Q("f"), { immediate: true });
    await flush();
    expect(c.snapshot.hits).toHaveLength(200);
    let guard = 0;
    while (c.snapshot.more && guard++ < 100) await c.loadMore();
    expect(c.snapshot.hits).toHaveLength(1000);
    expect(c.snapshot.more).toBe(false);
    expect(new Set(c.snapshot.hits.map((h) => h.path)).size).toBe(1000);
  });

  it("refresh en silencio: no activa «cargando» y sustituye los resultados; si falla, conserva los que había", async () => {
    const s = fakeSearcher();
    const c = make(s, { pageSize: 5 });
    c.setQuery(Q("f"), { immediate: true });
    s.calls[0].resolve(page([H(1)]));
    await flush();
    snaps.length = 0;
    c.refresh();
    expect(c.snapshot.loading).toBe(false);
    s.calls[1].resolve(page([H(1), H(2)]));
    await flush();
    expect(c.snapshot.hits).toHaveLength(2);
    expect(snaps.every((x) => !x.loading)).toBe(true);
    c.refresh();
    s.calls[2].reject("fallo puntual");
    await flush();
    expect(c.snapshot.hits).toHaveLength(2);
    expect(c.snapshot.error).toBeNull();
  });

  it("refresh no hace nada si ya se paginó (no mueve la lista bajo el usuario) ni mientras hay una consulta en curso", async () => {
    const s = fakeSearcher();
    const c = make(s, { pageSize: 2 });
    c.setQuery(Q("f"), { immediate: true });
    c.refresh(); // en curso
    expect(s.calls).toHaveLength(1);
    s.calls[0].resolve(page([H(1), H(2)], true));
    await flush();
    const p = c.loadMore();
    s.calls[1].resolve(page([H(3), H(4)], true));
    await p;
    c.refresh();
    expect(s.calls).toHaveLength(2);
  });

  it("una página «cancelada» por un tercero se repite una vez", async () => {
    const s = fakeSearcher();
    const c = make(s);
    c.setQuery(Q("f"), { immediate: true });
    s.calls[0].resolve(page([], false, { cancelled: true }));
    await flush();
    expect(s.calls).toHaveLength(2);
    s.calls[1].resolve(page([], false, { cancelled: true }));
    await flush();
    expect(s.calls).toHaveLength(2);
    expect(c.snapshot.loading).toBe(false);
  });

  it("dispose: nada más se emite", async () => {
    const s = fakeSearcher();
    const c = make(s);
    c.setQuery(Q("f"), { immediate: true });
    c.dispose();
    const n = snaps.length;
    s.calls[0].resolve(page([H(1)]));
    await flush();
    c.setQuery(Q("g"));
    expect(snaps).toHaveLength(n);
  });
});
