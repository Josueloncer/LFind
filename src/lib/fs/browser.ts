import { DEFAULT_PREFS } from "./types";
import type { DirEvent, DirHandle, FsProvider, OpEvent, OpHandle, OpRequest, PlacesInfo, Prefs, StatsEvent } from "./types";

const NA = () => Promise.reject("Esta función solo está disponible en la aplicación de escritorio.");

const KEY = "nova.prefs";

/**
 * Proveedor para `npm run dev` en un navegador normal. No hay acceso a archivos
 * reales y NO se inventa ninguno: la interfaz muestra un estado explicativo.
 * Solo las preferencias se guardan (localStorage) para poder probar temas y vistas.
 */
export class BrowserProvider implements FsProvider {
  readonly available = false;

  openDir(_path: string, onEvent: (e: DirEvent) => void): DirHandle {
    queueMicrotask(() =>
      onEvent({ type: "error", message: "Sin acceso al sistema de archivos fuera de la aplicación de escritorio." }),
    );
    return { cancel() {} };
  }

  async getPlaces(): Promise<PlacesInfo> {
    return { places: [], drives: [] };
  }

  async loadPrefs(): Promise<Prefs> {
    try {
      return { ...DEFAULT_PREFS, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
    } catch {
      return DEFAULT_PREFS;
    }
  }

  async savePrefs(p: Prefs): Promise<void> {
    try {
      localStorage.setItem(KEY, JSON.stringify(p));
    } catch {
      /* almacenamiento no disponible */
    }
  }

  async vibrancy() {
    return false;
  }

  async setWindowTheme() {}

  async showWindow() {}

  startOp(_req: OpRequest, onEvent: (e: OpEvent) => void): OpHandle {
    queueMicrotask(() => onEvent({ type: "done", completed: 0, skipped: 0, cancelled: false, undoable: false, errors: [{ path: "", message: "Las operaciones de archivo solo están disponibles en la aplicación de escritorio.", kind: "other" }] }));
    return { cancel() {}, resolve() {} };
  }
  renamePath = NA as FsProvider["renamePath"];
  createFolder = NA as FsProvider["createFolder"];
  createFile = NA as FsProvider["createFile"];
  undoLast = NA as FsProvider["undoLast"];
  peekUndo = async () => null;
  openPath = NA as FsProvider["openPath"];
  revealPath = NA as FsProvider["revealPath"];
  createShortcut = NA as FsProvider["createShortcut"];
  getProperties = NA as FsProvider["getProperties"];
  getDriveUsage = NA as FsProvider["getDriveUsage"];
  startStats(_paths: string[], onEvent: (e: StatsEvent) => void) {
    queueMicrotask(() => onEvent({ type: "done", stats: { files: 0, dirs: 0, bytes: 0, errors: 0 }, cancelled: true }));
    return { cancel() {} };
  }

  getThumbnail = () => Promise.reject({ kind: "unsupported", message: "Sin acceso al disco fuera de la aplicación de escritorio." });
  setThumbEpoch = async () => {};
  clearThumbCache = async () => 0;
  getTextPreview = NA as FsProvider["getTextPreview"];
}
