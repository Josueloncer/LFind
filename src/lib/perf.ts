// Marcas de arranque, solo en desarrollo (`npm run tauri dev`). En producción son no-ops.
// Se imprimen una vez, al terminar de leer la primera carpeta, en la consola de DevTools (F12).
const DEV = import.meta.env.DEV;
const seen = new Set<string>();

export function mark(name: string) {
  if (DEV) performance.mark(`nova:${name}`);
}

export function markOnce(name: string) {
  if (DEV && !seen.has(name)) {
    seen.add(name);
    performance.mark(`nova:${name}`);
  }
}

export function reportOnce(extra?: Record<string, unknown>) {
  if (!DEV || seen.has("__report")) return;
  seen.add("__report");
  const rows = [
    ...performance.getEntriesByType("paint").map((p) => ({ evento: p.name, ms: Math.round(p.startTime) })),
    ...performance.getEntriesByType("mark").filter((m) => m.name.startsWith("nova:")).map((m) => ({ evento: m.name.slice(5), ms: Math.round(m.startTime) })),
  ].sort((a, b) => a.ms - b.ms);
  console.info("[nova] arranque (ms desde que empezó la navegación del webview)", extra ?? "");
  console.table(rows);
}
