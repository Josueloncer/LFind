import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { schemeOf } from "../themes";
import { DEFAULT_PREFS } from "./types";
import type {
  Inspection,
  HashEvent,
  AvailStat,
  RecycleItem,
  TerminalShell,
  WinFeature,
  WinStatus,
  DupConfig,
  DupEvent,
  DupGroup,
  DupPending,
  DupSummary,
  AnalyzeEvent, CatStat, DirHit, DirsQuery, ExtStat, FileHit, FilesQuery, Issue, Page, ReportInfo, TreeNode, DirEvent, DirHandle, PathState, Place, NewTemplate, SendResult, SendToItem, DriveUsage, ResolvedPath, FileProps, IconItem, IconsReply, FsProvider, IndexSettings, IndexStatus, OpEvent, OpHandle, OpRequest, PlacesInfo, Prefs, SearchPage, SearchQuery, StatsEvent, SystemClipboardFiles, TextPreview, Theme, ThumbReply, UndoResult, UpdateEvent, UpdateInfo, ViewerSource,
} from "./types";

export class TauriProvider implements FsProvider {
  readonly available = true;

  openDir(path: string, onEvent: (e: DirEvent) => void): DirHandle {
    const channel = new Channel<DirEvent>();
    channel.onmessage = onEvent;
    let id: number | null = null;
    let cancelled = false;

    invoke<number>("open_dir", { path, onEvent: channel })
      .then((jobId) => {
        id = jobId;
        // Se canceló antes de recibir el id: avisar al backend ahora.
        if (cancelled) void invoke("cancel_dir", { id });
      })
      .catch((e) => onEvent({ type: "error", message: String(e) }));

    return {
      cancel() {
        cancelled = true;
        if (id !== null) void invoke("cancel_dir", { id }).catch(() => {});
      },
    };
  }

  getPlaces(): Promise<PlacesInfo> {
    return invoke<PlacesInfo>("get_places");
  }

  getCloudPlaces = () => invoke<Place[]>("get_cloud_places");
  getDrives = () => invoke<Place[]>("get_drives");
  drivesMask = () => invoke<number>("drives_mask");
  checkPaths = (paths: string[]) => invoke<PathState[]>("check_paths", { paths });
  cloudOnly = (paths: string[]) => invoke<boolean[]>("cloud_only", { paths });

  async loadPrefs(): Promise<Prefs> {
    return { ...DEFAULT_PREFS, ...(await invoke<Partial<Prefs>>("get_prefs")) };
  }

  savePrefs(p: Prefs): Promise<void> {
    return invoke("set_prefs", { prefs: p });
  }

  vibrancy(): Promise<boolean> {
    return invoke<{ vibrancy: boolean }>("get_platform_info").then((i) => i.vibrancy);
  }

  async setWindowTheme(theme: Theme): Promise<void> {
    // Cyberpunk/Hacker → oscuro, Minimal → claro; «system» devuelve el control al sistema.
    await getCurrentWindow().setTheme(schemeOf(theme));
  }

  async showWindow(): Promise<void> {
    await getCurrentWindow().show();
  }

  logUiError(scope: string, message: string, stack: string | null): Promise<void> {
    return invoke("log_ui_error", { scope, message, stack });
  }

  startOp(req: OpRequest, onEvent: (e: OpEvent) => void): OpHandle {
    const channel = new Channel<OpEvent>();
    channel.onmessage = onEvent;
    const id = invoke<number>("start_op", { req, onEvent: channel });
    id.catch((e) => onEvent({ type: "done", completed: 0, skipped: 0, cancelled: false, undoable: false, errors: [{ path: "", message: String(e), kind: "other" }] }));
    return {
      cancel: () => void id.then((i) => invoke("cancel_job", { id: i })).catch(() => {}),
      resolve: (decision, applyAll) => void id.then((i) => invoke("resolve_conflict", { id: i, decision, applyAll })).catch(() => {}),
      pause: (paused) => void id.then((i) => invoke("pause_job", { id: i, paused })).catch(() => {}),
    };
  }

  renamePath = (path: string, newName: string) => invoke<string>("rename_path", { path, newName });
  createFolder = (parent: string, name: string) => invoke<string>("create_folder", { parent, name });
  createFile = (parent: string, name: string) => invoke<string>("create_file", { parent, name });
  undoLast = () => invoke<UndoResult>("undo_last");
  peekUndo = () => invoke<string | null>("peek_undo");
  openPath = (path: string) => invoke<void>("open_path", { path });
  revealPath = (path: string) => invoke<void>("reveal_path", { path });
  createShortcut = (target: string) => invoke<string>("create_shortcut", { target });
  getProperties = (path: string) => invoke<FileProps>("get_properties", { path });
  getDriveUsage = (root: string) => invoke<DriveUsage>("get_drive_usage", { root });
  resolvePath = (input: string) => invoke<ResolvedPath>("resolve_path", { input });

