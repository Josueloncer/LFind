import { useCallback, useMemo } from "react";
import { errMsg, fs } from "../lib/fs";
import type { Entry, NewTemplate, SendToItem } from "../lib/fs";
import { scheduleRefresh } from "../lib/contextMenu";
import { basename, join, parent, samePath } from "../lib/path";
import { copyText, useClipboard } from "../stores/clipboard";
import { useNav } from "../stores/nav";
import { useOps } from "../stores/ops";
import { usePrefs } from "../stores/prefs";
import { pullFromSystem, pushToSystem } from "../lib/systemClipboard";
import { useUi } from "../stores/ui";

interface Args {
  path: string | null;
  visible: Entry[];
  selected: Set<string>;
  reload(): void;
  /** Tras crear algo: recargar, seleccionarlo y ponerlo en modo renombrar. */
  afterCreate(name: string): void;
  afterRename(name: string): void;
  startRename(name: string): void;
}

/** Acciones de archivo sobre la selección actual. Todas pasan por el backend, que las valida. */
export function useFileActions(a: Args) {
  const { path, visible, selected } = a;
  const toast = useUi((s) => s.toast);
  const fail = useCallback((e: unknown) => toast(errMsg(e), "error"), [toast]);

  const sel = useMemo(() => visible.filter((e) => selected.has(e.name)), [visible, selected]);
  const pathOf = useCallback((e: Entry) => join(path ?? "", e.name), [path]);

  return useMemo(() => {
    const paths = (l: Entry[]) => l.map(pathOf);
    const act = {
      sel,
      copy(l: Entry[] = sel) {
        if (!l.length) return;
        const p = paths(l);
        useClipboard.getState().set(p, "copy");
        void pushToSystem(p, "copy");
      },
      cut(l: Entry[] = sel) {
        if (!l.length) return;
        const p = paths(l);
        useClipboard.getState().set(p, "cut");
        void pushToSystem(p, "cut");
      },
      async paste(into: string | null = path) {
        await pullFromSystem(); // lo copiado fuera de LFind (Explorador, otras apps) también se puede pegar aquí
        const clip = useClipboard.getState();
        if (!into || !clip.mode || !clip.paths.length) return;
        if (clip.mode === "cut") {
          if (clip.paths.every((p) => { const par = parent(p); return par !== null && samePath(par, into); })) {
            toast("Los elementos ya están en esta carpeta.");
            return;
          }
          useOps.getState().start({ kind: "move", sources: clip.paths, dest: into });
          clip.clear(); // lo cortado se pega una sola vez
        } else {
          useOps.getState().start({ kind: "copy", sources: clip.paths, dest: into });
        }
      },
      remove(l: Entry[] = sel, permanent = false) {
        if (l.length) useUi.getState().openDialog({ type: "delete", paths: paths(l), permanent });
      },
      newFolder: () => path && fs.createFolder(path, "Nueva carpeta").then((p) => a.afterCreate(basename(p))).catch(fail),
      newFile: () => path && fs.createFile(path, "Nuevo documento.txt").then((p) => a.afterCreate(basename(p))).catch(fail),
      rename(oldName: string, newName: string) {
        if (!path || newName === oldName) return;
        fs.renamePath(join(path, oldName), newName.trim()).then((p) => a.afterRename(basename(p))).catch(fail);
      },
      open(e: Entry) {
        if (e.kind === "dir") useNav.getState().navigate(pathOf(e));
        else fs.openPath(pathOf(e)).catch(fail);
      },
      openInNewTab: (e: Entry) => useNav.getState().openTab(pathOf(e)),
      reveal: (e: Entry) => fs.revealPath(pathOf(e)).catch(fail),
      openFolderInExplorer: () => path && fs.openPath(path).catch(fail),
      copyPaths(l: Entry[]) {
        const text = l.length ? paths(l).join("\n") : path ?? "";
        void copyText(text).then((ok) => toast(ok ? (l.length > 1 ? "Rutas copiadas" : "Ruta copiada") : "No se pudo copiar al portapapeles", ok ? "success" : "error"));
      },
      shortcut: (e: Entry) => fs.createShortcut(pathOf(e)).then(() => (toast("Acceso directo creado", "success"), a.reload())).catch(fail),
      properties(l: Entry[]) {
        const p = l.length ? paths(l) : path ? [path] : [];
        if (p.length) useUi.getState().openDialog({ type: "properties", paths: p });
      },
      togglePin: (e: Entry) => usePrefs.getState().toggleFavorite(pathOf(e), e.name),
      isPinned: (e: Entry) => usePrefs.getState().favorites.some((f) => samePath(f.path, pathOf(e))),
      openWith: (e: Entry) => fs.openWith(pathOf(e)).catch(fail),
      openTerminal: (e?: Entry) => {
        const d = e ? pathOf(e) : path;
        if (d) fs.openTerminal(d).catch(fail);
      },
      compress(l: Entry[] = sel) {
        if (!path || !l.length) return;
        toast("Comprimiendo…");
        fs.compressPaths(paths(l), path).then((p) => (toast(`Creado ${basename(p)}`, "success"), a.afterCreate(basename(p)))).catch(fail);
      },
      extract(e: Entry) {
        toast("Extrayendo…");
        fs.extractArchive(pathOf(e)).then((p) => (toast(`Extraído en ${basename(p)}`, "success"), a.reload())).catch(fail);
      },
      sendTo(l: Entry[], t: SendToItem) {
        if (!l.length) return;
        fs.sendTo(paths(l), t.id)
          .then((r) => {
            if (r.kind === "zip") act.compress(l);
            else if (r.kind === "copyTo") useOps.getState().start({ kind: "copy", sources: paths(l), dest: r.dest });
            else scheduleRefresh(a.reload);
          })
          .catch(fail);
      },
      newFromTemplate(t: NewTemplate) {
        if (path) fs.createFromTemplate(path, t.id).then((p) => a.afterCreate(basename(p))).catch(fail);
      },
      undo: () => void useOps.getState().undo(),
      startRename: a.startRename,
    };
    return act;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sel, path, pathOf, fail, toast, a.reload, a.afterCreate, a.afterRename, a.startRename]);
}
