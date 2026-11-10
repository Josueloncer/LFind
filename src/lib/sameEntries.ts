import type { Entry } from "./fs";

/** ¿Las dos lecturas de la carpeta dicen exactamente lo mismo? */
export function sameEntries(a: readonly Entry[], b: readonly Entry[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    const x = a[i] as unknown as Record<string, unknown>;
    const y = b[i] as unknown as Record<string, unknown>;
    const keys = Object.keys(x);
    if (keys.length !== Object.keys(y).length || keys.some((k) => x[k] !== y[k])) return false;
  }
  return true;
}
