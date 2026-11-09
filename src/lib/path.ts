// Utilidades de ruta que entienden Windows (C:\, UNC) y rutas POSIX.

const isWin = (p: string) => /^[A-Za-z]:/.test(p) || p.startsWith("\\\\") || p.includes("\\");
const sepOf = (p: string) => (isWin(p) ? "\\" : "/");

/** Ruta virtual de la vista «Mi equipo»: se navega a ella como a una carpeta, pero no existe en disco. */
export const COMPUTER = "lfind://computer";

export const isDriveRoot = (p: string): boolean => /^[A-Za-z]:[\\/]*$/.test(p);

/** Clave de comparación: sin distinguir mayúsculas, tipo de barra ni barra final. */
export function canonicalKey(p: string): string {
  if (!isWin(p)) return p.replace(/\/+$/, "") || p;
  return p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
}

/** A dónde lleva «subir»: la carpeta contenedora o, desde la raíz de una unidad, Mi equipo. */
export const upOf = (p: string): string | null => (p === COMPUTER ? null : parent(p) ?? (isDriveRoot(p) ? COMPUTER : null));

export function join(base: string, name: string): string {
  const s = sepOf(base);
  return base.endsWith(s) ? base + name : base + s + name;
}

/** Carpeta contenedora, o `null` si ya estamos en la raíz. */
export function parent(p: string): string | null {
  if (p === COMPUTER) return null;
  if (sepOf(p) === "\\") {
    const t = p.replace(/\\+$/, "");
    if (/^[A-Za-z]:$/.test(t)) return null;
    const i = t.lastIndexOf("\\");
    if (i <= 0) return null;
    const up = t.slice(0, i);
    if (/^[A-Za-z]:$/.test(up)) return up + "\\";
    if (/^\\\\[^\\]*$/.test(up)) return null; // UNC: no subir de \\servidor
    return up;
  }
  if (p === "/" || p === "") return null;
  const t = p.replace(/\/+$/, "");
  const i = t.lastIndexOf("/");
  return i <= 0 ? "/" : t.slice(0, i);
}

export function basename(p: string): string {
  if (p === COMPUTER) return "Mi equipo";
  const t = p.replace(/[\\/]+$/, "");
  if (/^[A-Za-z]:$/.test(t)) return t;
  if (t === "") return "/";
  return t.slice(Math.max(t.lastIndexOf("\\"), t.lastIndexOf("/")) + 1);
}

export interface Segment {
  label: string;
  path: string;
}

/** Migas de pan: cada segmento con la ruta completa a la que lleva. */
export function segments(p: string): Segment[] {
  if (sepOf(p) === "\\") {
    const parts = p.split("\\").filter(Boolean);
    const out: Segment[] = [];
    let acc = "";
    parts.forEach((part, i) => {
      acc = i === 0 ? (/^[A-Za-z]:$/.test(part) ? part + "\\" : "\\\\" + part) : join(acc, part);
      out.push({ label: part, path: acc });
    });
    return out;
  }
  const parts = p.split("/").filter(Boolean);
  const out: Segment[] = [{ label: "/", path: "/" }];
  let acc = "";
  for (const part of parts) {
    acc += "/" + part;
    out.push({ label: part, path: acc });
  }
  return out;
}

export const samePath = (a: string, b: string): boolean => canonicalKey(a) === canonicalKey(b);

/** Raíz de la unidad (`c:`, `\\\\srv\\share`, `/`), en minúsculas. Sirve para saber si dos rutas están en la misma unidad. */
export function rootOf(p: string): string {
  const m = /^([A-Za-z]:)/.exec(p);
  if (m) return m[1].toLowerCase();
  const u = /^\\\\[^\\]+\\[^\\]+/.exec(p);
  if (u) return u[0].toLowerCase();
  return "/";
}

/** ¿`child` es `parent` o está dentro de él? */
export function isInside(parentPath: string, child: string): boolean {
  const norm = (x: string) => x.replace(/[\\/]+$/, "").replace(/\//g, "\\").toLowerCase();
  const a = norm(parentPath);
  const b = norm(child);
  return b === a || b.startsWith(a + "\\");
}
