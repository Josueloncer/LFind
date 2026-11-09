// Espejo de los tipos serializados por nova-core (Rust). Mantener sincronizados.

export type EntryKind = "file" | "dir" | "other";

export interface Entry {
  name: string;
  kind: EntryKind;
  size: number;
  modified: number | null;
  created: number | null;
  hidden: boolean;
  ext: string | null;
  link: boolean;
}

export type DirEvent =
  | { type: "batch"; entries: Entry[] }
  | { type: "done"; total: number; skipped: number; cancelled: boolean }
  | { type: "error"; message: string };

export interface DirHandle {
  cancel(): void;
}

export interface Place {
  id: string;
  label: string;
  path: string;
  kind: "home" | "desktop" | "documents" | "downloads" | "pictures" | "music" | "videos" | "drive";
}

export interface PlacesInfo {
  places: Place[];
  drives: Place[];
}

export type Theme = "system" | "dark" | "light" | "cyberpunk" | "hacker" | "minimal";
export type ViewMode = "details" | "list" | "tiles" | "content" | "icons";
export type SortKey = "name" | "modified" | "size" | "kind" | "created";
export type SortDir = "asc" | "desc";
export type GroupBy = "none" | "type" | "modified" | "created" | "size" | "name";
/** Columnas opcionales de la vista de detalles (el nombre siempre se muestra). */
export type Column = "modified" | "created" | "type" | "size";

/** Vista recordada para una carpeta concreta. */
export interface FolderView {
  path: string;
  viewMode: ViewMode;
  iconSize: number;
  sortKey: SortKey;
  sortDir: SortDir;
  groupBy: GroupBy;
  /** `true` = el usuario pidió recordar el modo y el tamaño de icono para esta carpeta. Si no, se hereda la vista global. */
  pinned?: boolean;
}

export interface Favorite {
  path: string;
  label: string;
}

export interface Prefs {
  theme: Theme;
  viewMode: ViewMode;
  showHidden: boolean;
  sortKey: SortKey;
  sortDir: SortDir;
  sidebarCollapsed: boolean;
  favorites: Favorite[];
  iconSize: number;
  groupBy: GroupBy;
  columns: Column[];
  recents: string[];
  folderViews: FolderView[];
  thumbnails: boolean;
  /** Iconos del sistema (Explorador de Windows) en lugar de los glifos propios. */
  systemIcons: boolean;
  previewPane: boolean;
  previewWidth: number;
  autoUpdateCheck: boolean;
  lastUpdateCheck: number;
  version: number;
}

export const DEFAULT_PREFS: Prefs = {
  theme: "system",
  viewMode: "details",
  showHidden: false,
  sortKey: "name",
  sortDir: "asc",
  sidebarCollapsed: false,
  favorites: [],
  iconSize: 64,
  groupBy: "none",
  columns: ["modified", "type", "size"],
  recents: [],
  folderViews: [],
  thumbnails: true,
  systemIcons: true,
  previewPane: false,
  previewWidth: 320,
  autoUpdateCheck: true,
  lastUpdateCheck: 0,
  version: 2,
};

// ───────── operaciones de archivo (espejo de src-tauri/commands.rs y nova-core::ops) ─────────

