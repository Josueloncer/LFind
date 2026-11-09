import { Suspense, lazy, useEffect, useRef } from "react";
import { ContextMenu } from "./components/ContextMenu";
import { AppearancePanel } from "./components/AppearancePanel";
import { CommandPalette } from "./components/CommandPalette";
import { CustomizeBar } from "./components/CustomizeBar";
import { registerBuiltinCommands } from "./lib/builtinCommands";
import { usePalette } from "./stores/palette";
import { useTerminal } from "./stores/terminal";
import { Dialogs } from "./components/Dialogs";
import { GlyphDefs } from "./components/Glyphs";
import { Panes } from "./components/Panes";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { Overlays } from "./components/Transfers";
import { fs } from "./lib/fs";
import { canonicalKey } from "./lib/path";
import { mark } from "./lib/perf";
import { resolveTheme } from "./lib/themes";
import { useApp } from "./stores/app";
import { useCustomize } from "./stores/customize";
import { useBackgrounds } from "./stores/backgrounds";
import { BG_SLOTS, MANAGED_VARS, appearanceVars, diffVars } from "./lib/appearance";
import { toggleDualPane } from "./stores/dual";
import { useNav } from "./stores/nav";
import { useOps } from "./stores/ops";
import { usePathHealth } from "./stores/pathHealth";
import { useIndex } from "./stores/indexStatus";
import { usePrefs } from "./stores/prefs";
import { useSearch } from "./stores/search";
import { useUpdate } from "./stores/update";

const UPDATE_CHECK_DELAY_MS = 8000;
const PATH_CHECK_DELAY_MS = 2500;
const FOCUS_RECHECK_MS = 60_000;

const cache = (k: string, v: string) => {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* sin almacenamiento: solo se pierde el arranque sin parpadeo */
  }
};

