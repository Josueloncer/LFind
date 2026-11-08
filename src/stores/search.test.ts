import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_FILTERS } from "../lib/searchQuery";
import { useSearch } from "./search";

const src = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8");

beforeEach(() => {
  useSearch.setState({ scope: "folder", text: "", filters: DEFAULT_FILTERS, focusTick: 0, selected: null });
});

describe("abrir la búsqueda global (Update 05.1)", () => {
  it("openGlobal cambia al ámbito global y pide el foco del campo", () => {
    useSearch.getState().openGlobal();
    expect(useSearch.getState().scope).toBe("global");
    expect(useSearch.getState().focusTick).toBe(1);
  });

  it("si ya está abierta, vuelve a pedir el foco sin perder texto ni filtros", () => {
    useSearch.getState().openGlobal();
    useSearch.getState().setText("informe");
    useSearch.getState().patchFilters({ exts: "pdf" });
    useSearch.getState().openGlobal();
    const s = useSearch.getState();
    expect(s.scope).toBe("global");
    expect(s.text).toBe("informe");
    expect(s.filters.exts).toBe("pdf");
    expect(s.focusTick).toBe(2);
  });

  it("salir devuelve a la navegación normal (ámbito carpeta, sin texto) y se puede volver a abrir", () => {
    useSearch.getState().openGlobal();
    useSearch.getState().setText("foto");
    useSearch.getState().leave();
    expect(useSearch.getState().scope).toBe("folder");
    expect(useSearch.getState().text).toBe("");
    useSearch.getState().openGlobal();
    expect(useSearch.getState().scope).toBe("global");
  });

  it("botón, atajo e icono del campo usan la MISMA acción (no hay una segunda implementación)", () => {
    const toolbar = src("../components/Toolbar.tsx");
    const app = src("../App.tsx");
    expect(toolbar).toContain('className="tbtn tb-global"');
    expect(toolbar).toContain("onClick={openGlobal}");
    expect(toolbar).toContain(": openGlobal())"); // icono de globo del campo
    expect(app).toContain("useSearch.getState().openGlobal()");
    expect(app).not.toContain('setScope("global")');
    // el campo recibe el foco al cambiar focusTick
    expect(toolbar).toContain("[focusTick]");
    expect(toolbar).toContain('getElementById("nova-filter")');
  });

  it("el botón es permanente: no depende de tb-hide-narrow y solo oculta el texto, nunca el icono", () => {
    const toolbar = src("../components/Toolbar.tsx");
    const line = toolbar.split("\n").find((l) => l.includes('tbtn tb-global')) ?? "";
    expect(line).not.toContain("tb-hide-narrow");
    const css = src("../styles/app.css");
    expect(css).toContain(".tb-global .tb-label { display: none; }");
    expect(css).not.toMatch(/\.tb-global\s*\{[^}]*display:\s*none/);
  });
});
