// Lógica pura de la recuperación tras un error de renderizado (sin React ni IPC): `errorRecovery.test.ts`.

/** Fallos dentro de esta ventana de tiempo cuentan como un error que se repite. */
export const FAILURE_WINDOW_MS = 30_000;
/** A partir de estos fallos seguidos, reintentar ya no es seguro: solo volvería a fallar. */
export const MAX_RECENT_FAILURES = 3;
/** Longitud máxima de lo que se envía al registro. */
export const MAX_REPORT_CHARS = 4000;

/** Historial de fallos con el nuevo incluido y sin los que ya quedaron fuera de la ventana. */
export function registerFailure(history: readonly number[], now: number): number[] {
  return [...history.filter((t) => now - t < FAILURE_WINDOW_MS), now];
}

export const canRetry = (history: readonly number[]): boolean => history.length < MAX_RECENT_FAILURES;

const clip = (s: string): string => (s.length > MAX_REPORT_CHARS ? `${s.slice(0, MAX_REPORT_CHARS)}…` : s);

/** Datos para el registro local. Nunca se muestran al usuario. */
export function errorReport(error: unknown, componentStack?: string | null): { message: string; stack: string | null } {
  const e = error instanceof Error ? error : null;
  let message = e ? `${e.name}: ${e.message}` : "";
  if (!e) {
    try {
      message = String(error);
    } catch {
      message = "Error no representable";
    }
  }
  const stack = [e?.stack, componentStack].filter(Boolean).join("\n--- componentes ---\n");
  return { message: clip(message), stack: stack ? clip(stack) : null };
}
