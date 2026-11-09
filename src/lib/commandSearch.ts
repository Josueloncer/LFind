// Búsqueda de la paleta de comandos. Pura y sin estado: se prepara una vez al abrir y cada tecla solo compara cadenas ya normalizadas.

export interface Command {
  id: string;
  title: string;
  section: string;
  subtitle?: string;
  shortcut?: string;
  /** Otras palabras por las que se puede encontrar. */
  keywords?: string;
  icon?: string;
  run(): void;
}

export interface PreparedCommand {
  command: Command;
  title: string;
  extra: string;
}

export const normalize = (s: string): string => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

export const prepare = (commands: readonly Command[]): PreparedCommand[] =>
  commands.map((command) => ({ command, title: normalize(command.title), extra: normalize(`${command.section} ${command.subtitle ?? ""} ${command.keywords ?? ""}`) }));

/** Puntuación de una palabra buscada en un comando; 0 si no aparece. */
function wordScore(word: string, c: PreparedCommand): number {
  const at = c.title.indexOf(word);
  if (at === 0) return 100;
  if (at > 0) return c.title[at - 1] === " " ? 70 : 40;
  return c.extra.includes(word) ? 15 : 0;
}

export const MAX_RESULTS = 60;

/** Sin texto, primero lo usado hace poco y después en el orden natural. Todas las palabras deben aparecer. */
export function searchCommands(all: readonly PreparedCommand[], query: string, recent: readonly string[] = [], limit = MAX_RESULTS): Command[] {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  if (!words.length) {
    const firsts = recent.flatMap((id) => all.filter((c) => c.command.id === id));
    const rest = all.filter((c) => !recent.includes(c.command.id));
    return [...firsts, ...rest].slice(0, limit).map((c) => c.command);
  }
  const hits: { c: PreparedCommand; score: number }[] = [];
  for (const c of all) {
    let score = 0;
    for (const w of words) {
      const s = wordScore(w, c);
      if (!s) {
        score = 0;
        break;
      }
      score += s;
    }
    if (score) hits.push({ c, score: score - c.title.length / 100 + (recent.includes(c.command.id) ? 5 : 0) });
  }
  hits.sort((a, b) => b.score - a.score);
  return hits.slice(0, limit).map((h) => h.c.command);
}

/** ¿Lo escrito parece una ruta? */
export const looksLikePath = (q: string): boolean => /^(?:[a-zA-Z]:[\\/]?|\\\\)/.test(q.trim());
