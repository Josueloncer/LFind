// Utilidades de ruta que entienden Windows (C:\, UNC) y rutas POSIX.

const isWin = (p: string) => /^[A-Za-z]:/.test(p) || p.startsWith("\\\\") || p.includes("\\");
const sepOf = (p: string) => (isWin(p) ? "\\" : "/");

/** Ruta virtual de la vista «Mi equipo»: se navega a ella como a una carpeta, pero no existe en disco. */
export const COMPUTER = "lfind://computer";

/** Analizador de almacenamiento. Con informe abierto: `lfind://storage?root=…&at=carpeta/subcarpeta`. */
export const STORAGE = "lfind://storage";

/** Papelera de reciclaje de Windows. */
export const RECYCLE = "lfind://recycle";

/** Detector de archivos duplicados. */
export const DUPLICATES = "lfind://duplicates";

export const isVirtual = (p: string): boolean => p.startsWith("lfind://");
export const isStorage = (p: string): boolean => p === STORAGE || p.startsWith(STORAGE + "?");

export const storageRoute = (root?: string | null, at = ""): string =>
  root ? `${STORAGE}?root=${encodeURIComponent(root)}${at ? `&at=${encodeURIComponent(at)}` : ""}` : STORAGE;

export function parseStorageRoute(p: string): { root: string | null; at: string } {
  const q = new URLSearchParams(p.slice(STORAGE.length + 1));
  return { root: q.get("root"), at: q.get("at") ?? "" };
}

/** Ruta de `path` respecto a `root`, con `/`. Cadena vacía si son la misma; sin distinguir mayúsculas ni tipo de barra. */
export function relativeTo(root: string, path: string): string {
  const clean = (x: string) => x.replace(/\\/g, "/").replace(/\/+$/, "");
  const a = clean(root);
  const b = clean(path);
  return b.toLowerCase().startsWith(a.toLowerCase()) ? b.slice(a.length).replace(/^\/+/, "") : b;
}

function storageUp(p: string): string | null {
  if (p === STORAGE) return null;
  const { root, at } = parseStorageRoute(p);
  if (!at) return STORAGE;
  const i = at.lastIndexOf("/");
  return storageRoute(root, i < 0 ? "" : at.slice(0, i));
}

export const isDriveRoot = (p: string): boolean => /^[A-Za-z]:[\\/]*$/.test(p);

/** Clave de comparación: sin distinguir mayúsculas, tipo de barra ni barra final. */
export function canonicalKey(p: string): string {
  if (!isWin(p)) return p.replace(/\/+$/, "") || p;
  return p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
}

/** A dónde lleva «subir»: la carpeta contenedora o, desde la raíz de una unidad, Mi equipo. */
export const upOf = (p: string): string | null => (isStorage(p) ? storageUp(p) : p === COMPUTER ? null : parent(p) ?? (isDriveRoot(p) ? COMPUTER : null));

export function join(base: string, name: string): string {
  const s = sepOf(base);
  return base.endsWith(s) ? base + name : base + s + name;
}

/** Carpeta contenedora, o `null` si ya estamos en la raíz. */
export function parent(p: string): string | null {
  if (isVirtual(p)) return null;
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
  if (isStorage(p)) {
    const { root, at } = parseStorageRoute(p);
    return at ? at.slice(at.lastIndexOf("/") + 1) : root ? `Análisis de ${basename(root)}` : "Almacenamiento";
  }
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
