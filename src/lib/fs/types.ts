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
  /** Valida lo escrito en la barra de dirección (existe, es accesible). Rechaza con el mensaje para el usuario. */
  resolvePath(input: string): Promise<ResolvedPath>;

  // Miniaturas y vista previa. `epoch`: las solicitudes con una época anterior a la vigente se descartan
  // sin trabajo en el backend; `null` = no caduca (panel de vista previa).
  getThumbnail(path: string, px: number, epoch: number | null): Promise<ThumbReply>;
  setThumbEpoch(epoch: number): Promise<void>;
  clearThumbCache(): Promise<number>;
  getTextPreview(path: string): Promise<TextPreview>;

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
}

/** Extrae un mensaje legible de lo que rechaza una llamada IPC (texto u objeto OpError). */
export function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return "Error desconocido.";
}
