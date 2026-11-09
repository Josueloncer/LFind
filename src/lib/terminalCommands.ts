import type { TerminalShell } from "./fs";

/** Orden para cambiar de carpeta, o `null` si esa ruta no se puede escribir con seguridad en ese shell. */
export function cdCommand(shell: TerminalShell, path: string): string | null {
  if (/[\r\n\0]/.test(path)) return null;
  if (shell === "powershell") return `Set-Location -LiteralPath '${path.replace(/'/g, "''")}'\r`;
  // En CMD, `%` se expandiría como variable y `"` rompería el entrecomillado.
  if (/[%"^!]/.test(path)) return null;
  return `cd /d "${path}"\r`;
}

/** Carpeta que anuncia el shell con OSC 9;9, o `null` si es otra secuencia. */
export function parseCwdReport(data: string): string | null {
  if (!data.startsWith("9;")) return null;
  const path = data.slice(2).replace(/^"|"$/g, "").trim();
  return path || null;
}

export interface InputState {
  dirty: boolean;
}

/** Sigue si hay una línea a medio escribir: en ese caso no se debe inyectar nada. */
export function trackInput(state: InputState, data: string): void {
  if (data.includes("\r") || data.includes("\x03")) state.dirty = false;
  else if (data !== "\x7f" && !data.startsWith("\x1b")) state.dirty = true;
}

/** Colores de la terminal a partir de los del tema. */
export function terminalTheme(read: (name: string) => string) {
  return { background: read("--bg-content"), foreground: read("--text"), cursor: read("--accent"), cursorAccent: read("--bg-content"), selectionBackground: read("--sel") };
}
