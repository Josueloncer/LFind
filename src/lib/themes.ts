import type { Theme } from "./fs";

/** Tema ya resuelto: el que se escribe en `<html data-theme>`. «system» nunca llega al CSS. */
export type ResolvedTheme = Exclude<Theme, "system">;
export type Scheme = "dark" | "light";

export interface ThemeInfo {
  id: Theme;
  label: string;
  /** Esquema de color del tema: decide el aspecto de la barra de título del sistema y los controles nativos. */
  scheme: Scheme | "auto";
}

/** Orden del selector. Los valores de cada tema viven en `styles/themes.css` (misma lista de variables en todos). */
export const THEMES: readonly ThemeInfo[] = [
  { id: "system", label: "Sistema", scheme: "auto" },
  { id: "dark", label: "Oscuro", scheme: "dark" },
  { id: "light", label: "Claro", scheme: "light" },
  { id: "cyberpunk", label: "Cyberpunk", scheme: "dark" },
  { id: "hacker", label: "Hacker / Terminal", scheme: "dark" },
  { id: "minimal", label: "Minimal", scheme: "light" },
];

export const THEME_IDS: readonly Theme[] = THEMES.map((t) => t.id);

/** Valor desconocido (archivo editado a mano, versión futura) → «system». */
export const normalizeTheme = (t: unknown): Theme => (THEME_IDS.includes(t as Theme) ? (t as Theme) : "system");

export function resolveTheme(t: Theme, systemDark: boolean): ResolvedTheme {
  return t === "system" ? (systemDark ? "dark" : "light") : t;
}

/** Esquema claro/oscuro de un tema; `null` para «system» (el sistema decide). */
export function schemeOf(t: Theme): Scheme | null {
  const info = THEMES.find((x) => x.id === t);
  return !info || info.scheme === "auto" ? null : info.scheme;
}
