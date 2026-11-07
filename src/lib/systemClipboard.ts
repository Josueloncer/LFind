/**
 * Sincronía del portapapeles de archivos de LFind con el de Windows (CF_HDROP + «Preferred DropEffect»).
 *  · Copiar/cortar en LFind → se publica en el sistema (se puede pegar en el Explorador, en un correo, etc.).
 *  · Pegar en LFind → si otra aplicación cambió el portapapeles del sistema desde la última sincronía, se usa lo suyo.
 * Fuera de Windows (o si falla) todo sigue funcionando con el portapapeles interno.
 */
import { fs } from "./fs";
import { useClipboard } from "../stores/clipboard";

export type ClipMode = "copy" | "cut";

/**
 * ¿Hay archivos en el portapapeles del sistema? Se mantiene en memoria (al arrancar, al volver a la ventana y tras copiar)
 * para que abrir un menú —que debe activar o no «Pegar»— no consulte nada.
 */
let sysHasFiles = false;
export const systemHasFiles = (): boolean => sysHasFiles;
export async function refreshSystemHasFiles(): Promise<void> {
  try {
    sysHasFiles = await fs.clipboardHasFiles();
  } catch {
    /* se conserva el último valor */
  }
}
let watching = false;
/** Idempotente. Actualiza `systemHasFiles` al arrancar y cada vez que la ventana recupera el foco (otra app pudo copiar). */
export function watchSystemClipboard(): void {
  if (watching || typeof window === "undefined") return;
  watching = true;
  void refreshSystemHasFiles();
  window.addEventListener("focus", () => void refreshSystemHasFiles());
}

/** Publica `paths` en el portapapeles del sistema. No lanza: si falla se queda el interno. */
export async function pushToSystem(paths: string[], mode: ClipMode): Promise<void> {
  try {
    const seq = await fs.clipboardSetFiles(paths, mode === "cut");
    sysHasFiles = true;
    const c = useClipboard.getState();
    // Solo si el almacén sigue conteniendo lo mismo (otra copia posterior habría reemplazado ya el contenido).
    if (c.mode === mode && c.paths === paths) c.setSeq(seq);
  } catch {
    /* sin portapapeles del sistema: se usa el interno */
  }
}

/**
 * Antes de pegar: ¿cambió el portapapeles del sistema por fuera de LFind? Si es así, lo trae al almacén.
 * Barato: solo consulta el número de secuencia; lee el contenido únicamente si cambió.
 */
export async function pullFromSystem(): Promise<void> {
  try {
    const c = useClipboard.getState();
    const seq = await fs.clipboardSequence();
    if (!seq || seq === c.seq) return;
    const sys = await fs.clipboardReadFiles();
    if (sys && sys.paths.length) useClipboard.getState().set(sys.paths, sys.cut ? "cut" : "copy", sys.sequence);
    else useClipboard.getState().setSeq(seq); // el sistema tiene otra cosa (texto…): lo interno sigue valiendo
  } catch {
    /* ignorar: se usa el interno */
  }
}
