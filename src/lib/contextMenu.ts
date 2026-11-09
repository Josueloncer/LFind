/**
 * Menú contextual propio de LFind (modelo puro, sin React ni Tauri).
 *
 * Se construye de forma SÍNCRONA a partir de lo que ya está en memoria: la selección, el estado del portapapeles, las
 * plantillas de «Nuevo» y los destinos de «Enviar a» (cargados en segundo plano al arrancar). Abrir el menú no hace
 * ninguna llamada al sistema, así que no hay nada que esperar.
 */
import type { Entry, NewTemplate, SendToItem } from "./fs/types";
import { newMenu, sortMenu, viewModeItems } from "./menus";
import type { EffectiveView } from "./viewState";
import type { MenuItem } from "../stores/ui";

/** Formatos que sabe extraer `tar.exe` de Windows (libarchive). */
const ARCHIVE_EXT = new Set(["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "iso", "cab"]);
export function isArchive(name: string): boolean {
  const i = name.lastIndexOf(".");
  return i > 0 && ARCHIVE_EXT.has(name.slice(i + 1).toLowerCase());
}

/** Punto donde se pidió el menú (ratón o teclado). */
export interface ContextPoint {
  clientX: number;
  clientY: number;
  shiftKey: boolean;
  keyboard?: boolean;
  timeStamp?: number;
}

/** Lo que el menú necesita saber (todo en memoria). */
export interface MenuState {
  /** Carpeta que se muestra. */
  path: string;
  /** Elementos sobre los que se hizo clic derecho. Vacío = espacio vacío de la carpeta. */
  list: Entry[];
  /** ¿Hay algo que pegar? (portapapeles interno o del sistema, sin consultar nada) */
  canPaste: boolean;
  view: EffectiveView;
  sendTo: readonly SendToItem[];
  templates: readonly NewTemplate[];
  /** Mayús pulsado al hacer clic: ofrece lo que normalmente no se muestra. */
  shift?: boolean;
}

/** Acciones reales (las ejecuta `useFileActions` y, por debajo, Rust). */
export interface MenuActions {
  open(e: Entry): void;
  openInNewTab(e: Entry): void;
  openWith(e: Entry): void;
  reveal(e: Entry): void;
  openTerminal(dir?: Entry): void;
  openFolderInExplorer(): void;
  cut(l: Entry[]): void;
  copy(l: Entry[]): void;
  paste(into?: Entry): void;
  copyPaths(l: Entry[]): void;
  shortcut(e: Entry): void;
  togglePin(e: Entry): void;
  isPinned(e: Entry): boolean;
  /** Fija o quita de Ubicaciones la carpeta que se está viendo. */
  togglePinFolder(): void;
  folderPinned(): boolean;
  startRename(name: string): void;
  remove(l: Entry[], permanent: boolean): void;
  properties(l: Entry[]): void;
  compress(l: Entry[]): void;
  extract(e: Entry): void;
  sendTo(l: Entry[], t: SendToItem): void;
  newFolder(): void;
  newFile(): void;
  newFromTemplate(t: NewTemplate): void;
  undo(): void;
  reload(): void;
  setView(patch: Partial<EffectiveView>): void;
  /** Abre el analizador de almacenamiento para esa carpeta (sin argumento: la que se está viendo). */
  analyze?(dir?: Entry): void;
}

const sep: MenuItem = { kind: "sep" };

export const pinLabel = (pinned: boolean) => (pinned ? "Quitar de Ubicaciones" : "Fijar en Ubicaciones");

/** Lo que se puede hacer con una fila de la barra lateral o una tarjeta de Mi equipo. Lo opcional solo aparece si aplica. */
export interface PlaceMenuActions {
  open(): void;
  openInNewTab(): void;
  pinned: boolean;
  togglePin(): void;
  rename?(): void;
  moveUp?(): void;
  moveDown?(): void;
  copyPath(): void;
  properties(): void;
  analyze?(): void;
  forget?(): void;
  clearAll?(): void;
}

export function placeMenu(a: PlaceMenuActions): MenuItem[] {
  const opt = (label: string, run: (() => void) | undefined, icon?: string): MenuItem[] => (run ? [{ kind: "item", label, icon, run }] : []);
  return tidy([
    { kind: "item", label: "Abrir", icon: "open", run: a.open },
    { kind: "item", label: "Abrir en una pestaña nueva", icon: "tab", run: a.openInNewTab },
    sep,
    { kind: "item", label: pinLabel(a.pinned), icon: "pin", run: a.togglePin },
    ...opt("Cambiar nombre", a.rename, "rename"),
    ...opt("Analizar almacenamiento", a.analyze, "props"),
    ...opt("Subir en la lista", a.moveUp),
    ...opt("Bajar en la lista", a.moveDown),
    sep,
    { kind: "item", label: "Copiar ruta de acceso", icon: "path", run: a.copyPath },
    { kind: "item", label: "Propiedades", icon: "props", run: a.properties },
    sep,
    ...opt("Quitar del historial", a.forget),
    ...opt("Limpiar historial", a.clearAll),
  ]);
}

/** Quita separadores duplicados, iniciales y finales (los grupos vacíos no dejan huecos). */
export function tidy(items: MenuItem[]): MenuItem[] {
  const out: MenuItem[] = [];
  for (const it of items) {
    if (it.kind === "sep" && (!out.length || out[out.length - 1].kind === "sep")) continue;
    out.push(it);
  }
  while (out.length && out[out.length - 1].kind === "sep") out.pop();
  return out;
}

export function buildContextMenu(s: MenuState, a: MenuActions): MenuItem[] {
  return tidy(s.list.length ? itemMenu(s, a) : backgroundMenu(s, a));
}

function itemMenu(s: MenuState, a: MenuActions): MenuItem[] {
  const list = s.list;
  const one = list.length === 1;
  const e = list[0];
  const dirOne = one && e.kind === "dir";
  const fileOne = one && e.kind !== "dir";
  const send: MenuItem[] = s.sendTo.length
    ? [{ kind: "submenu", label: "Enviar a", icon: "send", items: s.sendTo.map((t): MenuItem => ({ kind: "item", label: t.label, run: () => a.sendTo(list, t) })) }]
    : [];
  return [
    ...(one ? [{ kind: "item", label: "Abrir", icon: "open", shortcut: "Intro", run: () => a.open(e) } as MenuItem] : []),
    ...(fileOne ? [{ kind: "item", label: "Abrir con…", icon: "openwith", run: () => a.openWith(e) } as MenuItem] : []),
    ...(dirOne ? [{ kind: "item", label: "Abrir en una pestaña nueva", icon: "tab", run: () => a.openInNewTab(e) } as MenuItem] : []),
    ...(dirOne ? [{ kind: "item", label: "Abrir terminal aquí", icon: "terminal", run: () => a.openTerminal(e) } as MenuItem] : []),
    ...(fileOne ? [{ kind: "item", label: "Mostrar en el Explorador de Windows", icon: "reveal", run: () => a.reveal(e) } as MenuItem] : []),
    sep,
    { kind: "item", label: "Cortar", icon: "cut", shortcut: "Ctrl+X", run: () => a.cut(list) },
    { kind: "item", label: "Copiar", icon: "copy", shortcut: "Ctrl+C", run: () => a.copy(list) },
    ...(dirOne ? [{ kind: "item", label: "Pegar en la carpeta", icon: "paste", disabled: !s.canPaste, run: () => a.paste(e) } as MenuItem] : []),
    sep,
    { kind: "item", label: one ? "Copiar ruta de acceso" : "Copiar rutas", icon: "path", run: () => a.copyPaths(list) },
    ...(one ? [{ kind: "item", label: "Crear acceso directo", icon: "shortcut", run: () => a.shortcut(e) } as MenuItem] : []),
    ...(dirOne ? [{ kind: "item", label: pinLabel(a.isPinned(e)), icon: "pin", run: () => a.togglePin(e) } as MenuItem] : []),
    ...(dirOne && a.analyze ? [{ kind: "item", label: "Analizar almacenamiento", icon: "props", run: () => a.analyze?.(e) } as MenuItem] : []),
    sep,
    { kind: "item", label: one && isArchive(e.name) ? "Comprimir en otro .zip" : list.length > 1 ? "Comprimir en .zip" : "Comprimir en .zip", icon: "zip", run: () => a.compress(list) },
    ...(one && isArchive(e.name) ? [{ kind: "item", label: "Extraer aquí", icon: "extract", run: () => a.extract(e) } as MenuItem] : []),
    ...send,
    sep,
    ...(one ? [{ kind: "item", label: "Cambiar nombre", icon: "rename", shortcut: "F2", run: () => a.startRename(e.name) } as MenuItem] : []),
    { kind: "item", label: "Eliminar", icon: "trash", shortcut: "Supr", run: () => a.remove(list, false) },
    { kind: "item", label: "Eliminar permanentemente", danger: true, shortcut: "Mayús+Supr", run: () => a.remove(list, true) },
    sep,
    { kind: "item", label: one ? "Propiedades" : "Propiedades de la selección", icon: "props", shortcut: "Alt+Intro", run: () => a.properties(list) },
  ];
}

function backgroundMenu(s: MenuState, a: MenuActions): MenuItem[] {
  const nuevo: MenuItem[] = [
    ...newMenu({ newFolder: a.newFolder, newFile: a.newFile }),
    ...(s.templates.length ? ([sep, ...s.templates.map((t): MenuItem => ({ kind: "item", label: t.label, run: () => a.newFromTemplate(t) }))] as MenuItem[]) : []),
  ];
  return [
    { kind: "submenu", label: "Nuevo", icon: "new", items: nuevo },
    sep,
    { kind: "item", label: "Pegar", icon: "paste", shortcut: "Ctrl+V", disabled: !s.canPaste, run: () => a.paste() },
    { kind: "item", label: "Deshacer", icon: "undo", shortcut: "Ctrl+Z", run: a.undo },
    sep,
    { kind: "submenu", label: "Ordenar y agrupar", icon: "sort", items: sortMenu(s.view, a) },
    { kind: "submenu", label: "Ver", icon: "view", items: viewModeItems(s.view, a) },
    sep,
    { kind: "item", label: "Actualizar", icon: "refresh", shortcut: "F5", run: a.reload },
    { kind: "item", label: "Abrir terminal aquí", icon: "terminal", run: () => a.openTerminal() },
    { kind: "item", label: "Abrir esta carpeta en el Explorador de Windows", icon: "explorer", run: a.openFolderInExplorer },
    { kind: "item", label: "Copiar ruta de la carpeta", icon: "path", run: () => a.copyPaths([]) },
    { kind: "item", label: pinLabel(a.folderPinned()), icon: "pin", run: a.togglePinFolder },
    ...(a.analyze ? [{ kind: "item", label: "Analizar almacenamiento", icon: "props", run: () => a.analyze?.() } as MenuItem] : []),
    sep,
    { kind: "item", label: "Propiedades de la carpeta", icon: "props", run: () => a.properties([]) },
  ];
}

// ───────────────────────── Refresco tras acciones que terminan fuera de LFind ─────────────────────────

/** Comprimir/«Enviar a»/«Abrir con» acaban fuera de LFind: se recarga varias veces, cada vez más tarde (no hay vigilancia de carpeta). */
export const REFRESH_DELAYS_MS = [400, 1500, 4000] as const;
export function scheduleRefresh(
  reload: () => void,
  schedule: (fn: () => void, ms: number) => unknown = setTimeout,
  cancel: (h: unknown) => void = (h) => clearTimeout(h as number),
): () => void {
  const hs = REFRESH_DELAYS_MS.map((ms) => schedule(reload, ms));
  return () => hs.forEach(cancel);
}
