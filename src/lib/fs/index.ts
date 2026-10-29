import { BrowserProvider } from "./browser";
import { TauriProvider } from "./tauri";
import type { FsProvider } from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export const fs: FsProvider = isTauri ? new TauriProvider() : new BrowserProvider();
export * from "./types";