export type OpKind = "copy" | "move" | "trash" | "delete";
export interface OpRequest {
  kind: OpKind;
  sources: string[];
  dest?: string;
}
export interface Progress {
  bytesDone: number;
  bytesTotal: number;
  filesDone: number;
  filesTotal: number;
  current: string;
}
export interface ItemInfo {
  path: string;
  size: number;
  modified: number | null;
  isDir: boolean;
}
export interface ConflictInfo {
  src: ItemInfo;
  dst: ItemInfo;
}
export type Decision = "replace" | "skip" | "keepBoth" | "cancel";
export type ErrKind = "denied" | "inuse" | "notfound" | "exists" | "invalid" | "diskfull" | "blocked" | "toolong" | "other";
export interface OpError {
  path: string;
  message: string;
  kind: ErrKind;
}
export interface DoneInfo {
  completed: number;
  skipped: number;
  errors: OpError[];
  cancelled: boolean;
  undoable: boolean;
}
export type OpEvent = ({ type: "progress" } & Progress) | ({ type: "conflict" } & ConflictInfo) | ({ type: "done" } & DoneInfo);
export interface OpHandle {
  cancel(): void;
  resolve(decision: Decision, applyAll: boolean): void;
}
export interface FileProps {
  name: string;
  path: string;
  parent: string | null;
  isDir: boolean;
  size: number;
  created: number | null;
  modified: number | null;
  accessed: number | null;
  readonly: boolean;
  hidden: boolean;
  system: boolean;
  link: boolean;
  linkTarget: string | null;
  ext: string | null;
}
export interface Stats {
  files: number;
  dirs: number;
  bytes: number;
  errors: number;
}
export type StatsEvent = ({ type: "progress" } & Stats) | { type: "done"; stats: Stats; cancelled: boolean };
export type ResolvedPath = { type: "dir"; path: string } | { type: "file"; dir: string; file: string };
export interface DriveUsage {
  root: string;
  total: number;
  free: number;
  used: number;
  kind: "fixed" | "removable" | "network" | "cdrom" | "ramdisk" | "unknown";
  label: string | null;
  fs: string | null;
}
export type ThumbFailKind = "unsupported" | "toolarge" | "cancelled" | "failed";
export interface ThumbReply {
  dataUrl: string;
}
/** Elemento de un lote de iconos del sistema. */
export interface IconItem {
  path: string;
  dir: boolean;
}
export interface IconsReply {
  /** Una clave por elemento pedido, en el mismo orden. `null` = sin icono del sistema (se usa el glifo propio). */
  keys: (string | null)[];
  /** `data:` URL de los iconos que la interfaz aún no tenía, por clave. */
  icons: Record<string, string>;
}
export interface TextPreview {
  text: string;
  truncated: boolean;
  totalBytes: number;
}
/** Dato mostrado en la ficha del visor. */
export interface ViewerFact {
  label: string;
  value: string;
}
/** Cómo se debe mostrar un archivo (espejo de nova-core::viewer::Source). */
export type ViewerSource =
  | { kind: "image"; token: string; mime: string; width: number | null; height: number | null; reduced: boolean; note: string | null; facts: ViewerFact[] }
  | { kind: "pdf"; token: string; size: number; illustrator: boolean }
  | { kind: "video" | "audio"; token: string; mime: string }
  | { kind: "text" }
  | { kind: "unsupported"; message: string; facts: ViewerFact[] }
  | { kind: "cancelled" };
/** Versión publicada superior a la instalada. */
export interface UpdateInfo {
  version: string;
  notes: string | null;
  size: number | null;
}
export type UpdateEvent = { type: "progress"; done: number; total: number | null } | { type: "verifying" };
export interface UndoResult {
  description: string;
  restored: number;
  errors: OpError[];
}

// ───────── portapapeles de archivos del sistema (espejo de src-tauri/clipboard_cmd.rs) ─────────

/** Destino de «Enviar a» (carpeta SendTo del usuario). `zip` = «Carpeta comprimida»: la resuelve LFind. */
export interface SendToItem {
  id: string;
  label: string;
  kind: "app" | "folder" | "zip";
}
/** Plantilla del submenú «Nuevo» (registrada en Windows con `ShellNew`). `id` es la extensión con punto. */
/** Qué debe hacer la interfaz tras «Enviar a». */
export type SendResult = { kind: "launched" } | { kind: "copyTo"; dest: string } | { kind: "zip" };

export interface NewTemplate {
  id: string;
  label: string;
}

export interface SystemClipboardFiles {
  paths: string[];
  cut: boolean;
  sequence: number;
}

/**
 * Contrato entre la interfaz y el sistema. Las pantallas nunca llaman a Tauri
 * directamente: todo pasa por aquí, de modo que las operaciones futuras
 * (copiar, mover, buscar, miniaturas) se añaden como métodos nuevos.
 */
// ───────── búsqueda global e índice (espejo de nova-core::index::model; Update 05) ─────────

