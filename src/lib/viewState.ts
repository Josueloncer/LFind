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

/** La vista de una carpeta: la recordada para ella o, si no la hay, la predeterminada. */
export function effectiveView(p: ViewPrefs, path: string): EffectiveView {
  const k = folderKey(path);
  const f = p.folderViews.find((v) => folderKey(v.path) === k);
  const src = f ?? p;
  return { viewMode: src.viewMode, iconSize: src.iconSize, sortKey: src.sortKey, sortDir: src.sortDir, groupBy: src.groupBy };
}

/** Guarda un cambio solo para esa carpeta; la lista se limita a las 300 más recientes. */
export function withFolderView(p: ViewPrefs, path: string, patch: Partial<EffectiveView>): FolderView[] {
  const k = folderKey(path);
  const next: FolderView = { ...effectiveView(p, path), ...patch, path };
  return [...p.folderViews.filter((v) => folderKey(v.path) !== k), next].slice(-MAX_FOLDER_VIEWS);
}

export function withRecent(recents: string[], path: string): string[] {
  const k = folderKey(path);
  return [path, ...recents.filter((r) => folderKey(r) !== k)].slice(0, MAX_RECENTS);
}
