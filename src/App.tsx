import { useEffect } from "react";
import { ContextMenu } from "./components/ContextMenu";
import { Dialogs } from "./components/Dialogs";
import { GlyphDefs } from "./components/Glyphs";
import { Pane } from "./components/Pane";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { Overlays } from "./components/Transfers";
import { fs } from "./lib/fs";
import { mark } from "./lib/perf";
import { resolveTheme } from "./lib/themes";
import { useApp } from "./stores/app";
import { useNav } from "./stores/nav";
import { useOps } from "./stores/ops";
import { useIndex } from "./stores/indexStatus";
import { usePrefs } from "./stores/prefs";
import { useSearch } from "./stores/search";
import { useUpdate } from "./stores/update";

const UPDATE_CHECK_DELAY_MS = 8000;

const cache = (k: string, v: string) => {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* sin almacenamiento: solo se pierde el arranque sin parpadeo */
  }
};

export default function App() {
  const ready = useApp((s) => s.ready);
  const places = useApp((s) => s.places);
  const drives = useApp((s) => s.drives);
  const vibrancy = useApp((s) => s.vibrancy);
  const boot = useApp((s) => s.boot);
  const loadUsage = useApp((s) => s.loadUsage);
  const opsVersion = useOps((s) => s.version);
  const loaded = usePrefs((s) => s.loaded);
  const theme = usePrefs((s) => s.theme);
  const sidebarCollapsed = usePrefs((s) => s.sidebarCollapsed);
  const load = usePrefs((s) => s.load);
  const update = usePrefs((s) => s.update);

  useEffect(() => {
    mark("app-mounted");
    void load();
    void boot();
  }, [load, boot]);

  // Actualizaciones: bastante después del arranque, para no competir con la primera carpeta. Nunca descarga nada por su cuenta.
  useEffect(() => {
    if (!loaded) return;
    const t = setTimeout(() => void useUpdate.getState().autoCheck(), UPDATE_CHECK_DELAY_MS);
    return () => clearTimeout(t);
  }, [loaded]);

  // Estado del índice de búsqueda: consulta periódica (más frecuente mientras se indexa). No hace nada fuera de la app de escritorio.
  useEffect(() => useIndex.getState().start(), []);

  // Capacidad de las unidades: después del arranque (nunca lo retrasa) y tras cada operación.
  useEffect(() => {
    if (ready) loadUsage();
  }, [ready, opsVersion, loadUsage]);

  // Carpeta inicial: la primera ubicación real disponible.
  useEffect(() => {
    if (!ready || !loaded) return;
    mark("booted");
    const first = places[0] ?? drives[0];
    if (first) useNav.getState().init(first.path);
  }, [ready, loaded, places, drives]);

  // Tema: "system" sigue al sistema. No se toca hasta conocer las preferencias reales:
  // hasta entonces manda el valor que dejó `public/boot.js`, así no hay parpadeo de tema.
  useEffect(() => {
    if (!loaded) return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = resolveTheme(theme, mq.matches);
    };
    apply();
    cache("nova.theme", theme);
    mq.addEventListener("change", apply);
    void fs.setWindowTheme(theme).catch(() => {});
    return () => mq.removeEventListener("change", apply);
  }, [theme, loaded]);

  useEffect(() => {
    if (!ready) return;
    document.documentElement.dataset.vibrancy = vibrancy ? "on" : "off";
    cache("nova.vibrancy", vibrancy ? "1" : "0");
  }, [vibrancy, ready]);

  // La ventana nace oculta: se muestra con la interfaz ya montada y las preferencias aplicadas.
  // Si el arranque se retrasa, se muestra igualmente a los 1,2 s (y Rust lo hace a los 6 s si todo falla).
  useEffect(() => {
    if (ready && loaded) void fs.showWindow().catch(() => {});
  }, [ready, loaded]);
  useEffect(() => {
    const t = setTimeout(() => void fs.showWindow().catch(() => {}), 1200);
    return () => clearTimeout(t);
  }, []);

  // Guardado inmediato de las preferencias pendientes si la ventana se cierra por otra vía (Alt+F4, etc.).
  useEffect(() => {
    const flush = () => void usePrefs.getState().flush();
    window.addEventListener("pagehide", flush);
    window.addEventListener("beforeunload", flush);
    return () => {
      window.removeEventListener("pagehide", flush);
      window.removeEventListener("beforeunload", flush);
    };
  }, []);

  // Atajos globales.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const nav = useNav.getState();
      const typing = e.target instanceof HTMLInputElement;
      const ctrl = e.ctrlKey || e.metaKey;
      if (e.altKey && e.key === "ArrowLeft") return e.preventDefault(), nav.goBack();
      if (e.altKey && e.key === "ArrowRight") return e.preventDefault(), nav.goForward();
      if (e.altKey && e.key === "ArrowUp") return e.preventDefault(), nav.goUp();
      if (e.key === "Backspace" && !typing && useSearch.getState().scope !== "global") return e.preventDefault(), nav.goUp();
      if (e.altKey && e.key.toLowerCase() === "p") return e.preventDefault(), update({ previewPane: !usePrefs.getState().previewPane });
      if (!ctrl) return;
      switch (e.key.toLowerCase()) {
        case "t": e.preventDefault(); nav.openTab(); break;
        case "w": e.preventDefault(); if (nav.activeId) nav.closeTab(nav.activeId); break;
        case "l": e.preventDefault(); window.dispatchEvent(new Event("nova:edit-path")); break;
        case "f":
          e.preventDefault();
          if (e.shiftKey) {
            useSearch.getState().openGlobal(); // Ctrl+Mayús+F: misma acción que el botón «Buscar en todos los archivos»
          } else document.getElementById("nova-filter")?.focus();
          break;
        case "b": e.preventDefault(); update({ sidebarCollapsed: !usePrefs.getState().sidebarCollapsed }); break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [update]);

  return (
    <div className="app" data-rail={sidebarCollapsed} data-booting={!(ready && loaded)}>
      <GlyphDefs />
      <TitleBar />
      <div className="body">
        <Sidebar />
        <Pane />
      </div>
      <ContextMenu />
      <Dialogs />
      <Overlays />
    </div>
  );
}