export type SearchKind = "any" | "file" | "dir";
export type SearchSort = "relevance" | "name" | "size" | "modified";
/** Rango de fechas en milisegundos epoch Unix (ambos extremos incluidos). */
export interface DateRange {
  after: number | null;
  before: number | null;
}
export interface SearchQuery {
  /** Varias palabras = todas deben aparecer en el nombre, en cualquier orden. */
  text: string;
  /** El nombre completo (con o sin extensión) debe ser igual al texto. */
  exact: boolean;
  exts: string[];
  category: string | null;
  kind: SearchKind;
  minSize: number | null;
  maxSize: number | null;
  modified: DateRange;
  created: DateRange;
  accessed: DateRange;
  /** Raíces indexadas (p. ej. "C:\\"). Vacío = todas las conectadas. */
  drives: string[];
  folder: string | null;
  sort: SearchSort;
  desc: boolean;
  limit: number;
  offset: number;
}
export interface SearchHit {
  path: string;
  name: string;
  ext: string;
  isDir: boolean;
  size: number;
  modified: number | null;
  created: number | null;
}
export interface SearchPage {
  hits: SearchHit[];
  /** Hay más resultados después de esta página. */
  more: boolean;
  cancelled: boolean;
  elapsedMs: number;
}
export type RootState = "pending" | "scanning" | "complete" | "interrupted";
export interface IndexRoot {
  id: number;
  path: string;
  state: RootState;
  online: boolean;
  gen: number;
  files: number;
  dirs: number;
  skipped: number;
  lastComplete: number | null;
}
export interface IndexIssue {
  path: string;
  message: string;
}
export type IndexPhase = "idle" | "scanning" | "paused";
export interface IndexStatus {
  phase: IndexPhase;
  currentRoot: string | null;
  scanned: number;
  /** 0..1 aproximado; `null` si no hay estimación (primer indexado de la unidad). */
  progress: number | null;
  roots: IndexRoot[];
  totalFiles: number;
  totalDirs: number;
  skipped: number;
  issues: IndexIssue[];
  lastUpdate: number | null;
}
export interface IndexSettings {
  enabled: boolean;
  /** `null`: todas las unidades fijas. Lista: solo esas raíces. */
  drives: string[] | null;
  excludedPaths: string[];
  excludedNames: string[];
  excludedRootDirs: string[];
  excludedExts: string[];
  /** Cada cuántos minutos se reconcilia una unidad ya indexada (0 = solo al arrancar). */
  reconcileMinutes: number;
}

export interface FsProvider {
  /** `false` fuera de la app de escritorio: no hay acceso a archivos reales. */
  readonly available: boolean;
  openDir(path: string, onEvent: (e: DirEvent) => void): DirHandle;
  getPlaces(): Promise<PlacesInfo>;
  loadPrefs(): Promise<Prefs>;
  savePrefs(p: Prefs): Promise<void>;
  vibrancy(): Promise<boolean>;
  setWindowTheme(theme: Theme): Promise<void>;
  /** La ventana nace oculta; la interfaz la muestra cuando ya tiene contenido. Idempotente. */
  showWindow(): Promise<void>;
  /** Guarda un error de la interfaz en el registro local de diagnóstico. */
  logUiError(scope: string, message: string, stack: string | null): Promise<void>;

  // Operaciones de archivo. Todas se validan de nuevo en el backend (Rust).
  startOp(req: OpRequest, onEvent: (e: OpEvent) => void): OpHandle;
  renamePath(path: string, newName: string): Promise<string>;
  createFolder(parent: string, name: string): Promise<string>;
  createFile(parent: string, name: string): Promise<string>;
  undoLast(): Promise<UndoResult>;
  peekUndo(): Promise<string | null>;
  openPath(path: string): Promise<void>;
  revealPath(path: string): Promise<void>;
  createShortcut(target: string): Promise<string>;
  getProperties(path: string): Promise<FileProps>;
  startStats(paths: string[], onEvent: (e: StatsEvent) => void): { cancel(): void };
  getDriveUsage(root: string): Promise<DriveUsage>;
  /** Valida lo escrito en la barra de dirección (existe, es accesible). Rechaza con el mensaje para el usuario. */
  resolvePath(input: string): Promise<ResolvedPath>;

