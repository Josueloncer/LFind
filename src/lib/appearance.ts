// Personalización visual: modelo, límites y traducción a variables CSS. Puro, sin React ni IPC. Ver `appearance.test.ts`.

export const BG_SLOTS = ["general", "sidebar", "main", "preview"] as const;
export type BgSlot = (typeof BG_SLOTS)[number];
export const COLOR_KEYS = ["accent", "selection", "border", "text", "interactive", "folder"] as const;
export type ColorKey = (typeof COLOR_KEYS)[number];
export type Density = "compact" | "normal" | "comfortable";

export interface Appearance {
  /** Identificador de la imagen de cada zona, o `null`. */
  backgrounds: Record<BgSlot, string | null>;
  blur: number;
  intensity: number;
  contrast: number;
  /** Opacidad de los paneles sobre el fondo. Tiene un mínimo para que los nombres se lean siempre. */
  surface: number;
  colors: Record<ColorKey, string | null>;
  radius: number;
  spacing: number;
  density: Density;
  iconScale: number;
  font: string;
  fontScale: number;
  sidebarWidth: number;
}

export interface CustomTheme {
  id: string;
  name: string;
  appearance: Appearance;
}

export const LIMITS = {
  blur: [0, 40],
  intensity: [20, 100],
  contrast: [60, 140],
  surface: [68, 100],
  radius: [0, 200],
  spacing: [85, 125],
  iconScale: [85, 130],
  fontScale: [88, 120],
  sidebarWidth: [180, 480],
} as const;
type NumberKey = keyof typeof LIMITS;

export const DEFAULT_APPEARANCE: Appearance = {
  backgrounds: { general: null, sidebar: null, main: null, preview: null },
  blur: 14,
  intensity: 100,
  contrast: 100,
  surface: 82,
  colors: { accent: null, selection: null, border: null, text: null, interactive: null, folder: null },
  radius: 100,
  spacing: 100,
  density: "normal",
  iconScale: 100,
  font: "theme",
  fontScale: 100,
  sidebarWidth: 240,
};

/** Lo mínimo que debe quedar para el contenido al ensanchar la barra lateral. */
const MIN_CONTENT = 420;

export function clampSidebarWidth(width: number, windowWidth: number): number {
  const [lo, hi] = LIMITS.sidebarWidth;
  return Math.round(Math.max(lo, Math.min(hi, windowWidth - MIN_CONTENT, width)));
}

/** Ancho que corresponde a la posición del puntero: la barra crece desde el borde de la ventana en el que esté. */
export const sidebarWidthAt = (clientX: number, side: "left" | "right", windowWidth: number): number => clampSidebarWidth(side === "left" ? clientX : windowWidth - clientX, windowWidth);

/** Tipografías del sistema: ninguna se descarga. `theme` deja la del tema. */
export const FONTS: readonly { id: string; label: string; stack: string | null }[] = [
  { id: "theme", label: "La del tema", stack: null },
  { id: "segoe", label: "Segoe UI", stack: '"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif' },
  { id: "arial", label: "Arial", stack: "Arial, Helvetica, sans-serif" },
  { id: "verdana", label: "Verdana", stack: "Verdana, Geneva, sans-serif" },
  { id: "tahoma", label: "Tahoma", stack: "Tahoma, Geneva, sans-serif" },
  { id: "georgia", label: "Georgia", stack: "Georgia, serif" },
  { id: "mono", label: "Cascadia Mono", stack: '"Cascadia Mono", Consolas, monospace' },
];

const CUSTOM_FONT = "custom:";

