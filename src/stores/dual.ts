// Acciones de la vista de doble panel que combinan navegación y preferencias.
import { useNav } from "./nav";
import { usePrefs } from "./prefs";
import { folderKey } from "../lib/viewState";

export const clampSplit = (v: number): number => Math.round(Math.min(800, Math.max(200, v)));

export function setDualSplit(v: number): void {
  usePrefs.getState().update({ dualSplit: clampSplit(v) });
}

export function toggleDualPane(): void {
  const before = useNav.getState();
  const tab = before.other ? before.tabs.find((t) => t.id === before.activeId) : undefined;
  const kept = tab?.orders?.[folderKey(tab.path)];
  useNav.getState().toggleDual();
  // Al volver a un panel, la carpeta que queda a la vista no cambia de orden.
  if (tab && kept) usePrefs.getState().setView(tab.path, kept);
  usePrefs.getState().update({ dualPane: !!useNav.getState().other });
}
