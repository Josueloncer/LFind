import { useEffect } from "react";
import { useCustomize } from "../stores/customize";
import { usePrefs } from "../stores/prefs";
import { useUi } from "../stores/ui";

/** Aviso flotante del modo de personalización: al ser flotante, abrirlo no desplaza nada de la interfaz. */
export function CustomizeBar() {
  const active = useCustomize((s) => s.active);
  const exit = useCustomize((s) => s.exit);
  const picking = useCustomize((s) => s.picking);
  const pickBusy = useCustomize((s) => s.pickBusy);
  const cancelPick = useCustomize((s) => s.cancelPick);

  useEffect(() => {
    if (!picking) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !useUi.getState().menu && !useUi.getState().dialog) cancelPick();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [picking, cancelPick]);

  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !useUi.getState().menu && !useUi.getState().dialog) exit();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, exit]);

  if (picking) {
    return (
      <div className="cz-bar" role="toolbar" aria-label="Elegir imagen de fondo">
        <span className="cz-text"><b>{pickBusy ? "Preparando la imagen…" : "Elige una imagen de fondo"}</b><span className="cz-hint">Busca en LFind y haz doble clic en la imagen, o selecciónala y pulsa Intro.</span></span>
        <button className="btn" onClick={cancelPick}>Cancelar</button>
      </div>
    );
  }
  if (!active) return null;
  const restore = () => {
    usePrefs.getState().resetLayout();
    useUi.getState().toast("Distribución restaurada. Tus ubicaciones fijadas no han cambiado.", "success");
  };
  return (
    <div className="cz-bar" role="toolbar" aria-label="Personalizar interfaz">
      <span className="cz-text"><b>Personalizar interfaz</b><span className="cz-hint">Arrastra secciones, filas y herramientas. El ojo oculta o muestra cada herramienta.</span></span>
      <button className="btn" onClick={() => useCustomize.getState().openAppearance()}>Apariencia…</button>
      <button className="btn" onClick={restore}>Restaurar predeterminada</button>
      <button className="btn primary" onClick={exit}>Listo</button>
    </div>
  );
}
