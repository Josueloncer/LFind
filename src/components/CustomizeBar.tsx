import { useEffect } from "react";
import { useCustomize } from "../stores/customize";
import { usePrefs } from "../stores/prefs";
import { useUi } from "../stores/ui";

/** Aviso flotante del modo de personalización: al ser flotante, abrirlo no desplaza nada de la interfaz. */
export function CustomizeBar() {
  const active = useCustomize((s) => s.active);
  const exit = useCustomize((s) => s.exit);

  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !useUi.getState().menu && !useUi.getState().dialog) exit();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, exit]);

  if (!active) return null;
  const restore = () => {
    usePrefs.getState().resetLayout();
    useUi.getState().toast("Distribución restaurada. Tus ubicaciones fijadas no han cambiado.", "success");
  };
  return (
    <div className="cz-bar" role="toolbar" aria-label="Personalizar interfaz">
      <span className="cz-text"><b>Personalizar interfaz</b><span className="cz-hint">Arrastra secciones, filas y herramientas. El ojo oculta o muestra cada herramienta.</span></span>
      <button className="btn" onClick={restore}>Restaurar predeterminada</button>
      <button className="btn primary" onClick={exit}>Listo</button>
    </div>
  );
}