const TerminalPanel = lazy(() => import("./components/TerminalPanel"));

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
  const customizing = useCustomize((s) => s.active);
  const load = usePrefs((s) => s.load);
  const update = usePrefs((s) => s.update);

  useEffect(() => {
    mark("app-mounted");
    void load();
    void boot();
    // Si la interfaz se recargó, un análisis anterior ya no tiene quien lo escuche.
    void fs.analyzerCancel().catch(() => {});
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

  // Unidades que aparecen o desaparecen, y carpetas de OneDrive (lectura rápida del registro, después del arranque).
  useEffect(() => {
    if (!ready) return;
    useApp.getState().loadCloud();
    return useApp.getState().watchDrives();
  }, [ready]);

  // Rutas guardadas: se comprueban en reposo tras arrancar, cuando cambian los accesos fijados y tras cada operación.
  const pinKeys = usePrefs((s) => s.favorites.map((f) => canonicalKey(f.path)).join("|"));
  const firstCheck = useRef(true);
  useEffect(() => {
    if (!loaded) return;
    const t = setTimeout(() => void usePathHealth.getState().check(), firstCheck.current ? PATH_CHECK_DELAY_MS : 300);
    firstCheck.current = false;
    return () => clearTimeout(t);
  }, [loaded, pinKeys, opsVersion]);

  // Al volver a la ventana puede haber cambiado algo fuera de LFind; como mucho una vez por minuto.
  useEffect(() => {
    let last = Date.now();
    const onFocus = () => {
      if (Date.now() - last < FOCUS_RECHECK_MS) return;
      last = Date.now();
      void usePathHealth.getState().check();
      useApp.getState().loadCloud();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  // Carpeta inicial: la primera ubicación real disponible.
  useEffect(() => {
    if (!ready || !loaded) return;
    mark("booted");
    const first = places[0] ?? drives[0];
    if (first) {
      useNav.getState().init(first.path);
      // La vista doble se recuerda; el segundo panel empieza en la misma carpeta que el primero.
      if (usePrefs.getState().dualPane && !useNav.getState().other) useNav.getState().toggleDual();
    }
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

  // Personalización: variables sobre las del tema; lo que está en su valor original no se escribe y manda el tema.
  const appearance = usePrefs((s) => s.appearance);
  const bgLoaded = useBackgrounds((s) => s.urls);
  const bgUrls = useBackgrounds((s) => s.baked);
  const bgLive = useBackgrounds((s) => s.liveFilter);
  const themeBase = useRef<{ theme: string; values: Record<string, number> } | null>(null);
  const writtenVars = useRef<Record<string, string>>({});
  useEffect(() => {
    if (!loaded) return;
    const root = document.documentElement;
    const current = root.dataset.theme ?? "";
    // Las medidas del tema se toman una vez por tema; al mover un control solo se escribe lo que cambia.
    if (themeBase.current?.theme !== current) {
      for (const name of MANAGED_VARS) root.style.removeProperty(name);
      writtenVars.current = {};
      const cs = getComputedStyle(root);
      themeBase.current = { theme: current, values: Object.fromEntries(MANAGED_VARS.map((n) => [n, parseFloat(cs.getPropertyValue(n)) || 0])) };
    }
    const base = themeBase.current.values;
    const vars = appearanceVars(appearance, (n) => base[n] ?? 0);
    const { set: changed, remove } = diffVars(writtenVars.current, vars);
    for (const name of remove) root.style.removeProperty(name);
    for (const [k, v] of changed) root.style.setProperty(k, v);
    writtenVars.current = vars;
    useBackgrounds.getState().ensure(BG_SLOTS.flatMap((s) => appearance.backgrounds[s] ?? []));
  }, [appearance, theme, loaded]);
  useEffect(() => {
    useBackgrounds.getState().bake(appearance.blur, appearance.contrast);
  }, [bgLoaded, appearance.blur, appearance.contrast]);
  useEffect(() => {
    let timer = 0;
    const onResize = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        const a = usePrefs.getState().appearance;
        useBackgrounds.getState().bake(a.blur, a.contrast);
      }, 400);
    };
    window.addEventListener("resize", onResize);
    return () => (window.clearTimeout(timer), window.removeEventListener("resize", onResize));
  }, []);
  useEffect(() => {
    if (bgLive) document.documentElement.dataset.bgFilter = "1";
    else delete document.documentElement.dataset.bgFilter;
  }, [bgLive]);
  useEffect(() => {
    const root = document.documentElement;
    for (const s of BG_SLOTS) {
      const url = appearance.backgrounds[s] ? bgUrls[appearance.backgrounds[s]!] : undefined;
      if (url) {
        root.style.setProperty(`--cz-img-${s}`, `url("${url}")`);
        root.dataset[`bg${s[0].toUpperCase()}${s.slice(1)}`] = "1";
      } else {
        root.style.removeProperty(`--cz-img-${s}`);
        delete root.dataset[`bg${s[0].toUpperCase()}${s.slice(1)}`];
      }
    }
    if (BG_SLOTS.some((s) => appearance.backgrounds[s] && bgUrls[appearance.backgrounds[s]!])) root.dataset.bg = "1";
    else delete root.dataset.bg;
  }, [appearance.backgrounds, bgUrls]);

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

  useEffect(registerBuiltinCommands, []);
  const terminalOpen = useTerminal((s) => s.open);
  const sidebarSide = usePrefs((s) => s.sidebarSide);

  // Carpeta pedida desde Windows: al arrancar y si ya había una ventana abierta.
  useEffect(() => {
    if (!ready || !loaded || !fs.available) return;
    void fs.takeStartupPath().then((p) => p && useNav.getState().navigate(p)).catch(() => {});
    const off = fs.onOpenPath((p) => useNav.getState().openTab(p));
    return () => void off.then((f) => f()).catch(() => {});
  }, [ready, loaded]);

  // Atajos globales.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const nav = useNav.getState();
      // Dentro de la terminal, las teclas son del shell.
      if (e.target instanceof Element && e.target.closest(".term")) return;
      const typing = e.target instanceof HTMLInputElement;
      const ctrl = e.ctrlKey || e.metaKey;
      if (ctrl && !e.shiftKey && e.key.toLowerCase() === "k") return e.preventDefault(), usePalette.getState().toggle();
      if (e.altKey && e.key === "ArrowLeft") return e.preventDefault(), nav.goBack();
      if (e.altKey && e.key === "ArrowRight") return e.preventDefault(), nav.goForward();
      if (e.altKey && e.key === "ArrowUp") return e.preventDefault(), nav.goUp();
      if (e.key === "Backspace" && !typing && (useSearch.getState().scope !== "global" || useSearch.getState().side !== nav.side)) return e.preventDefault(), nav.goUp();
      if (e.key === "F6" && nav.other) return e.preventDefault(), nav.focusSide(nav.side === 0 ? 1 : 0);
      if (e.altKey && e.key.toLowerCase() === "p") return e.preventDefault(), update({ previewPane: !usePrefs.getState().previewPane });
      if (!ctrl) return;
      switch (e.key.toLowerCase()) {
        case "d": if (e.shiftKey) (e.preventDefault(), toggleDualPane()); break;
        case "x": if (e.shiftKey && nav.other) (e.preventDefault(), nav.swap()); break;
        case "t": e.preventDefault(); nav.openTab(); break;
        case "w": e.preventDefault(); if (nav.activeId) nav.closeTab(nav.activeId); break;
        case "l": e.preventDefault(); window.dispatchEvent(new Event("nova:edit-path")); break;
        case "f":
          e.preventDefault();
          if (e.shiftKey) {
            useSearch.getState().openGlobal(); // Ctrl+Mayús+F: misma acción que el botón «Buscar en todos los archivos»
          } else document.getElementById("nova-filter")?.focus();
          break;
        case "j": e.preventDefault(); useTerminal.getState().toggle(); break;
        case "i": e.preventDefault(); update({ previewPane: true, previewTab: "details" }); break;
        case "b": e.preventDefault(); update({ sidebarCollapsed: !usePrefs.getState().sidebarCollapsed }); break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [update]);

  return (
    <div className="app" data-sidebar={sidebarSide} data-rail={sidebarCollapsed && !customizing} data-booting={!(ready && loaded)}>
      <GlyphDefs />
      <TitleBar />
      <div className="body">
        <Sidebar />
        <div className="workspace">
          <Panes />
          {terminalOpen && <Suspense fallback={null}><TerminalPanel /></Suspense>}
        </div>
      </div>
      <ContextMenu />
      <Dialogs />
      <Overlays />
      <CustomizeBar />
      <CommandPalette />
      <AppearancePanel />
    </div>
  );
}