/** ¿Se puede escribir este nombre dentro de una cadena CSS sin romperla? */
const safeFamily = (name: string): boolean => name.trim().length > 0 && name.length <= 100 && !/["\\\u0000-\u001f]/.test(name);

export const customFont = (family: string): string => CUSTOM_FONT + family;

const customFamily = (id: string): string | null => {
  if (!id.startsWith(CUSTOM_FONT)) return null;
  const family = id.slice(CUSTOM_FONT.length);
  return safeFamily(family) ? family : null;
};

/** Pila CSS de una elección. Si la fuente deja de estar instalada, el navegador pasa a las de respaldo y la elección se conserva. */
export function fontStack(id: string): string | null {
  const family = customFamily(id);
  return family ? `"${family}", "Segoe UI", system-ui, sans-serif` : FONTS.find((f) => f.id === id)?.stack ?? null;
}

export const fontLabel = (id: string): string => customFamily(id) ?? FONTS.find((f) => f.id === id)?.label ?? FONTS[0].label;

export interface FontChoice {
  id: string;
  label: string;
  stack: string | null;
}

/** Opciones del selector: «la del tema» y las instaladas o, si no se pudieron consultar, las predefinidas. */
export function fontChoices(installed: readonly string[], query = ""): FontChoice[] {
  const base: FontChoice[] = installed.length ? [{ id: "theme", label: FONTS[0].label, stack: null }, ...installed.map((f) => ({ id: customFont(f), label: f, stack: fontStack(customFont(f)) }))] : FONTS.map((f) => ({ id: f.id, label: f.label, stack: f.stack }));
  const q = query.trim().toLowerCase();
  return q ? base.filter((f) => f.label.toLowerCase().includes(q)) : base;
}

export const PRESETS: readonly { id: string; label: string; patch: Partial<Appearance> }[] = [
  { id: "base", label: "Original", patch: {} },
  { id: "glass", label: "Cristal", patch: { surface: 70, blur: 26, radius: 140 } },
  { id: "compact", label: "Compacto", patch: { density: "compact", spacing: 90, fontScale: 94, iconScale: 92, radius: 70, sidebarWidth: 210 } },
  { id: "comfort", label: "Cómodo", patch: { density: "comfortable", spacing: 112, fontScale: 106, iconScale: 110, sidebarWidth: 270 } },
  { id: "sharp", label: "Nítido", patch: { radius: 30, surface: 92, blur: 6 } },
];

const clamp = (v: unknown, [lo, hi]: readonly [number, number], fallback: number): number => {
  const n = typeof v === "number" && Number.isFinite(v) ? v : fallback;
  return Math.min(hi, Math.max(lo, Math.round(n)));
};

const HEX = /^#[0-9a-f]{6}$/i;
const IMG = /^[0-9a-f]{16}$/;

/** Acepta lo guardado solo si es válido: un dato dañado o de otra versión vuelve a su valor por defecto, campo por campo. */
export function normalizeAppearance(raw: unknown): Appearance {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const bg = (o.backgrounds && typeof o.backgrounds === "object" ? o.backgrounds : {}) as Record<string, unknown>;
  const col = (o.colors && typeof o.colors === "object" ? o.colors : {}) as Record<string, unknown>;
  const out: Appearance = { ...DEFAULT_APPEARANCE, backgrounds: { ...DEFAULT_APPEARANCE.backgrounds }, colors: { ...DEFAULT_APPEARANCE.colors } };
  for (const s of BG_SLOTS) out.backgrounds[s] = typeof bg[s] === "string" && IMG.test(bg[s] as string) ? (bg[s] as string) : null;
  for (const c of COLOR_KEYS) out.colors[c] = typeof col[c] === "string" && HEX.test(col[c] as string) ? (col[c] as string).toLowerCase() : null;
  for (const k of Object.keys(LIMITS) as NumberKey[]) out[k] = clamp(o[k], LIMITS[k], DEFAULT_APPEARANCE[k]);
  if (o.density === "compact" || o.density === "comfortable") out.density = o.density;
  if (typeof o.font === "string" && fontStack(o.font) !== null || o.font === "theme") out.font = o.font as string;
  // Quien viene de la personalización por zonas conserva lo que veía en el contenido, que es la superficie principal.
  const main = (o.zones && typeof o.zones === "object" ? (o.zones as Record<string, unknown>).main : null) as Record<string, unknown> | null;
  if (main && typeof main === "object") {
    out.blur = main.blurOn === false ? 0 : clamp(main.blur, LIMITS.blur, out.blur);
    if (typeof main.transparency === "number") out.surface = clamp(100 - main.transparency, LIMITS.surface, out.surface);
  }
  return out;
}

export function normalizeThemes(raw: unknown): CustomTheme[] {
  if (!Array.isArray(raw)) return [];
  const out: CustomTheme[] = [];
  for (const t of raw.slice(0, 32)) {
    const o = t as Record<string, unknown> | null;
    if (o && typeof o.id === "string" && typeof o.name === "string" && o.name.trim() && !out.some((x) => x.id === o.id)) out.push({ id: o.id, name: o.name.trim().slice(0, 40), appearance: normalizeAppearance(o.appearance) });
  }
  return out;
}

export const isDefaultAppearance = (a: Appearance): boolean => JSON.stringify(a) === JSON.stringify(DEFAULT_APPEARANCE);

// ───────── color ─────────

export function rgbOf(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function luminance(hex: string): number {
  const [r, g, b] = rgbOf(hex).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** Texto legible sobre un color. */
export const onColor = (hex: string): string => (contrastRatio(hex, "#000000") >= contrastRatio(hex, "#ffffff") ? "#0f1115" : "#ffffff");

const alpha = (hex: string, a: number) => `rgba(${rgbOf(hex).join(", ")}, ${a})`;

/** Medidas del tema vigente, antes de personalizar. */
export type ThemeBase = (name: string) => number;

const SPACES = ["--sp-0", "--sp-1", "--sp-2", "--sp-3", "--sp-4", "--sp-5", "--sp-6", "--sp-7", "--sp-8", "--sp-9"];
const RADII = ["--r-xs", "--r-s", "--r-m", "--r-l"];
const ICONS = ["--icon-s", "--icon-m", "--icon-l"];
const SIZES = ["--fs-xs", "--fs-s", "--fs-m", "--fs-l", "--fs-xl"];
const COLOR_VARS = ["--accent", "--accent-rgb", "--accent-2", "--on-accent", "--focus-ring", "--sel", "--sel-edge", "--line", "--line-strong", "--text", "--text-2", "--text-3", "--bg-hover", "--bg-press", "--fold-back-a", "--fold-back-b", "--fold-front-a", "--fold-front-b", "--cat-folder"];
const OTHER_VARS = ["--font", "--font-display", "--sidebar-w-open", "--cz-row", "--cz-surface", "--cz-blur", "--cz-intensity", "--cz-contrast"];

/** Todas las variables que puede fijar la personalización; se retiran antes de medir el tema. */
export const MANAGED_VARS: readonly string[] = [...SPACES, ...RADII, ...ICONS, ...SIZES, ...COLOR_VARS, ...OTHER_VARS];

const px = (n: number) => `${Math.round(n * 10) / 10}px`;
const ROW_DELTA: Record<Density, number> = { compact: -4, normal: 0, comfortable: 6 };
export const rowDelta = (d: Density): number => ROW_DELTA[d];

/** Variables CSS de una personalización. Lo que está en su valor original no se escribe: el tema sigue mandando. */
export function appearanceVars(a: Appearance, base: ThemeBase): Record<string, string> {
  const v: Record<string, string> = {};
  const scale = (names: string[], pct: number) => {
    if (pct !== 100) for (const n of names) v[n] = px((base(n) * pct) / 100);
  };
  scale(RADII, a.radius);
  scale(SPACES, a.spacing);
  scale(ICONS, a.iconScale);
  scale(SIZES, a.fontScale);
  const font = fontStack(a.font);
  if (font) (v["--font"] = font), (v["--font-display"] = font);
  if (a.sidebarWidth !== DEFAULT_APPEARANCE.sidebarWidth) v["--sidebar-w-open"] = `${a.sidebarWidth}px`;
  if (a.density !== "normal") v["--cz-row"] = `${ROW_DELTA[a.density]}px`;
  const c = a.colors;
  if (c.accent) {
    v["--accent"] = c.accent;
    v["--accent-rgb"] = rgbOf(c.accent).join(", ");
    v["--accent-2"] = c.accent;
    v["--on-accent"] = onColor(c.accent);
    v["--focus-ring"] = alpha(c.accent, 0.75);
    if (!c.selection) (v["--sel"] = alpha(c.accent, 0.2)), (v["--sel-edge"] = alpha(c.accent, 0.38));
  }
  if (c.selection) (v["--sel"] = alpha(c.selection, 0.28)), (v["--sel-edge"] = alpha(c.selection, 0.5));
  if (c.border) (v["--line"] = alpha(c.border, 0.18)), (v["--line-strong"] = alpha(c.border, 0.34));
  if (c.text) (v["--text"] = c.text), (v["--text-2"] = alpha(c.text, 0.74)), (v["--text-3"] = alpha(c.text, 0.56));
  if (c.interactive) (v["--bg-hover"] = alpha(c.interactive, 0.14)), (v["--bg-press"] = alpha(c.interactive, 0.24));
  if (c.folder) {
    // Las mismas luces y sombras que los iconos de los temas, con el tono elegido.
    v["--fold-front-a"] = `color-mix(in srgb, ${c.folder} 68%, white)`;
    v["--fold-front-b"] = c.folder;
    v["--fold-back-a"] = `color-mix(in srgb, ${c.folder} 82%, black)`;
    v["--fold-back-b"] = `color-mix(in srgb, ${c.folder} 68%, black)`;
    v["--cat-folder"] = c.folder;
  }
  v["--cz-surface"] = `${a.surface}%`;
  v["--cz-blur"] = `${a.blur}px`;
  v["--cz-intensity"] = String(a.intensity / 100);
  v["--cz-contrast"] = String(a.contrast / 100);
  return v;
}

/** Qué hay que tocar para pasar de unas variables a otras: cada escritura en la raíz obliga a recalcular los estilos de toda la ventana. */
export function diffVars(prev: Record<string, string>, next: Record<string, string>): { set: [string, string][]; remove: string[] } {
  return { set: Object.entries(next).filter(([k, v]) => prev[k] !== v), remove: Object.keys(prev).filter((k) => !(k in next)) };
}

export interface LegibilityIssue {
  key: ColorKey;
  message: string;
}

/** Avisos de combinaciones difíciles de leer, con el color de fondo vigente. */
export function legibilityIssues(a: Appearance, background: string): LegibilityIssue[] {
  const out: LegibilityIssue[] = [];
  if (a.colors.text && contrastRatio(a.colors.text, background) < 4.5) out.push({ key: "text", message: "El texto se lee con dificultad sobre este fondo." });
  if (a.colors.accent && contrastRatio(a.colors.accent, background) < 3) out.push({ key: "accent", message: "El acento apenas se distingue del fondo." });
  return out;
}
