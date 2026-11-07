import { create } from "zustand";
import { fs } from "../lib/fs";
import type { NewTemplate, SendToItem } from "../lib/fs";

/**
 * Datos del sistema que alimentan el menú contextual («Enviar a», plantillas de «Nuevo»). Se cargan UNA vez, en segundo
 * plano tras el arranque, y el menú los lee de memoria: abrir un menú nunca consulta al sistema.
 */
interface ShellData {
  sendTo: SendToItem[];
  templates: NewTemplate[];
  loaded: boolean;
  load(): Promise<void>;
}

let inflight: Promise<void> | null = null;

export const useShellData = create<ShellData>((set, get) => ({
  sendTo: [],
  templates: [],
  loaded: false,
  load() {
    if (get().loaded) return Promise.resolve();
    inflight ??= Promise.all([fs.getSendTo().catch(() => []), fs.getNewTemplates().catch(() => [])]).then(([sendTo, templates]) => {
      set({ sendTo, templates, loaded: true });
    });
    return inflight;
  },
}));
