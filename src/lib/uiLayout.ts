// Distribución personalizable de la interfaz. Identificadores estables: nunca dependen del texto visible. Ver `uiLayout.test.ts`.
import { DEFAULT_LAYOUT } from "./fs/types";
import type { Layout } from "./fs/types";
import { applyOrder } from "./reorder";

export const SECTION_IDS = ["locations", "recents", "computer", "storage"] as const;
export type SectionId = (typeof SECTION_IDS)[number];

export interface ToolDef {
  id: string;
  label: string;
  /** Se dibuja un separador entre herramientas de grupos distintos. */
  group: string;
  /** Sin esto la función quedaría inaccesible: la búsqueda y la ruta necesitan su campo. */
  hideable: boolean;
  /** Se oculta sola en ventanas estrechas. */
  narrow?: boolean;
}

/** Orden original. `address` es la barra de dirección: no se mueve, y separa la zona izquierda de la derecha. */
export const TOOLS: readonly ToolDef[] = [
  { id: "panel", label: "Panel lateral", group: "panel", hideable: true },
  { id: "back", label: "Atrás", group: "nav", hideable: true },
  { id: "forward", label: "Adelante", group: "nav", hideable: true },
  { id: "up", label: "Subir un nivel", group: "nav", hideable: true },
  { id: "address", label: "Barra de dirección", group: "address", hideable: false },
  { id: "new", label: "Nuevo", group: "folder", hideable: true },
  { id: "sort", label: "Ordenar", group: "folder", hideable: true },
  { id: "view", label: "Ver", group: "folder", hideable: true },
  { id: "hidden", label: "Elementos ocultos", group: "toggles", hideable: true, narrow: true },
  { id: "pin", label: "Fijar en Ubicaciones", group: "toggles", hideable: true, narrow: true },
  { id: "preview", label: "Panel de vista previa", group: "toggles", hideable: true, narrow: true },
  { id: "global", label: "Buscar en todos los archivos", group: "search", hideable: true },
  { id: "filter", label: "Filtro y búsqueda", group: "search", hideable: false },
];

export const toolDef = (id: string): ToolDef | undefined => TOOLS.find((t) => t.id === id);

const strings = (v: unknown): string[] => (Array.isArray(v) ? [...new Set(v.filter((x): x is string => typeof x === "string"))] : []);

/** Lo que venga guardado se acepta solo si tiene la forma correcta: un dato dañado nunca rompe la interfaz. */
export function normalizeLayout(raw: unknown): Layout {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  return {
    sections: strings(o.sections),
    places: strings(o.places),
    drives: strings(o.drives),
    toolbar: strings(o.toolbar),
    hiddenTools: strings(o.hiddenTools).filter((id) => toolDef(id)?.hideable),
  };
}

export const sectionOrder = (l: Layout): string[] => applyOrder([...SECTION_IDS] as string[], (s) => s, l.sections);
export const toolOrder = (l: Layout): string[] => applyOrder(TOOLS.map((t) => t.id), (s) => s, l.toolbar);

/**
 * Orden único de la sección Ubicaciones: carpetas fijadas y ubicaciones del sistema forman una sola lista.
 * Una carpeta fijada que aún no tiene posición guardada va arriba, como siempre han aparecido; lo demás nuevo, al final.
 */
export function locationOrder(pins: readonly string[], places: readonly string[], saved: readonly string[]): string[] {
  const placed = new Set(saved);
  return applyOrder([...pins, ...places], (k) => k, [...pins.filter((k) => !placed.has(k)), ...saved]);
}

export function toggleTool(l: Layout, id: string): Layout {
  if (!toolDef(id)?.hideable) return l;
  return { ...l, hiddenTools: l.hiddenTools.includes(id) ? l.hiddenTools.filter((x) => x !== id) : [...l.hiddenTools, id] };
}

/** Elementos a la izquierda y a la derecha de la barra de dirección. */
export function toolZones(shown: readonly string[]): { left: string[]; right: string[] } {
  const i = shown.indexOf("address");
  return { left: shown.slice(0, i), right: shown.slice(i + 1) };
}

/** ¿Va un separador entre estas dos herramientas? */
export const needsSeparator = (a: string, b: string): boolean => toolDef(a)?.group !== toolDef(b)?.group;

export const isDefaultLayout = (l: Layout): boolean => JSON.stringify(l) === JSON.stringify(DEFAULT_LAYOUT);
