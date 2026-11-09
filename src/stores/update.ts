import { create } from "zustand";
import { errMsg, fs } from "../lib/fs";
import type { UpdateInfo } from "../lib/fs";
import { usePrefs } from "./prefs";
import { useUi } from "./ui";

export type UpdatePhase =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "latest" }
  | { phase: "available"; info: UpdateInfo }
  | { phase: "downloading"; info: UpdateInfo; done: number; total: number | null }
  | { phase: "verifying"; info: UpdateInfo }
  | { phase: "ready"; info: UpdateInfo }
  | { phase: "installing"; info: UpdateInfo }
  | { phase: "error"; message: string; info: UpdateInfo | null };

/** Entre comprobaciones automáticas: arrancar LFind varias veces seguidas no debe repetir la consulta. */
export const AUTO_CHECK_EVERY_MS = 6 * 3_600_000;
export const shouldAutoCheck = (enabled: boolean, last: number, now: number): boolean => enabled && now - last >= AUTO_CHECK_EVERY_MS;

const CHECK_FAILED = "No se pudo comprobar si hay actualizaciones.";

interface UpdateState {
  state: UpdatePhase;
  installed: string;
  loadVersion(): void;
  check(): Promise<void>;
  /** Consulta al iniciar: sin interfaz, y si encuentra algo solo avisa. Nunca descarga. */
  autoCheck(): Promise<void>;
  download(): Promise<void>;
  cancel(): void;
  restart(): Promise<void>;
}

const kindOf = (e: unknown) => (e as { kind?: string } | null)?.kind;

export const useUpdate = create<UpdateState>((set, get) => ({
  state: { phase: "idle" },
  installed: "",

  loadVersion() {
    if (!get().installed && fs.available) void fs.appVersion().then((installed) => set({ installed })).catch(() => {});
  },

  async check() {
    const s = get().state.phase;
    if (s === "checking" || s === "downloading" || s === "verifying" || s === "ready" || s === "installing") return;
    set({ state: { phase: "checking" } });
    try {
      const info = await fs.updateCheck();
      set({ state: info ? { phase: "available", info } : { phase: "latest" } });
    } catch (e) {
      const offline = kindOf(e) === "offline";
      set({ state: { phase: "error", info: null, message: offline ? `${CHECK_FAILED} ${errMsg(e)}` : CHECK_FAILED } });
    }
  },

  async autoCheck() {
    const prefs = usePrefs.getState();
    if (!fs.available || get().state.phase !== "idle" || !shouldAutoCheck(prefs.autoUpdateCheck, prefs.lastUpdateCheck, Date.now())) return;
    prefs.update({ lastUpdateCheck: Date.now() });
    try {
      const info = await fs.updateCheck();
      if (!info || get().state.phase !== "idle") return;
      set({ state: { phase: "available", info } });
      useUi.getState().toast(`LFind ${info.version} está disponible`, "info", { label: "Ver", run: () => useUi.getState().openDialog({ type: "update" }) }, 12_000);
    } catch {
      /* comprobación en segundo plano: un fallo no merece molestar al usuario */
    }
  },

  async download() {
    const cur = get().state;
    if (cur.phase !== "available" && !(cur.phase === "error" && cur.info)) return;
    const info = cur.info as UpdateInfo;
    set({ state: { phase: "downloading", info, done: 0, total: info.size } });
    try {
      await fs.updateDownload((e) => {
        if (get().state.phase === "installing") return;
        set({ state: e.type === "verifying" ? { phase: "verifying", info } : { phase: "downloading", info, done: e.done, total: e.total ?? info.size } });
      });
      set({ state: { phase: "ready", info } });
    } catch (e) {
      set({ state: kindOf(e) === "cancelled" ? { phase: "available", info } : { phase: "error", info, message: errMsg(e) } });
    }
  },

  cancel: () => void fs.updateCancel().catch(() => {}),

  async restart() {
    const cur = get().state;
    if (cur.phase !== "ready") return;
    set({ state: { phase: "installing", info: cur.info } });
    await usePrefs.getState().flush().catch(() => {});
    try {
      await fs.updateRestart(); // en Windows, si todo va bien, la aplicación termina aquí
    } catch (e) {
      set({ state: kindOf(e) === "signature" ? { phase: "error", info: cur.info, message: errMsg(e) } : { phase: "ready", info: cur.info } });
      useUi.getState().toast(errMsg(e), "error");
    }
  },
}));
