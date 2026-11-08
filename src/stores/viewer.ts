import { create } from "zustand";

/** Contexto de navegación: lo aporta quien muestra la lista (carpeta o resultados de búsqueda). */
export interface ViewerNav {
  /** Selecciona el archivo anterior (-1) o siguiente (1). */
  step(dir: 1 | -1): void;
  /** Posición del archivo actual entre los archivos de la lista. */
  position(): { index: number; total: number } | null;
  /** ¿Hay un elemento único seleccionado que mostrar? */
  hasTarget(): boolean;
}

/** Órdenes de teclado que atiende el visualizador activo. */
export interface ViewerCommands {
  zoomIn(): void;
  zoomOut(): void;
  fit(): void;
  actual?(): void;
  rotate?(): void;
}

interface ViewerState {
  /** Vista ampliada sobre el explorador. El panel lateral es una preferencia (`previewPane`). */
  expanded: boolean;
  nav: ViewerNav | null;
  commands: ViewerCommands | null;
  expand(): void;
  collapse(): void;
  toggle(): void;
  bindNav(nav: ViewerNav | null): void;
  bindCommands(c: ViewerCommands | null): void;
}

/** Controlador central del visor: qué modo está abierto y cómo moverse. El contenido lo resuelve `useViewerSource`. */
export const useViewer = create<ViewerState>((set, get) => ({
  expanded: false,
  nav: null,
  commands: null,
  expand() {
    if (get().nav?.hasTarget()) set({ expanded: true });
  },
  collapse: () => set({ expanded: false }),
  toggle() {
    if (get().expanded) set({ expanded: false });
    else get().expand();
  },
  bindNav: (nav) => set({ nav }),
  bindCommands: (commands) => set({ commands }),
}));
