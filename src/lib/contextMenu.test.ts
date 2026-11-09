import { describe, expect, it, vi } from "vitest";
import { REFRESH_DELAYS_MS, buildContextMenu, isArchive, scheduleRefresh, tidy } from "./contextMenu";
import type { MenuActions, MenuState } from "./contextMenu";
import type { Entry } from "./fs/types";
import type { MenuItem } from "../stores/ui";

const file = (name: string): Entry => ({ name, kind: "file", size: 1, modified: 0, hidden: false } as unknown as Entry);
const dir = (name: string): Entry => ({ name, kind: "dir", size: 0, modified: 0, hidden: false } as unknown as Entry);
const view = { viewMode: "icons" as const, iconSize: 96, sortKey: "name" as const, sortDir: "asc" as const, groupBy: "none" as const };
const noop = () => {};
const actions = (): MenuActions => {
  const store: Record<string | symbol, unknown> = {};
  return new Proxy({} as MenuActions, { get: (_t, k) => (store[k] ??= k === "isPinned" ? () => false : vi.fn()) });
};
const state = (list: Entry[], extra: Partial<MenuState> = {}): MenuState => ({ path: "C:\\x", list, canPaste: true, view, sendTo: [], templates: [], ...extra });
const labels = (m: MenuItem[]) => m.flatMap((i) => (i.kind === "item" || i.kind === "submenu" ? [i.label] : []));
type Item = Extract<MenuItem, { kind: "item" }>;
const find = (m: MenuItem[], l: string) => m.find((i) => (i.kind === "item" || i.kind === "submenu") && i.label === l) as Item;

describe("menú contextual propio", () => {
  it("archivo: abrir, abrir con, cortar/copiar, comprimir, renombrar, eliminar…", () => {
    const l = labels(buildContextMenu(state([file("a.txt")]), actions()));
    expect(l).toEqual(expect.arrayContaining(["Abrir", "Abrir con…", "Cortar", "Copiar", "Comprimir en .zip", "Cambiar nombre", "Eliminar", "Propiedades"]));
    expect(l).not.toContain("Extraer aquí");
    expect(l).not.toContain("Pegar en la carpeta");
  });
  it("carpeta: pestaña nueva, terminal, pegar dentro, anclar", () => {
    const l = labels(buildContextMenu(state([dir("d")]), actions()));
    expect(l).toEqual(expect.arrayContaining(["Abrir en una pestaña nueva", "Abrir terminal aquí", "Pegar en la carpeta", "Fijar en Ubicaciones"]));
    expect(l).not.toContain("Abrir con…");
  });
  it("archivo comprimido: ofrece extraer", () => {
    expect(isArchive("x.ZIP")).toBe(true);
    expect(isArchive("x.txt")).toBe(false);
    expect(labels(buildContextMenu(state([file("x.zip")]), actions()))).toContain("Extraer aquí");
  });
  it("selección múltiple: sin Abrir/Cambiar nombre; las acciones reciben toda la selección", () => {
    const a = actions();
    const sel = [file("a"), dir("b"), file("c")];
    const m = buildContextMenu(state(sel), a);
    const l = labels(m);
    expect(l).not.toContain("Abrir");
    expect(l).not.toContain("Cambiar nombre");
    expect(l).toContain("Copiar rutas");
    find(m, "Copiar").run();
    expect(a.copy).toHaveBeenCalledWith(sel);
    find(m, "Eliminar permanentemente").run();
    expect(a.remove).toHaveBeenCalledWith(sel, true);
    find(m, "Comprimir en .zip").run();
    expect(a.compress).toHaveBeenCalledWith(sel);
  });
  it("«Enviar a» solo aparece si hay destinos y pasa el id (nunca una ruta)", () => {
    expect(labels(buildContextMenu(state([file("a")]), actions()))).not.toContain("Enviar a");
    const a = actions();
    const t = { id: "3", label: "Escritorio", kind: "app" as const };
    const sub = find(buildContextMenu(state([file("a")], { sendTo: [t] }), a), "Enviar a") as unknown as Extract<MenuItem, { kind: "submenu" }>;
    expect(sub.kind).toBe("submenu");
    const first = sub.items[0] as Item;
    first.run();
    expect(a.sendTo).toHaveBeenCalledWith([file("a")], t);
  });
  it("«Pegar» refleja el estado del portapapeles", () => {
    expect(find(buildContextMenu(state([]), actions()), "Pegar").disabled).toBe(false);
    expect(find(buildContextMenu(state([], { canPaste: false }), actions()), "Pegar").disabled).toBe(true);
    expect(find(buildContextMenu(state([dir("d")], { canPaste: false }), actions()), "Pegar en la carpeta").disabled).toBe(true);
  });
  it("espacio vacío: Nuevo (con plantillas), Ordenar, Ver, Actualizar…", () => {
    const m = buildContextMenu(state([], { templates: [{ id: "t1", label: "Documento de Word" }] }), actions());
    expect(labels(m)).toEqual(expect.arrayContaining(["Nuevo", "Pegar", "Deshacer", "Ordenar y agrupar", "Ver", "Actualizar", "Propiedades de la carpeta"]));
    const nuevo = find(m, "Nuevo") as unknown as Extract<MenuItem, { kind: "submenu" }>;
    expect(labels(nuevo.items)).toEqual(expect.arrayContaining(["Carpeta", "Documento de Word"]));
  });
  it("sin separadores duplicados, iniciales ni finales", () => {
    const sep: MenuItem = { kind: "sep" };
    const it: MenuItem = { kind: "item", label: "x", run: noop };
    expect(tidy([sep, it, sep, sep, it, sep])).toEqual([it, sep, it]);
    for (const s of [state([]), state([file("a")]), state([dir("a"), file("b")])]) {
      const m = buildContextMenu(s, actions());
      expect(m[0].kind).not.toBe("sep");
      expect(m[m.length - 1].kind).not.toBe("sep");
      m.forEach((x, i) => i && expect(x.kind === "sep" && m[i - 1].kind === "sep").toBe(false));
    }
  });
  it("construir el menú con 5000 elementos seleccionados es instantáneo (< 5 ms de media)", () => {
    const big = Array.from({ length: 5000 }, (_, i) => file(`f${i}.txt`));
    const a = actions();
    const t0 = performance.now();
    for (let i = 0; i < 200; i++) buildContextMenu(state(big), a);
    const avg = (performance.now() - t0) / 200;
    console.log(`buildContextMenu(5000 elementos): ${avg.toFixed(4)} ms de media`);
    expect(avg).toBeLessThan(5);
  });
  it("scheduleRefresh recarga a 0,4/1,5/4 s y se puede cancelar", () => {
    const calls: number[] = [];
    const cancelled: unknown[] = [];
    const reload = vi.fn();
    const cancel = scheduleRefresh(reload, (_f, ms) => (calls.push(ms), ms), (h) => cancelled.push(h));
    expect(calls).toEqual([...REFRESH_DELAYS_MS]);
    cancel();
    expect(cancelled).toEqual([...REFRESH_DELAYS_MS]);
  });
});
