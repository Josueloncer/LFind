import type { FolderView, GroupBy, Prefs, SortDir, SortKey, ViewMode } from "./fs";

export interface EffectiveView {
  viewMode: ViewMode;
  iconSize: number;
  sortKey: SortKey;
  sortDir: SortDir;
  groupBy: GroupBy;
}

export const MAX_FOLDER_VIEWS = 300;
export const MAX_RECENTS = 15;

/** Clave estable de una carpeta: sin barra final y sin distinguir mayúsculas. */
export const folderKey = (p: string) => p.replace(/[\\/]+$/, "").toLowerCase() || p;

type ViewPrefs = Pick<Prefs, "viewMode" | "iconSize" | "sortKey" | "sortDir" | "groupBy" | "folderViews">;

const findFolder = (p: Pick<Prefs, "folderViews">, path: string) => {
  const k = folderKey(path);
  return p.folderViews.find((v) => folderKey(v.path) === k);
};

/** ¿El usuario pidió recordar el modo y el tamaño de icono de esta carpeta? */
export const isPinned = (p: Pick<Prefs, "folderViews">, path: string): boolean => findFolder(p, path)?.pinned === true;

/**
 * La vista de una carpeta.
 * - Modo y tamaño de icono: los de la carpeta SOLO si está «fijada» (acción explícita); si no, la vista global.
 * - Orden y agrupación: los recordados para la carpeta, o los globales si no hay ninguno.
 */
export function effectiveView(p: ViewPrefs, path: string): EffectiveView {
  const f = findFolder(p, path);
  const look = f?.pinned ? f : p;
  const order = f ?? p;
  return { viewMode: look.viewMode, iconSize: look.iconSize, sortKey: order.sortKey, sortDir: order.sortDir, groupBy: order.groupBy };
}

/** Guarda un cambio solo para esa carpeta; la lista se limita a las 300 más recientes. */
export function withFolderView(p: ViewPrefs, path: string, patch: Partial<EffectiveView>, pinned?: boolean): FolderView[] {
  const k = folderKey(path);
  const prev = findFolder(p, path);
  const next: FolderView = { ...effectiveView(p, path), ...patch, path, pinned: pinned ?? prev?.pinned ?? false };
  return [...p.folderViews.filter((v) => folderKey(v.path) !== k), next].slice(-MAX_FOLDER_VIEWS);
}

/** Separa un cambio de vista en lo que va a la vista GLOBAL (modo, tamaño) y lo que es de la carpeta (orden, agrupación). */
export function splitViewPatch(patch: Partial<EffectiveView>): { look: Partial<Pick<EffectiveView, "viewMode" | "iconSize">>; order: Partial<Pick<EffectiveView, "sortKey" | "sortDir" | "groupBy">> } {
  const { viewMode, iconSize, ...order } = patch;
  const look: Partial<Pick<EffectiveView, "viewMode" | "iconSize">> = {};
  if (viewMode !== undefined) look.viewMode = viewMode;
  if (iconSize !== undefined) look.iconSize = iconSize;
  return { look, order };
}

/** Aplica un cambio de vista según la regla A: modo/tamaño → global (o la carpeta, si está fijada); orden → carpeta. */
export function applyViewChange(p: ViewPrefs, path: string, patch: Partial<EffectiveView>): Partial<Pick<Prefs, "viewMode" | "iconSize" | "folderViews">> {
  const { look, order } = splitViewPatch(patch);
  if (isPinned(p, path)) return { folderViews: withFolderView(p, path, patch) };
  const out: Partial<Pick<Prefs, "viewMode" | "iconSize" | "folderViews">> = { ...look };
  if (Object.keys(order).length > 0) out.folderViews = withFolderView(p, path, order);
  return out;
}

/** Fija (o suelta) el modo y el tamaño de esta carpeta. Al fijar se congela la vista que se está viendo. */
export function withPinned(p: ViewPrefs, path: string, pinned: boolean): FolderView[] {
  return withFolderView(p, path, pinned ? effectiveView(p, path) : {}, pinned);
}

export function withRecent(recents: string[], path: string): string[] {
  const k = folderKey(path);
  return [path, ...recents.filter((r) => folderKey(r) !== k)].slice(0, MAX_RECENTS);
}
