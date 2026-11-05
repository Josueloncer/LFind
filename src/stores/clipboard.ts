import { create } from "zustand";

/**
 * Portapapeles interno de LFind (copiar/cortar entre carpetas, pestañas y paneles).
 * No se sincroniza con el portapapeles de Windows: pegar en el Explorador de Windows no está soportado todavía.
 */
interface ClipState {
  paths: string[];
  mode: "copy" | "cut" | null;
  set(paths: string[], mode: "copy" | "cut"): void;
  clear(): void;
}

export const useClipboard = create<ClipState>((set) => ({
  paths: [],
  mode: null,
  set: (paths, mode) => set({ paths, mode }),
  clear: () => set({ paths: [], mode: null }),
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
