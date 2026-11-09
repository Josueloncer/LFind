import { create } from "zustand";
import type { Command } from "../lib/commandSearch";

/** Fuente de comandos. `all` se consulta al abrir la paleta; `forQuery` añade resultados que dependen de lo escrito. */
export interface CommandProvider {
  all(): readonly Command[];
  forQuery?(query: string): readonly Command[];
}

const providers = new Set<CommandProvider>();

/** Punto de extensión: registrar aquí cualquier comando futuro. Devuelve la función para retirarlo. */
export function registerCommands(p: CommandProvider): () => void {
  providers.add(p);
  return () => void providers.delete(p);
}

export const allCommands = (): Command[] => [...providers].flatMap((p) => [...p.all()]);
export const queryCommands = (query: string): Command[] => [...providers].flatMap((p) => [...(p.forQuery?.(query) ?? [])]);

const MAX_RECENT = 8;

export const usePalette = create<{ open: boolean; recent: string[]; show(): void; hide(): void; toggle(): void; used(id: string): void }>((set) => ({
  open: false,
  recent: [],
  show: () => set({ open: true }),
  hide: () => set({ open: false }),
  toggle: () => set((s) => ({ open: !s.open })),
  used: (id) => set((s) => ({ recent: [id, ...s.recent.filter((x) => x !== id)].slice(0, MAX_RECENT) })),
}));