  startStats(paths: string[], onEvent: (e: StatsEvent) => void) {
    const channel = new Channel<StatsEvent>();
    channel.onmessage = onEvent;
    const id = invoke<number>("start_stats", { paths, onEvent: channel });
    id.catch(() => onEvent({ type: "done", stats: { files: 0, dirs: 0, bytes: 0, errors: 1 }, cancelled: true }));
    return { cancel: () => void id.then((i) => invoke("cancel_job", { id: i })).catch(() => {}) };
  }

  getThumbnail = (path: string, px: number, epoch: number | null) => invoke<ThumbReply>("get_thumbnail", { path, px, epoch });
  setThumbEpoch = (epoch: number) => invoke<void>("set_thumb_epoch", { epoch });
  // Vaciar «miniaturas» vacía también los iconos propios de archivo guardados en disco.
  clearThumbCache = async () => (await invoke<number>("clear_thumb_cache")) + (await invoke<number>("clear_icon_cache").catch(() => 0));
  getIcons = (items: IconItem[], px: number, have: string[]) => invoke<IconsReply>("get_icons", { items, px, have });
  clearIconCache = () => invoke<number>("clear_icon_cache");
  clipboardSetFiles = (paths: string[], cut: boolean) => invoke<number>("clipboard_set_files", { paths, cut });
  clipboardReadFiles = () => invoke<SystemClipboardFiles | null>("clipboard_read_files");
  clipboardSequence = () => invoke<number>("clipboard_sequence");
  clipboardHasFiles = () => invoke<boolean>("clipboard_has_files");
  openWith = (path: string) => invoke<void>("open_with", { path });
  openTerminal = (dir: string) => invoke<void>("open_terminal", { dir });
  compressPaths = (paths: string[], dir: string) => invoke<string>("compress_paths", { paths, dir });
  extractArchive = (path: string) => invoke<string>("extract_archive", { path });
  getSendTo = () => invoke<SendToItem[]>("list_send_to");
  sendTo = (paths: string[], id: string) => invoke<SendResult>("send_to", { paths, id });
  getNewTemplates = () => invoke<NewTemplate[]>("list_new_templates");
  createFromTemplate = (dir: string, id: string) => invoke<string>("create_from_template", { dir, id });
  getTextPreview = (path: string) => invoke<TextPreview>("get_text_preview", { path });
  viewerOpen = (path: string) => invoke<ViewerSource>("viewer_open", { path });
  viewerClose = () => invoke<void>("viewer_close");
  viewerUrl = (token: string) => convertFileSrc(token, "lfview");

  appVersion = () => invoke<string>("app_version");
  updateCheck = () => invoke<UpdateInfo | null>("update_check");
  updateDownload(onEvent: (e: UpdateEvent) => void): Promise<void> {
    const channel = new Channel<UpdateEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("update_download", { onEvent: channel });
  }
  updateCancel = () => invoke<void>("update_cancel");
  updateRestart = () => invoke<void>("update_restart");

