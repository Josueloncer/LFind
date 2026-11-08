import { create } from "zustand";
import { fs } from "../lib/fs";
import { EMPTY_SNAPSHOT, SearchController } from "../lib/searchController";
import type { SearchSnapshot } from "../lib/searchController";
import { DEFAULT_FILTERS, buildQuery, hasCriteria } from "../lib/searchQuery";
import type { SearchFilters } from "../lib/searchQuery";

/**
 * Búsqueda global: estado de la interfaz (ámbito, texto, filtros) + resultados. La lógica de esperar, cancelar y paginar vive en
 * `SearchController` (probado aparte). El ámbito «folder» es el filtro de siempre de la carpeta abierta (lo gestiona `Pane`);
 * «global» consulta el índice de todas las unidades.
 */
export type Scope = "folder" | "global";

interface SearchState {
  scope: Scope;
  text: string;
  filters: SearchFilters;
  filtersOpen: boolean;
  snap: SearchSnapshot;
  /** Se incrementa cada vez que algo pide llevar el cursor al campo de búsqueda (lo atiende la barra superior). */
  focusTick: number;
  /** Ruta del resultado seleccionado. */
  selected: string | null;
  /** «Abrir ubicación»: la carpeta que se abre selecciona este archivo en cuanto se lee. */
  reveal: { path: string; at: number } | null;
  /**
   * ÚNICA vía para abrir la búsqueda global: botón de la barra, Ctrl+Mayús+F e icono del campo llaman aquí.
   * Cambia al ámbito global (conserva texto y filtros) y pide el foco del campo, aunque ya estuviera abierta.
   */
  openGlobal(): void;
  setScope(s: Scope): void;
  setText(t: string): void;
  patchFilters(p: Partial<SearchFilters>): void;
  resetFilters(): void;
  toggleFilters(): void;
  select(path: string | null): void;
  loadMore(): void;
  /** Repite la consulta vigente sin tocar lo que se ve (el indexado en curso aporta resultados nuevos). */
  refresh(): void;
  /** Sale de la búsqueda global dejando la carpeta abierta como estaba. */
  leave(): void;
  setReveal(path: string | null): void;
}

let controller: SearchController | null = null;

function ctl(): SearchController {
  return (controller ??= new SearchController((q) => fs.indexSearch(q), (snap) => useSearch.setState({ snap })));
}

/** Aplica el estado actual al controlador: sin texto ni filtros (o fuera del ámbito global) no se consulta nada. */
function sync(s: Pick<SearchState, "scope" | "text" | "filters">, immediate = false) {
  if (s.scope !== "global" || !hasCriteria(s.text, s.filters)) return ctl().setQuery(null);
  // El «ahora» se redondea al minuto: dos sincronizaciones seguidas dan la misma consulta y no se repite el trabajo.
  const now = Math.floor(Date.now() / 60_000) * 60_000;
  ctl().setQuery(buildQuery(s.text, s.filters, { now }), { immediate });
}

export const useSearch = create<SearchState>((set, get) => ({
  scope: "folder",
  text: "",
  filters: DEFAULT_FILTERS,
  filtersOpen: false,
  snap: EMPTY_SNAPSHOT,
  focusTick: 0,
  selected: null,
  reveal: null,

  openGlobal() {
    if (get().scope !== "global") get().setScope("global");
    set((s) => ({ focusTick: s.focusTick + 1 }));
  },
  setScope(scope) {
    set({ scope, selected: null });
    sync(get());
  },
  setText(text) {
    set({ text, selected: null });
    sync(get());
  },
  patchFilters(p) {
    set((s) => ({ filters: { ...s.filters, ...p }, selected: null }));
    sync(get(), "sort" in p || "desc" in p);
  },
  resetFilters() {
    set({ filters: DEFAULT_FILTERS, selected: null });
    sync(get());
  },
  toggleFilters: () => set((s) => ({ filtersOpen: !s.filtersOpen })),
  select: (selected) => set({ selected }),
  loadMore: () => void ctl().loadMore(),
  refresh: () => ctl().refresh(),
  leave() {
    set({ scope: "folder", text: "", selected: null });
    sync(get());
  },
  setReveal: (path) => set({ reveal: path ? { path, at: Date.now() } : null }),
}));
