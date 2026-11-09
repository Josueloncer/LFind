import { create } from "zustand";
import { zoneKey } from "../lib/appearance";
import { bakeBackground } from "../lib/backgroundImage";
import { fs } from "../lib/fs";

interface BackgroundsState {
  /** Imágenes guardadas, ya cargadas. Solo se conservan las que están en uso. */
  urls: Record<string, string>;
  /** Las mismas con el desenfoque y el contraste aplicados: son las que se pintan. */
  baked: Record<string, string>;
  /** Copias con el desenfoque de una zona que no coincide con el de la ventana (`zoneKey`). */
  zoneBaked: Record<string, string>;
  /** No se pudo preparar una imagen: se recurre al filtro en vivo, más costoso. */
  liveFilter: boolean;
  ensure(ids: readonly string[]): void;
  /** Prepara las imágenes con estos ajustes. Las peticiones seguidas (un control deslizante) se resuelven en una sola, la última. */
  bake(blur: number, contrast: number, zones?: readonly ZoneBake[]): void;
}

type ZoneBake = { id: string; blur: number };

const pick = (o: Record<string, string>, ids: readonly string[]) => Object.fromEntries(Object.entries(o).filter(([k]) => ids.includes(k)));

let wanted: readonly string[] = [];
const loading = new Set<string>();
let pending: { blur: number; contrast: number; zones: readonly ZoneBake[] } | null = null;
let baking = false;
let bakedWith: Record<string, string> = {};
let zoneWith: Record<string, string> = {};
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
      const { blur, contrast, zones } = pending;
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
      const tag = `${contrast}:${side}`;
      for (const z of zones) {
        const k = zoneKey(z.id, z.blur);
        const url = get().urls[z.id];
        if (!url || zoneWith[k] === tag) continue;
        let out = url;
        let live = false;
        try {
          out = await bakeBackground(url, z.blur, contrast, side);
        } catch {
          live = true;
        }
        if (!get().urls[z.id]) continue;
        zoneWith[k] = tag;
        set((s) => ({ zoneBaked: { ...s.zoneBaked, [k]: out }, liveFilter: s.liveFilter || live }));
      }
      // Solo cuando ya no queda nada por hacer: lo que se está viendo no desaparece a mitad de un deslizamiento.
      if (!pending) {
        const used = zones.map((z) => zoneKey(z.id, z.blur));
        zoneWith = pick(zoneWith, used);
        if (Object.keys(get().zoneBaked).some((k) => !used.includes(k))) set((s) => ({ zoneBaked: pick(s.zoneBaked, used) }));
      }
    }
    baking = false;
  };

  return {
    urls: {},
    baked: {},
    zoneBaked: {},
    liveFilter: false,
    ensure(ids) {
      wanted = ids;
      const { urls: cur, baked } = get();
      const kept = pick(cur, ids);
      if (Object.keys(kept).length !== Object.keys(cur).length) {
        const zoneKept = Object.keys(get().zoneBaked).filter((k) => ids.includes(k.split("@")[0]));
        set({ urls: kept, baked: pick(baked, ids), zoneBaked: pick(get().zoneBaked, zoneKept) });
        zoneWith = pick(zoneWith, zoneKept);
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
    bake(blur, contrast, zones = []) {
      pending = { blur, contrast, zones };
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
  zoneWith = {};
  viewSide = 0;
  useBackgrounds.setState({ urls: {}, baked: {}, zoneBaked: {}, liveFilter: false });
}
