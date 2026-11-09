import type { EffectiveView } from "../lib/viewState";

/** Lo que el panel activo expone a las herramientas globales (paleta de comandos, inspector). No guarda nada: el panel lo actualiza al enfocarse. */
export interface PaneActions {
  path: string | null;
  view: EffectiveView;
  setView(patch: Partial<EffectiveView>): void;
  reload(): void;
}

let current: PaneActions | null = null;

export const setPaneActions = (a: PaneActions | null): void => void (current = a);
export const paneActions = (): PaneActions | null => current;
