/**
 * Archivos soltados desde otras aplicaciones (Explorador, WinRAR, el escritorio…).
 *
 * El arrastre llega como `drop` HTML5 con `dataTransfer.files`, que no lleva rutas. WebView2 las entrega a la aplicación si la
 * página le reenvía esos objetos con `chrome.webview.postMessageWithAdditionalObjects` (ver `src-tauri/src/dropin_cmd.rs`); la
 * aplicación responde con el evento `external-drop` y aquí se lanza la copia con el gestor de operaciones de siempre (progreso,
 * cancelación sin restos, conflictos de nombres). Lo que se arrastra desde el propio LFind no pasa por aquí.
 */
import { listen } from "@tauri-apps/api/event";
import { draggedPaths } from "./dnd";
import { isTauri } from "./fs";
import { useOps } from "../stores/ops";
import { useUi } from "../stores/ui";

export const PREFIX = "lfind-drop:";

/** Último arrastre externo, para poder explicar un fallo (`window.__lfindDrop` en las herramientas de desarrollo). */
const diag = (patch: Record<string, unknown>) => {
  const w = window as unknown as { __lfindDrop?: Record<string, unknown> };
  w.__lfindDrop = { ...(w.__lfindDrop ?? {}), ...patch };
};

/**
 * Último `dragover` con archivos que no vienen de LFind. No se usa `dragleave`: Chromium lo envía sin `relatedTarget` también al
 * pasar de un elemento a otro, y el estado se perdía justo antes de soltar (la página acababa abriendo el archivo).
 */
let lastOver = 0;
let handled = false;
const ALIVE_MS = 400;
/** ¿Hay sobre la ventana un arrastre de archivos que viene de fuera de LFind? */
export const externalDragActive = (): boolean => !draggedPaths() && performance.now() - lastOver < ALIVE_MS;
const isExternal = (e: DragEvent) => !draggedPaths() && !!e.dataTransfer?.types.includes("Files");

/** Copiar salvo con Mayús, como al soltar desde otra unidad en el Explorador. */
export const externalEffect = (ev: { shiftKey: boolean }): "copy" | "move" => (ev.shiftKey ? "move" : "copy");

type WebView = { postMessageWithAdditionalObjects?(message: string, objects: unknown): void };
const webview = (): WebView | undefined => (window as unknown as { chrome?: { webview?: WebView } }).chrome?.webview;

const pending = new Map<string, { dest: string; move: boolean; parts: DropPart[] }>();
let watching = false;

export function watchExternalDrags(): void {
  if (watching || !isTauri) return;
  watching = true;
  const seen = (e: DragEvent) => void (isExternal(e) && (lastOver = performance.now()));
  window.addEventListener("dragenter", seen, true);
  window.addEventListener("dragover", seen, true);
  window.addEventListener("dragend", () => void (lastOver = 0), true);
  // Un archivo de fuera nunca debe abrirse en la ventana (el navegador navegaría a él y LFind desaparecería).
  window.addEventListener("drop", (e) => {
    if (!isExternal(e)) return;
    e.preventDefault();
    handled = false;
  }, true);
  window.addEventListener("drop", (e) => {
    if (!isExternal(e)) return;
    lastOver = 0;
    if (!handled) useUi.getState().toast("Suelta los archivos sobre una carpeta o en la lista de archivos.", "info");
  });
  void listen<{ token: string; paths: string[]; kinds: string[] }>("external-drop", ({ payload }) => {
    diag({ recibido: payload });
    const job = pending.get(payload.token);
    pending.delete(payload.token);
    if (!job) return;
    const { sources, unresolved } = resolveSources(job.parts, payload.paths);
    const ui = useUi.getState();
    if (unresolved.length) ui.toast(`No se pudo copiar ${unresolved.map((n) => `«${n}»`).join(", ")}: es una carpeta vacía o no se pudo leer.`, "error");
    if (!sources.length) return void (!unresolved.length && ui.toast("No se pudieron leer los archivos soltados.", "error"));
    useOps.getState().start({ kind: job.move ? "move" : "copy", sources, dest: job.dest });
  });
}

/** Qué se soltó, en orden: un archivo, o una carpeta representada por uno de sus archivos (`rel`, ruta dentro de ella). */
export type DropPart = { kind: "file"; name: string } | { kind: "dir"; name: string; rel: string } | { kind: "empty"; name: string };

