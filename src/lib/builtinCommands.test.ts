import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { registerBuiltinCommands } from "./builtinCommands";
import { useNav } from "../stores/nav";
import { allCommands, queryCommands } from "../stores/palette";
import { setPaneActions } from "../stores/paneActions";
import { usePrefs } from "../stores/prefs";
import { DEFAULT_PREFS } from "./fs/types";

const find = (id: string) => {
  const c = allCommands().find((x) => x.id === id);
  if (!c) throw new Error(`falta ${id}`);
  return c;
};
const setView = vi.fn();
const update = vi.fn();

beforeAll(registerBuiltinCommands);
beforeEach(() => {
  setView.mockReset();
  update.mockReset();
  usePrefs.setState({ ...DEFAULT_PREFS, update, favorites: [{ path: "C:\\Fijada", label: "Mi fijada" }] });
  useNav.setState({ tabs: [], activeId: null, other: null, side: 0 });
  useNav.getState().init("C:\\a");
  setPaneActions({ path: "C:\\a", view: { viewMode: "details", iconSize: 96, sortKey: "name", sortDir: "asc", groupBy: "none" }, setView, reload: vi.fn() });
});

describe("comandos propios", () => {
  it("los identificadores no se repiten", () => {
    const ids = allCommands().map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
  it("navega a una ubicación fijada", () => {
    find("go:c:\\fijada").run();
    expect(useNav.getState().tabs[0].path).toBe("C:\\Fijada");
  });
  it("cambia la vista y el orden a través del panel activo", () => {
    find("view:list").run();
    find("sort:size").run();
    expect(setView.mock.calls).toEqual([[{ viewMode: "list" }], [{ sortKey: "size" }]]);
  });
  it("abre pestañas y el doble panel", () => {
    find("tab:new").run();
    expect(useNav.getState().tabs).toHaveLength(2);
    expect(allCommands().some((c) => c.id === "dual:swap")).toBe(false);
    useNav.getState().toggleDual();
    expect(allCommands().some((c) => c.id === "dual:swap")).toBe(true);
  });
  it("cambia preferencias", () => {
    find("pref:hidden").run();
    expect(update).toHaveBeenCalledWith({ showHidden: true });
  });
  it("una ruta escrita se ofrece como destino", () => {
    expect(queryCommands("D:\\fotos")[0].title).toBe("Ir a D:\\fotos");
    expect(queryCommands("fotos")).toHaveLength(0);
  });
});
