import { useApp } from "../stores/app";
import { useCustomize } from "../stores/customize";
import { toggleDualPane } from "../stores/dual";
import { useNav } from "../stores/nav";
import { registerCommands } from "../stores/palette";
import { paneActions } from "../stores/paneActions";
import { usePrefs } from "../stores/prefs";
import { useSearch } from "../stores/search";
import { useStorage } from "../stores/storage";
import { useTerminal } from "../stores/terminal";
import { useUi } from "../stores/ui";
import { looksLikePath } from "./commandSearch";
import type { Command } from "./commandSearch";
import { GROUP_LABEL } from "./group";
import { SORTS } from "./menus";
import { COMPUTER, DUPLICATES, RECYCLE, basename } from "./path";
import { THEMES } from "./themes";
import { ICON_PRESETS, VIEW_LABEL } from "./view";
import type { GroupBy, ViewMode } from "./fs";
import type { EffectiveView } from "./viewState";

const NAV = "Ir a";
const VIEW = "Vista";
const SORT = "Ordenar";
const PANES = "Pestañas y paneles";
const TOOLS = "Herramientas";
const LOOK = "Apariencia";

const go = (path: string) => useNav.getState().navigate(path);
const setView = (patch: Partial<EffectiveView>): void => paneActions()?.setView(patch);

function places(): Command[] {
  const { places: p, drives, cloud } = useApp.getState();
  const prefs = usePrefs.getState();
  const out: Command[] = [{ id: "go:computer", title: "Mi equipo", section: NAV, icon: "props", run: () => go(COMPUTER) }];
  const add = (path: string, title: string, subtitle: string) => {
    const id = `go:${path.toLowerCase()}`;
    if (!out.some((c) => c.id === id)) out.push({ id, title, section: NAV, subtitle, icon: "folder", run: () => go(path) });
  };
  for (const f of prefs.favorites) add(f.path, f.label || basename(f.path), f.path);
  for (const x of [...p, ...cloud, ...drives]) add(x.path, x.label, x.path);
  for (const r of prefs.recentFolders.slice(0, 15)) add(r.path, basename(r.path) || r.path, r.path);
  return out;
}

function views(): Command[] {
  const modes: ViewMode[] = ["details", "list", "tiles", "content", "icons"];
  const out: Command[] = modes.filter((m) => m !== "icons").map((m) => ({ id: `view:${m}`, title: `Vista: ${VIEW_LABEL[m]}`, section: VIEW, run: () => setView({ viewMode: m }) }));
  for (const p of ICON_PRESETS) out.push({ id: `view:icons-${p.size}`, title: `Vista: ${p.label}`, section: VIEW, keywords: "iconos tamaño", run: () => setView({ viewMode: "icons", iconSize: p.size }) });
  const groups = Object.keys(GROUP_LABEL) as GroupBy[];
  const sorts: Command[] = SORTS.map(([key, label]) => ({ id: `sort:${key}`, title: `Ordenar por ${label.toLowerCase()}`, section: SORT, run: () => setView({ sortKey: key }) }));
  sorts.push(
    { id: "sort:asc", title: "Orden ascendente", section: SORT, run: () => setView({ sortDir: "asc" }) },
    { id: "sort:desc", title: "Orden descendente", section: SORT, run: () => setView({ sortDir: "desc" }) },
    ...groups.map((g): Command => ({ id: `group:${g}`, title: g === "none" ? "Sin agrupar" : `Agrupar por ${GROUP_LABEL[g].toLowerCase()}`, section: SORT, keywords: "agrupación grupos", run: () => setView({ groupBy: g }) })),
  );
  return [...out, ...sorts];
}

function panes(): Command[] {
  const n = useNav.getState();
  const step = (d: number) => {
    const s = useNav.getState();
    const at = s.tabs.findIndex((t) => t.id === s.activeId);
    if (s.tabs.length > 1 && at >= 0) s.setActive(s.tabs[(at + d + s.tabs.length) % s.tabs.length].id);
  };
  const dual = !!n.other;
  const out: Command[] = [
    { id: "tab:new", title: "Nueva pestaña", section: PANES, shortcut: "Ctrl+T", run: () => useNav.getState().openTab() },
    { id: "tab:close", title: "Cerrar pestaña", section: PANES, shortcut: "Ctrl+W", run: () => { const s = useNav.getState(); if (s.activeId) s.closeTab(s.activeId); } },
    { id: "tab:next", title: "Pestaña siguiente", section: PANES, run: () => step(1) },
    { id: "tab:prev", title: "Pestaña anterior", section: PANES, run: () => step(-1) },
    { id: "nav:back", title: "Atrás", section: PANES, shortcut: "Alt+←", run: () => useNav.getState().goBack() },
    { id: "nav:forward", title: "Adelante", section: PANES, shortcut: "Alt+→", run: () => useNav.getState().goForward() },
    { id: "nav:up", title: "Subir de carpeta", section: PANES, shortcut: "Alt+↑", run: () => useNav.getState().goUp() },
    { id: "nav:reload", title: "Actualizar carpeta", section: PANES, shortcut: "F5", run: () => paneActions()?.reload() },
    { id: "dual:toggle", title: dual ? "Volver a un panel" : "Abrir doble panel", section: PANES, shortcut: "Ctrl+Mayús+D", keywords: "dividir", run: toggleDualPane },
  ];
  if (dual) {
    out.push(
      { id: "dual:focus", title: "Cambiar de panel", section: PANES, shortcut: "F6", run: () => { const s = useNav.getState(); s.focusSide(s.side === 0 ? 1 : 0); } },
      { id: "dual:swap", title: "Intercambiar paneles", section: PANES, shortcut: "Ctrl+Mayús+X", run: () => useNav.getState().swap() },
    );
  }
  return out;
}

