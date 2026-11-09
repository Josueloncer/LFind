import { fs } from "./fs";
import type { Inspection } from "./fs";

const MAX_CACHED = 64;

export const inspectionKey = (path: string, size: number, modified: number | null | undefined): string => `${path}|${size}|${modified ?? 0}`;

/** Resultados recientes. La clave lleva tamaño y fecha: si el archivo cambia, se vuelve a consultar. */
export class InspectionCache {
  private items = new Map<string, Inspection>();
  private pending = new Map<string, Promise<Inspection>>();

  constructor(private load: (path: string) => Promise<Inspection>, private max = MAX_CACHED) {}

  peek(key: string): Inspection | undefined {
    return this.items.get(key);
  }

  get(key: string, path: string): Promise<Inspection> {
    const hit = this.items.get(key);
    if (hit) return Promise.resolve(hit);
    const going = this.pending.get(key);
    if (going) return going;
    const p = this.load(path).then((r) => {
      this.items.set(key, r);
      if (this.items.size > this.max) this.items.delete(this.items.keys().next().value as string);
      return r;
    }).finally(() => this.pending.delete(key));
    this.pending.set(key, p);
    return p;
  }
}

export const inspections = new InspectionCache((path) => fs.inspectPath(path));