  // Miniaturas y vista previa. `epoch`: las solicitudes con una época anterior a la vigente se descartan
  // sin trabajo en el backend; `null` = no caduca (panel de vista previa).
  getThumbnail(path: string, px: number, epoch: number | null): Promise<ThumbReply>;
  setThumbEpoch(epoch: number): Promise<void>;
  clearThumbCache(): Promise<number>;
  getTextPreview(path: string): Promise<TextPreview>;

  // Visor. `viewerOpen` prepara un archivo (una llamada nueva cancela la anterior) y da un permiso de lectura para ese archivo.
  viewerOpen(path: string): Promise<ViewerSource>;
  /** Retira los permisos y abandona lo pendiente. */
  viewerClose(): Promise<void>;
  /** URL del contenido asociado a un permiso. */
  viewerUrl(token: string): string;

  // Actualizaciones de la aplicación. Rechazan con `{ kind, message }`.
  appVersion(): Promise<string>;
  /** `null`: no hay una versión superior a la instalada. */
  updateCheck(): Promise<UpdateInfo | null>;
  /** Descarga y verifica la firma; todavía no instala. */
  updateDownload(onEvent: (e: UpdateEvent) => void): Promise<void>;
  updateCancel(): Promise<void>;
  /** Instala lo descargado. En Windows la aplicación se cierra y el instalador la vuelve a abrir. */
  updateRestart(): Promise<void>;

  /** Iconos del sistema (los del Explorador de Windows) de un lote de elementos. `have`: claves que ya se tienen. */
  getIcons(items: IconItem[], px: number, have: string[]): Promise<IconsReply>;
  clearIconCache(): Promise<number>;

  /** Portapapeles de archivos del sistema (copiar/cortar del Explorador). */
  clipboardSetFiles(paths: string[], cut: boolean): Promise<number>;
  clipboardReadFiles(): Promise<SystemClipboardFiles | null>;
  clipboardSequence(): Promise<number>;
  /** «Abrir con…»: el diálogo de Windows para elegir aplicación. */
  openWith(path: string): Promise<void>;
  /** Terminal (Windows Terminal si existe; si no, PowerShell) en una carpeta. */
  openTerminal(dir: string): Promise<void>;
  /** Comprime en un `.zip` dentro de `dir`; devuelve la ruta del archivo creado. */
  compressPaths(paths: string[], dir: string): Promise<string>;
  /** Extrae un archivo comprimido en una carpeta nueva junto a él; devuelve la carpeta. */
  extractArchive(path: string): Promise<string>;
  getSendTo(): Promise<SendToItem[]>;
  sendTo(paths: string[], id: string): Promise<SendResult>;
  getNewTemplates(): Promise<NewTemplate[]>;
  createFromTemplate(dir: string, id: string): Promise<string>;
  /** ¿Hay archivos en el portapapeles del sistema? (consulta barata, sin leer su contenido) */
  clipboardHasFiles(): Promise<boolean>;

  // Búsqueda global (índice persistente en Rust). Una búsqueda nueva cancela la anterior en el backend.
  indexSearch(query: SearchQuery): Promise<SearchPage>;
  indexStatus(): Promise<IndexStatus>;
  indexGetSettings(): Promise<IndexSettings>;
  indexSetSettings(settings: IndexSettings): Promise<void>;
  indexAvailableDrives(): Promise<string[]>;
  /** Inicia un indexado (solo lo que hace falta). `false` si ya hay uno en marcha. */
  indexStart(): Promise<boolean>;
  indexPause(): Promise<void>;
  indexResume(): Promise<void>;
  /** Borra el índice y lo construye de nuevo. */
  indexRebuild(): Promise<boolean>;
}

/** Extrae un mensaje legible de lo que rechaza una llamada IPC (texto u objeto OpError). */
export function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return "Error desconocido.";
}
