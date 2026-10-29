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

export type Theme = "system" | "dark" | "light";
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
export interface DriveUsage {
  root: string;
  total: number;
  free: number;
  used: number;
  kind: "fixed" | "removable" | "network" | "cdrom" | "ramdisk" | "unknown";
  label: string | null;
  fs: string | null;
}
export interface UndoResult {
  description: string;
  restored: number;
  errors: OpError[];
}

/**
 * Contrato entre la interfaz y el sistema. Las pantallas nunca llaman a Tauri
 * directamente: todo pasa por aquí, de modo que las operaciones futuras
 * (copiar, mover, buscar, miniaturas) se añaden como métodos nuevos.
 */
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
}

/** Extrae un mensaje legible de lo que rechaza una llamada IPC (texto u objeto OpError). */
export function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return "Error desconocido.";
}
