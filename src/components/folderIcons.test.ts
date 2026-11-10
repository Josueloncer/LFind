import { describe, expect, it, vi } from "vitest";
import { FOLDER_ICONS, FOLDER_ICON_CATEGORIES, folderIcon, searchFolderIcons } from "./folderIcons";
import { placeMenu } from "../lib/contextMenu";

describe("biblioteca de iconos de carpetas fijadas", () => {
  it("cada icono de las categorías existe, una sola vez, y hay de sobra para distinguir carpetas", () => {
    const all = FOLDER_ICON_CATEGORIES.flatMap((c) => c.icons);
    expect(new Set(all).size).toBe(all.length);
    for (const id of all) expect(FOLDER_ICONS[id], id).toBeDefined();
    expect(Object.keys(FOLDER_ICONS).length).toBe(all.length);
    expect(all.length).toBeGreaterThanOrEqual(100);
    expect(FOLDER_ICON_CATEGORIES.map((c) => c.label)).toEqual(expect.arrayContaining(["Trabajo", "Proyectos", "Documentos", "Descargas", "Imágenes", "Vídeos", "Música", "Desarrollo", "Juegos", "Personal", "Archivo"]));
  });
  it("busca por nombre en español sin tildes, por categoría y dentro de una categoría", () => {
    expect(searchFolderIcons("codigo")).toContain("code");
    expect(searchFolderIcons("MÚSICA")).toEqual(expect.arrayContaining(FOLDER_ICON_CATEGORIES.find((c) => c.id === "musica")!.icons));
    expect(searchFolderIcons("", "juegos")).toEqual(FOLDER_ICON_CATEGORIES.find((c) => c.id === "juegos")!.icons);
    expect(searchFolderIcons("cámara", "musica")).toEqual([]);
    expect(searchFolderIcons("zzzz")).toEqual([]);
  });
  it("un id desconocido (de una versión futura o borrado) vuelve al icono predeterminado", () => {
    expect(folderIcon("no-existe")).toBeUndefined();
    expect(folderIcon(undefined)).toBeUndefined();
    expect(folderIcon("rocket")).toBe(FOLDER_ICONS.rocket.Icon);
  });
  it("«Cambiar icono…» solo aparece en las carpetas fijadas", () => {
    const base = { open: vi.fn(), openInNewTab: vi.fn(), pinned: true, togglePin: vi.fn(), copyPath: vi.fn(), properties: vi.fn() };
    const changeIcon = vi.fn();
    const m = placeMenu({ ...base, changeIcon });
    const item = m.find((i) => i.kind === "item" && i.label === "Cambiar icono…");
    expect(item).toBeDefined();
    (item as { run(): void }).run();
    expect(changeIcon).toHaveBeenCalled();
    expect(placeMenu(base).some((i) => i.kind === "item" && i.label === "Cambiar icono…")).toBe(false);
  });
});
