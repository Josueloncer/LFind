import { isPinned } from "../lib/locations";
import { usePrefs } from "./prefs";
import { useUi } from "./ui";

/** Fija o quita una carpeta de Ubicaciones. Al fijarla, el aviso ofrece elegir su icono en el momento. */
export function togglePinWithIconOffer(path: string, label: string): void {
  const prefs = usePrefs.getState();
  const was = isPinned(prefs.favorites, path);
  prefs.toggleFavorite(path, label);
  if (was) return;
  const ui = useUi.getState();
  ui.toast(`«${label}» fijada en Ubicaciones`, "success", { label: "Elegir icono", run: () => useUi.getState().openDialog({ type: "folderIcon", path }) });
}