  analyzerStart(root: string, onEvent: (e: AnalyzeEvent) => void): Promise<void> {
    const channel = new Channel<AnalyzeEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("analyzer_start", { root, onEvent: channel });
  }
  analyzerUpdate(root: string, onEvent: (e: AnalyzeEvent) => void): Promise<void> {
    const channel = new Channel<AnalyzeEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("analyzer_update", { root, onEvent: channel });
  }
  analyzerWatch(root: string, onEvent: (e: AnalyzeEvent) => void): Promise<void> {
    const channel = new Channel<AnalyzeEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("analyzer_watch", { root, onEvent: channel });
  }
  analyzerUnwatch = () => invoke<void>("analyzer_unwatch");
  backgroundImport = (path: string) => invoke<string>("background_import", { path });
  listFonts = () => invoke<string[]>("list_fonts");
  dupStart(config: DupConfig, onEvent: (e: DupEvent) => void) {
    const channel = new Channel<DupEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("dup_start", { config, onEvent: channel });
  }
  dupPause = (paused: boolean) => invoke<void>("dup_pause", { paused });
  dupResume(onEvent: (e: DupEvent) => void) {
    const channel = new Channel<DupEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("dup_resume", { onEvent: channel });
  }
  dupPending = () => invoke<DupPending | null>("dup_pending");
  dupDiscard = () => invoke<void>("dup_discard");
  dupSuspend = () => invoke<void>("dup_suspend");
  dupCancel = () => invoke<void>("dup_cancel");
  dupDefaultExclusions = () => invoke<string[]>("dup_default_exclusions");
  dupSummary = () => invoke<DupSummary | null>("dup_summary");
  dupGroups = (offset: number, limit: number) => invoke<DupGroup[]>("dup_groups", { offset, limit });
  dupCheckRemoval = (paths: string[]) => invoke<{ allowed: string[]; blocked: [string, string][] }>("dup_check_removal", { paths });
  dupPrune = (paths: string[]) => invoke<void>("dup_prune", { paths });
  dupForget = () => invoke<void>("dup_forget");
  dupCompare = (a: string, b: string) => invoke<{ identical: boolean; firstDifference: number | null } | null>("dup_compare", { a, b });
  openWithList = (path: string) => invoke<string[]>("open_with_list", { path });
  openWithApp = (path: string, app: string) => invoke<void>("open_with_app", { path, app });
  startNativeDrag = (paths: string[]) => invoke<number>("start_native_drag", { paths });
  recycleList = () => invoke<{ items: RecycleItem[]; totalBytes: number | null }>("recycle_list");
  recycleRestore = (ids: string[]) => invoke<void>("recycle_restore", { ids });
  recyclePurge = (ids: string[]) => invoke<void>("recycle_purge", { ids });
  winRepair = () => invoke<WinStatus>("win_repair");
  termOpen(shell: TerminalShell, cwd: string | null, cols: number, rows: number, onData: (d: ArrayBuffer) => void, onExit: (code: number) => void) {
    const data = new Channel<ArrayBuffer>();
    data.onmessage = onData;
    const exit = new Channel<number>();
    exit.onmessage = onExit;
    return invoke<number>("term_open", { shell, cwd, cols, rows, onData: data, onExit: exit });
  }
  termWrite = (id: number, data: string) => invoke<void>("term_write", { id, data });
  termResize = (id: number, cols: number, rows: number) => invoke<void>("term_resize", { id, cols, rows });
  termAck = (id: number, bytes: number) => invoke<void>("term_ack", { id, bytes });
  termBusy = (id: number) => invoke<boolean>("term_busy", { id });
  termClose = (id: number) => invoke<void>("term_close", { id });
  takeStartupPath = () => invoke<string | null>("take_startup_path");
  onOpenPath = (handler: (path: string) => void) => listen<string>("lfind://open-path", (e) => handler(e.payload));
  winStatus = () => invoke<WinStatus>("win_status");
  winSet = (feature: WinFeature, on: boolean) => invoke<WinStatus>("win_set", { feature, on });
  winRestore = () => invoke<WinStatus>("win_restore");
  inspectPath = (path: string) => invoke<Inspection>("inspect_path", { path });
  startHash(path: string, onEvent: (e: HashEvent) => void) {
    const channel = new Channel<HashEvent>();
    channel.onmessage = onEvent;
    const id = invoke<number>("start_hash", { path, onEvent: channel });
    id.catch((e) => onEvent({ type: "done", hash: null, error: String(e) }));
    return { cancel: () => void id.then((i) => invoke("cancel_job", { id: i })).catch(() => {}) };
  }
  backgroundLoad = (id: string) => invoke<string | null>("background_load", { id });
  backgroundRetain = (keep: string[]) => invoke<number>("background_retain", { keep });
  analyzerPause = () => invoke<void>("analyzer_pause");
  analyzerResume = () => invoke<void>("analyzer_resume");
  analyzerCancel = () => invoke<void>("analyzer_cancel");
  analyzerReports = () => invoke<ReportInfo[]>("analyzer_reports");
  analyzerReport = (root: string) => invoke<ReportInfo | null>("analyzer_report", { root });
  analyzerForget = (root: string) => invoke<void>("analyzer_forget", { root });
  analyzerNode = (root: string, at: string) => invoke<DirHit | null>("analyzer_node", { root, at });
  analyzerDirs = (root: string, query: DirsQuery) => invoke<Page<DirHit>>("analyzer_dirs", { root, query });
  analyzerFiles = (root: string, query: FilesQuery) => invoke<Page<FileHit>>("analyzer_files", { root, query });
  analyzerCategories = (root: string, dir: number, avail?: string) => invoke<CatStat[]>("analyzer_categories", { root, dir, avail: avail ?? null });
  analyzerAvailability = (root: string, dir: number) => invoke<AvailStat[]>("analyzer_availability", { root, dir });
  analyzerExts = (root: string, dir: number, offset: number, limit: number, avail?: string) => invoke<Page<ExtStat>>("analyzer_exts", { root, dir, offset, limit, avail: avail ?? null });
  analyzerTreemap = (root: string, dir: number) => invoke<TreeNode[]>("analyzer_treemap", { root, dir });
  analyzerIssues = (root: string) => invoke<Issue[]>("analyzer_issues", { root });

  indexSearch = (query: SearchQuery) => invoke<SearchPage>("index_search", { query });
  indexStatus = () => invoke<IndexStatus>("index_status");
  indexGetSettings = () => invoke<IndexSettings>("index_get_settings");
  indexSetSettings = (settings: IndexSettings) => invoke<void>("index_set_settings", { settings });
  indexAvailableDrives = () => invoke<string[]>("index_available_drives");
  indexStart = () => invoke<boolean>("index_start");
  indexPause = () => invoke<void>("index_pause");
  indexResume = () => invoke<void>("index_resume");
  indexRebuild = () => invoke<boolean>("index_rebuild");
}
