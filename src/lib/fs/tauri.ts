import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { schemeOf } from "../themes";
import { DEFAULT_PREFS } from "./types";
import type {
  DirEvent, DirHandle, NewTemplate, SendResult, SendToItem, DriveUsage, ResolvedPath, FileProps, IconItem, IconsReply, FsProvider, IndexSettings, IndexStatus, OpEvent, OpHandle, OpRequest, PlacesInfo, Prefs, SearchPage, SearchQuery, StatsEvent, SystemClipboardFiles, TextPreview, Theme, ThumbReply, UndoResult, ViewerSource,
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
