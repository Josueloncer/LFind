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

  getCloudPlaces = async () => [];
  getDrives = async () => [];
  drivesMask = async () => 0;
  checkPaths: FsProvider["checkPaths"] = async (paths) => paths.map(() => "ok");
  cloudOnly: FsProvider["cloudOnly"] = async (paths) => paths.map(() => false);

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

  // En el navegador basta la consola, donde React ya informa del error.
  async logUiError() {}

  startOp(_req: OpRequest, onEvent: (e: OpEvent) => void): OpHandle {
    queueMicrotask(() => onEvent({ type: "done", completed: 0, skipped: 0, cancelled: false, undoable: false, errors: [{ path: "", message: "Las operaciones de archivo solo están disponibles en la aplicación de escritorio.", kind: "other" }] }));
    return { cancel() {}, resolve() {}, pause() {} };
  }
  renamePath = NA as FsProvider["renamePath"];
  createFolder = NA as FsProvider["createFolder"];
  createFile = NA as FsProvider["createFile"];
  undoLast = NA as FsProvider["undoLast"];
  peekUndo = async () => null;
  openPath = NA as FsProvider["openPath"];
  revealPath = NA as FsProvider["revealPath"];
  createShortcut = NA as FsProvider["createShortcut"];
  resolveShortcut = NA as FsProvider["resolveShortcut"];
  getProperties = NA as FsProvider["getProperties"];
  getDriveUsage = NA as FsProvider["getDriveUsage"];
  resolvePath = NA as FsProvider["resolvePath"];
  startStats(_paths: string[], onEvent: (e: StatsEvent) => void) {
    queueMicrotask(() => onEvent({ type: "done", stats: { files: 0, dirs: 0, bytes: 0, errors: 0 }, cancelled: true }));
    return { cancel() {} };
  }

  getThumbnail = () => Promise.reject({ kind: "unsupported", message: "Sin acceso al disco fuera de la aplicación de escritorio." });
  setThumbEpoch = async () => {};
  clearThumbCache = async () => 0;
  getTextPreview = NA as FsProvider["getTextPreview"];
  viewerOpen = NA as FsProvider["viewerOpen"];
  viewerClose = async () => {};
  viewerUrl = () => "";

  appVersion = async () => "";
  updateCheck = NA as FsProvider["updateCheck"];
  updateDownload = NA as FsProvider["updateDownload"];
  updateCancel = async () => {};
  updateRestart = NA as FsProvider["updateRestart"];
  getIcons: FsProvider["getIcons"] = async (items) => ({ keys: items.map(() => null), icons: {} });
  clearIconCache = async () => 0;
  // Fuera de la app de escritorio no hay Shell de Windows: la interfaz usa su menú propio.
  clipboardSetFiles = async () => 0;
  clipboardReadFiles = async () => null;
  clipboardSequence = async () => 0;
  clipboardHasFiles = async () => false;
  openWith = NA;
  openTerminal = NA;
  compressPaths = NA;
  extractArchive = NA;
  getSendTo = async () => [];
  sendTo = NA;
  getNewTemplates = async () => [];
  createFromTemplate = NA;

  // El analizador recorre el disco desde Rust: en el navegador la vista explica que hace falta la aplicación.
  analyzerStart = NA as FsProvider["analyzerStart"];
  analyzerUpdate = NA as FsProvider["analyzerUpdate"];
  analyzerWatch = NA as FsProvider["analyzerWatch"];
  analyzerUnwatch = async () => {};
  backgroundImport = NA as FsProvider["backgroundImport"];
  listFonts = async (): Promise<string[]> => [];
  dupStart = NA as FsProvider["dupStart"];
  dupPause = NA as FsProvider["dupPause"];
  dupResume = NA as FsProvider["dupResume"];
  dupPending = async () => null;
  dupDiscard = async () => {};
  dupSuspend = NA as FsProvider["dupSuspend"];
  dupCancel = NA as FsProvider["dupCancel"];
  dupDefaultExclusions = async (): Promise<string[]> => [];
  dupSummary = async () => null;
  dupGroups = async () => [];
  dupCheckRemoval = NA as FsProvider["dupCheckRemoval"];
  dupPrune = NA as FsProvider["dupPrune"];
  dupForget = NA as FsProvider["dupForget"];
  dupCompare = NA as FsProvider["dupCompare"];
  openWithList = async (): Promise<string[]> => [];
  openWithApp = NA as FsProvider["openWithApp"];
  startNativeDrag = NA as FsProvider["startNativeDrag"];
  recycleList = NA as FsProvider["recycleList"];
  recycleRestore = NA as FsProvider["recycleRestore"];
  recyclePurge = NA as FsProvider["recyclePurge"];
  recycleCancel = NA as FsProvider["recycleCancel"];
  winRepair = NA as FsProvider["winRepair"];
  termOpen = NA as FsProvider["termOpen"];
  termWrite = NA as FsProvider["termWrite"];
  termResize = NA as FsProvider["termResize"];
  termAck = NA as FsProvider["termAck"];
  termBusy = NA as FsProvider["termBusy"];
  termClose = NA as FsProvider["termClose"];
  takeStartupPath = async () => null;
  onOpenPath = async () => () => {};
  winStatus = NA as FsProvider["winStatus"];
  winSet = NA as FsProvider["winSet"];
  winRestore = NA as FsProvider["winRestore"];
  inspectPath = NA as FsProvider["inspectPath"];
  startHash = () => ({ cancel() {} });
  backgroundLoad = async () => null;
  backgroundRetain = async () => 0;
  analyzerPause = async () => {};
  analyzerResume = async () => {};
  analyzerCancel = async () => {};
  analyzerReports = async () => [];
  analyzerReport = async () => null;
  analyzerForget = NA as FsProvider["analyzerForget"];
  analyzerNode = NA as FsProvider["analyzerNode"];
  analyzerDirs = NA as FsProvider["analyzerDirs"];
  analyzerFiles = NA as FsProvider["analyzerFiles"];
  analyzerCategories = NA as FsProvider["analyzerCategories"];
  analyzerAvailability = NA as FsProvider["analyzerAvailability"];
  analyzerExts = NA as FsProvider["analyzerExts"];
  analyzerTreemap = NA as FsProvider["analyzerTreemap"];
  analyzerIssues = NA as FsProvider["analyzerIssues"];

  // Sin el motor de Rust no hay índice: nada se simula, la interfaz explica que hace falta la aplicación de escritorio.
  indexSearch = NA as FsProvider["indexSearch"];
  indexStatus = NA as FsProvider["indexStatus"];
  indexGetSettings = NA as FsProvider["indexGetSettings"];
  indexSetSettings = NA as FsProvider["indexSetSettings"];
  indexAvailableDrives = async () => [];
  indexStart = NA as FsProvider["indexStart"];
  indexPause = NA as FsProvider["indexPause"];
  indexResume = NA as FsProvider["indexResume"];
  indexRebuild = NA as FsProvider["indexRebuild"];
}