/** La carpeta que contiene un archivo, conociendo la ruta del archivo dentro de ella. `null` si no encaja. */
export function folderFromInner(abs: string, rel: string): string | null {
  const a = abs.replace(/\//g, "\\");
  const r = "\\" + rel.replace(/\//g, "\\").replace(/^\\+/, "");
  return a.toLowerCase().endsWith(r.toLowerCase()) && a.length > r.length ? a.slice(0, a.length - r.length) : null;
}

/** Empareja lo soltado con las rutas que devolvió WebView2 (una por cada archivo enviado, en el mismo orden). */
export function resolveSources(parts: readonly DropPart[], paths: readonly string[]): { sources: string[]; unresolved: string[] } {
  const sources: string[] = [];
  const unresolved: string[] = [];
  let at = 0;
  for (const p of parts) {
    if (p.kind === "empty") {
      unresolved.push(p.name);
      continue;
    }
    const abs = paths[at++];
    const src = !abs ? null : p.kind === "file" ? abs : folderFromInner(abs, p.rel);
    if (src) sources.push(src);
    else unresolved.push(p.name);
  }
  return { sources, unresolved };
}

type Entry = { isFile: boolean; isDirectory: boolean; name: string; fullPath: string; file?(ok: (f: File) => void, err: (e: unknown) => void): void; createReader?(): { readEntries(ok: (l: Entry[]) => void, err: (e: unknown) => void): void } };

/** El primer archivo dentro de una carpeta (en anchura, con un tope para no recorrer árboles enormes). */
async function firstFileIn(dir: Entry, budget = 4000): Promise<{ file: File; rel: string } | null> {
  const queue: Entry[] = [dir];
  let seen = 0;
  while (queue.length && seen < budget) {
    const d = queue.shift()!;
    const reader = d.createReader?.();
    if (!reader) continue;
    // `readEntries` devuelve tandas: se lee hasta que llega vacía.
    for (;;) {
      const batch = await new Promise<Entry[]>((ok) => reader.readEntries(ok, () => ok([])));
      if (!batch.length) break;
      seen += batch.length;
      for (const e of batch) {
        if (e.isFile && e.file) {
          const file = await new Promise<File | null>((ok) => e.file!(ok, () => ok(null)));
          if (file) return { file, rel: e.fullPath.slice(dir.fullPath.length).replace(/^\/+/, "") };
        } else if (e.isDirectory) queue.push(e);
      }
    }
  }
  return null;
}

/** Suelta en `dest` lo arrastrado desde fuera. Devuelve `false` si no hay nada que soltar. */
export function sendExternalDrop(ev: { dataTransfer: DataTransfer; shiftKey: boolean }, dest: string): boolean {
  const wv = webview();
  const dt = ev.dataTransfer;
  if (!wv?.postMessageWithAdditionalObjects || !dt?.files.length) return false;
  const token = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  handled = true;
  // Medido con WinRAR: una carpeta enviada como `File` hace caer la página, y como `FileSystemHandle` no llega (0 objetos). Por
  // eso de cada carpeta se envía uno de sus archivos con su ruta interna, y su ruta se deduce de la de ese archivo. Las entradas
  // hay que pedirlas durante el propio evento; sus archivos se pueden leer después.
  const entries = [...dt.items].filter((i) => i.kind === "file").map((i) => ({ entry: (i as DataTransferItem & { webkitGetAsEntry?(): Entry | null }).webkitGetAsEntry?.() ?? null, file: i.getAsFile() }));
  void (async () => {
    const parts: DropPart[] = [];
    const files: File[] = [];
    for (const { entry, file } of entries) {
      if (entry?.isDirectory) {
        const inner = await firstFileIn(entry);
        if (inner) (parts.push({ kind: "dir", name: entry.name, rel: inner.rel }), files.push(inner.file));
        else parts.push({ kind: "empty", name: entry.name });
      } else if (file) {
        parts.push({ kind: "file", name: file.name });
        files.push(file);
      }
    }
    diag({ partes: parts });
    pending.set(token, { dest, move: ev.shiftKey, parts });
    wv.postMessageWithAdditionalObjects!(PREFIX + token, files);
  })();
  return true;
}
