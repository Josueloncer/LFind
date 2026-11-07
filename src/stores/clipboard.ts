import { create } from "zustand";

/**
 * Portapapeles de archivos de LFind (copiar/cortar entre carpetas, pestañas y paneles).
 * Se sincroniza con el portapapeles de Windows (`lib/systemClipboard.ts`): copiar/cortar aquí publica los archivos
 * como el Explorador, y lo copiado fuera de LFind se pega aquí. `seq` es el número de secuencia del portapapeles del
 * sistema que corresponde a lo que contiene este almacén (0 = no sincronizado).
 */
interface ClipState {
  paths: string[];
  mode: "copy" | "cut" | null;
  seq: number;
  set(paths: string[], mode: "copy" | "cut", seq?: number): void;
  setSeq(seq: number): void;
  clear(): void;
}

export const useClipboard = create<ClipState>((set) => ({
  paths: [],
  mode: null,
  seq: 0,
  set: (paths, mode, seq = 0) => set({ paths, mode, seq }),
  setSeq: (seq) => set({ seq }),
  clear: () => set({ paths: [], mode: null }), // `seq` se conserva: lo que sigue en el sistema no se vuelve a importar
}));

/** Copia texto al portapapeles del sistema (rutas, nombres). */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.cssText = "position:fixed;opacity:0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      ta.remove();
      return ok;
    } catch {
      return false;
    }
  }
}