function tools(): Command[] {
  const prefs = () => usePrefs.getState();
  const ui = () => useUi.getState();
  const path = () => paneActions()?.path;
  return [
    { id: "tool:search", title: "Buscar en todos los archivos", section: TOOLS, shortcut: "Ctrl+Mayús+F", keywords: "global índice", run: () => useSearch.getState().openGlobal() },
    { id: "tool:filter", title: "Filtrar la carpeta", section: TOOLS, shortcut: "Ctrl+F", run: () => document.getElementById("nova-filter")?.focus() },
    { id: "tool:path", title: "Escribir una ruta", section: TOOLS, shortcut: "Ctrl+L", run: () => window.dispatchEvent(new Event("nova:edit-path")) },
    { id: "tool:analyze", title: "Analizar almacenamiento", section: TOOLS, keywords: "espacio disco tamaño", run: () => { const p = path(); if (p) useStorage.getState().analyze(p); else go("lfind://storage"); } },
    { id: "tool:terminal", title: "Terminal", section: TOOLS, shortcut: "Ctrl+J", keywords: "powershell cmd consola", run: () => useTerminal.getState().toggle() },
    { id: "tool:settings", title: "Configuración: Integración con Windows", section: TOOLS, keywords: "explorador preferido registro menú contextual inicio", run: () => ui().openDialog({ type: "settings" }) },
    { id: "tool:duplicates", title: "Detector de duplicados", section: TOOLS, keywords: "repetidos copias limpiar espacio", run: () => go(DUPLICATES) },
    { id: "tool:index", title: "Índice de búsqueda", section: TOOLS, keywords: "configurar reindexar", run: () => ui().openDialog({ type: "index" }) },
    { id: "tool:update", title: "Buscar actualizaciones", section: TOOLS, run: () => ui().openDialog({ type: "update" }) },
    { id: "pref:preview", title: prefs().previewPane ? "Ocultar panel de vista previa" : "Mostrar panel de vista previa", section: TOOLS, shortcut: "Alt+P", run: () => prefs().update({ previewPane: !prefs().previewPane }) },
    { id: "tool:inspector", title: "Abrir inspector de archivos", section: TOOLS, shortcut: "Ctrl+I", keywords: "detalles propiedades metadatos exif hash", run: () => prefs().update({ previewPane: true, previewTab: "details" }) },
    { id: "pref:sidebar", title: "Mostrar u ocultar la barra lateral", section: TOOLS, shortcut: "Ctrl+B", run: () => prefs().update({ sidebarCollapsed: !prefs().sidebarCollapsed }) },
    { id: "pref:sidebar-side", title: prefs().sidebarSide === "left" ? "Mover la barra lateral a la derecha" : "Mover la barra lateral a la izquierda", section: TOOLS, run: () => prefs().update({ sidebarSide: prefs().sidebarSide === "left" ? "right" : "left" }) },
    { id: "tool:recycle", title: "Papelera de reciclaje", section: TOOLS, keywords: "eliminados restaurar vaciar", run: () => go(RECYCLE) },
    { id: "pref:hidden", title: prefs().showHidden ? "Ocultar archivos ocultos" : "Mostrar archivos ocultos", section: TOOLS, run: () => prefs().update({ showHidden: !prefs().showHidden }) },
    { id: "pref:thumbs", title: prefs().thumbnails ? "Desactivar miniaturas" : "Activar miniaturas", section: TOOLS, run: () => prefs().update({ thumbnails: !prefs().thumbnails }) },
    { id: "look:panel", title: "Personalización", section: LOOK, keywords: "fondo colores fuente tipografía", run: () => useCustomize.getState().openAppearance() },
    { id: "look:layout", title: "Personalizar interfaz", section: LOOK, keywords: "barras herramientas reorganizar", run: () => useCustomize.getState().enter() },
    ...THEMES.map((t): Command => ({ id: `look:theme-${t.id}`, title: `Tema: ${t.label}`, section: LOOK, run: () => prefs().update({ theme: t.id }) })),
  ];
}

let done = false;

/** Registra los comandos propios de LFind. Se llama una vez al arrancar y no calcula nada hasta abrir la paleta. */
export function registerBuiltinCommands(): void {
  if (done) return;
  done = true;
  registerCommands({
    all: () => [...places(), ...panes(), ...views(), ...tools()],
    forQuery(q) {
      const path = q.trim();
      return looksLikePath(path) ? [{ id: "go:typed", title: `Ir a ${path}`, section: NAV, icon: "folder", run: () => go(path) }] : [];
    },
  });
}
