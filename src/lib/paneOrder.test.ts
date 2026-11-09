import { describe, expect, it } from "vitest";
import { DEFAULT_PREFS } from "./fs/types";
import { MAX_PANE_ORDERS, applyViewChange, effectiveView, keepsPaneOrder, paneView, withPaneOrder } from "./viewState";
import type { PaneOrder } from "./viewState";

const size: PaneOrder = { sortKey: "size", sortDir: "desc", groupBy: "none" };

describe("orden propio de un panel", () => {
  it("se aplica solo a su carpeta y sin distinguir mayúsculas", () => {
    const v = effectiveView(DEFAULT_PREFS, "C:\\a");
    const o = withPaneOrder(undefined, "C:\\A\\", size);
    expect(paneView(v, o, "c:\\a").sortKey).toBe("size");
    expect(paneView(v, o, "C:\\b")).toBe(v);
  });

  it("solo se usa en la vista doble, si comparten carpeta o si ya lo tenía", () => {
    const o = withPaneOrder(undefined, "C:\\a", size);
    expect(keepsPaneOrder(false, "C:\\a", "C:\\a", o)).toBe(false);
    expect(keepsPaneOrder(true, "C:\\a", "c:\\A\\", undefined)).toBe(true);
    expect(keepsPaneOrder(true, "C:\\a", "C:\\b", undefined)).toBe(false);
    expect(keepsPaneOrder(true, "C:\\a", "C:\\b", o)).toBe(true);
  });

  it("guarda un número limitado de carpetas y conserva las recientes", () => {
    let o = withPaneOrder(undefined, "C:\\primera", size);
    for (let i = 0; i < MAX_PANE_ORDERS + 10; i++) o = withPaneOrder(o, `C:\\d${i}`, size);
    expect(Object.keys(o)).toHaveLength(MAX_PANE_ORDERS);
    expect(o["c:\\primera"]).toBeUndefined();
    expect(o[`c:\\d${MAX_PANE_ORDERS + 9}`]).toEqual(size);
  });

  it("con `folderOnly`, ordenar una carpeta no cambia el orden general del que depende el otro panel", () => {
    const patch = { sortKey: "size", sortDir: "desc" } as const;
    expect(applyViewChange(DEFAULT_PREFS, "C:\\a", patch, true).sortKey).toBeUndefined();
    expect(applyViewChange(DEFAULT_PREFS, "C:\\a", patch).sortKey).toBe("size");
    const next = { ...DEFAULT_PREFS, ...applyViewChange(DEFAULT_PREFS, "C:\\a", patch, true) };
    expect(effectiveView(next, "C:\\a").sortKey).toBe("size");
    expect(effectiveView(next, "C:\\otra").sortKey).toBe(DEFAULT_PREFS.sortKey);
  });
});
