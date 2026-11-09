import { describe, expect, it, vi } from "vitest";
import { sortDescription, sortMenu, viewMenu } from "./menus";
import type { MenuItem } from "../stores/ui";

const items = (m: MenuItem[]) => m.filter((i): i is Extract<MenuItem, { kind: "item" }> => i.kind === "item");
const view = { viewMode: "icons" as const, iconSize: 96, sortKey: "name" as const, sortDir: "asc" as const, groupBy: "none" as const };

describe("menú Ver", () => {
  const a = (pinned = false) => ({ setView: vi.fn(), setColumns: vi.fn(), applyAll: vi.fn(), pinned, togglePin: vi.fn() });
  const t = () => ({ thumbnails: true, toggleThumbnails: vi.fn(), systemIcons: true, toggleSystemIcons: vi.fn(), clearThumbCache: vi.fn() });

  it("marca el modo y el tamaño de icono actuales", () => {
    const m = items(viewMenu(view, ["size"], a(), t()));
    expect(m.filter((i) => i.checked).map((i) => i.label)).toEqual(expect.arrayContaining(["Iconos grandes", "Nombre", "Tamaño", "Mostrar miniaturas"]));
    expect(m.find((i) => i.label === "Iconos pequeños")?.checked).toBe(false);
    expect(m.find((i) => i.label === "Detalles")?.checked).toBe(false);
  });

  it("cada opción llama a la acción correspondiente", () => {
    const act = a();
    const th = t();
    const m = items(viewMenu(view, ["size"], act, th));
    m.find((i) => i.label === "Iconos extragrandes")!.run();
    expect(act.setView).toHaveBeenCalledWith({ viewMode: "icons", iconSize: 256 });
    m.find((i) => i.label === "Mosaicos")!.run();
    expect(act.setView).toHaveBeenCalledWith({ viewMode: "tiles" });
    m.find((i) => i.label === "Fecha de creación")!.run();
    expect(act.setColumns).toHaveBeenCalledWith(["size", "created"]);
    m.find((i) => i.label === "Tamaño")!.run();
    expect(act.setColumns).toHaveBeenCalledWith([]);
    m.find((i) => i.label === "Mostrar miniaturas")!.run();
    m.find((i) => i.label === "Borrar la caché de miniaturas e iconos")!.run();
    expect(th.toggleThumbnails).toHaveBeenCalled();
    expect(th.clearThumbCache).toHaveBeenCalled();
  });

  it("ofrece recordar la vista solo para la carpeta (acción explícita) y dejar de hacerlo", () => {
    const act = a();
    items(viewMenu(view, [], act, t())).find((i) => i.label === "Recordar esta vista solo para esta carpeta")!.run();
    expect(act.togglePin).toHaveBeenCalledTimes(1);
    const pinned = a(true);
    const m = items(viewMenu(view, [], pinned, t()));
    expect(m.find((i) => i.label === "Recordar esta vista solo para esta carpeta")).toBeUndefined();
    m.find((i) => i.label === "Dejar de recordar la vista de esta carpeta")!.run();
    expect(pinned.togglePin).toHaveBeenCalledTimes(1);
  });

  it("el menú Ordenar permite agrupar y cambiar el sentido", () => {
    const setView = vi.fn();
    const m = items(sortMenu({ ...view, sortKey: "created", groupBy: "type" }, { setView }));
    expect(m.filter((i) => i.checked).map((i) => i.label)).toEqual(expect.arrayContaining(["Fecha de creación", "Ascendente", "Tipo"]));
    m.find((i) => i.label === "Descendente")!.run();
    expect(setView).toHaveBeenCalledWith({ sortDir: "desc" });
    m.find((i) => i.label === "(Ninguno)")!.run();
    expect(setView).toHaveBeenCalledWith({ groupBy: "none" });
  });
});

describe("menú de ordenar", () => {
  const view = { viewMode: "details", iconSize: 32, sortKey: "modified", sortDir: "desc", groupBy: "none" } as const;
  const find = (items: ReturnType<typeof sortMenu>, label: string) => items.find((i) => i.kind === "item" && i.label === label) as Extract<ReturnType<typeof sortMenu>[number], { kind: "item" }>;
  it("repetir el criterio invierte el sentido y la flecha lo indica", () => {
    let patch: unknown = null;
    const item = find(sortMenu(view, { setView: (p) => (patch = p) }), "Fecha de modificación");
    expect(item.shortcut).toBe("↓");
    item.run?.();
    expect(patch).toEqual({ sortKey: "modified", sortDir: "asc" });
  });
  it("describe qué queda arriba", () => {
    expect(sortDescription(view)).toBe("Fecha de modificación: más reciente primero");
    expect(sortDescription({ sortKey: "name", sortDir: "desc" })).toBe("Nombre: de la Z a la A");
  });
});
