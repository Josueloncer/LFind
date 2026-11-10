import { create } from "zustand";
import { bakeBackground } from "../lib/backgroundImage";
import { fs } from "../lib/fs";

interface BackgroundsState {
  /** Imágenes guardadas, ya cargadas. Solo se conservan las que están en uso. */
  urls: Record<string, string>;
  /** Las mismas con el desenfoque y el contraste aplicados: son las que se pintan. */
  baked: Record<string, string>;
  /** No se pudo preparar una imagen: se recurre al filtro en vivo, más costoso. */
  liveFilter: boolean;
  ensure(ids: readonly string[]): void;
  /** Prepara las imágenes con estos ajustes. Las peticiones seguidas (un control deslizante) se resuelven en una sola, la última. */
  bake(blur: number, contrast: number): void;
}

const pick = (o: Record<string, string>, ids: readonly string[]) => Object.fromEntries(Object.entries(o).filter(([k]) => ids.includes(k)));

let wanted: readonly string[] = [];
const loading = new Set<string>();
let pending: { blur: number; contrast: number } | null = null;
let baking = false;
let bakedWith: Record<string, string> = {};
let viewSide = 0;

/** Tamaño de la ventana para preparar las imágenes. Cambios pequeños no merecen repetir el trabajo. */
function currentSide(): number {
  const now = Math.max(window.innerWidth, window.innerHeight);
  if (!viewSide || Math.abs(now - viewSide) / viewSide > 0.1) viewSide = now;
  return viewSide;
}

export const useBackgrounds = create<BackgroundsState>((set, get) => {
  const pump = async () => {
    baking = true;
    while (pending) {
      const { blur, contrast } = pending;
      pending = null;
      const side = currentSide();
      const key = `${blur}:${contrast}:${side}`;
      for (const [id, url] of Object.entries(get().urls)) {
        if (bakedWith[id] === key) continue;
        let out = url;
        let live = false;
        try {
          out = await bakeBackground(url, blur, contrast, side);
        } catch {
          live = true;
        }
        if (!get().urls[id]) continue;
        bakedWith[id] = key;
        set((s) => ({ baked: { ...s.baked, [id]: out }, liveFilter: s.liveFilter || live }));
      }
    }
    baking = false;
  };

  return {
    urls: {},
    baked: {},
    liveFilter: false,
    ensure(ids) {
      wanted = ids;
      const { urls: cur, baked } = get();
      const kept = pick(cur, ids);
      if (Object.keys(kept).length !== Object.keys(cur).length) {
        set({ urls: kept, baked: pick(baked, ids) });
        bakedWith = pick(bakedWith, ids);
      }
      for (const id of ids) {
        if (kept[id] || loading.has(id) || !fs.available) continue;
        loading.add(id);
        void fs.backgroundLoad(id).then((url) => {
          if (url && wanted.includes(id)) set((s) => ({ urls: { ...s.urls, [id]: url } }));
        }).catch(() => {}).finally(() => loading.delete(id));
      }
    },
    bake(blur, contrast) {
      pending = { blur, contrast };
      if (!baking) void pump();
    },
  };
});

/** Solo para las pruebas. */
export function resetBackgroundsForTest(): void {
  wanted = [];
  loading.clear();
  pending = null;
  baking = false;
  bakedWith = {};
  viewSide = 0;
  useBackgrounds.setState({ urls: {}, baked: {}, liveFilter: false });
}
