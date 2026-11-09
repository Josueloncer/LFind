import { beforeEach, describe, expect, it, vi } from "vitest";
import { toggleDualPane } from "./dual";
import { useNav } from "./nav";
import { usePrefs } from "./prefs";

const setView = vi.fn();
const update = vi.fn();
const size = { sortKey: "size", sortDir: "desc", groupBy: "none" } as const;

beforeEach(() => {
  setView.mockReset();
  update.mockReset();
  usePrefs.setState({ setView, update });
  useNav.setState({ tabs: [], activeId: null, other: null, side: 0 });
  useNav.getState().init("C:\a");
  useNav.getState().toggleDual();
});

describe("volver a un panel", () => {
  it("la carpeta que queda a la vista conserva el orden que tenía", () => {
    useNav.getState().setPaneOrder(0, "C:\a", size);
    toggleDualPane();
    expect(useNav.getState().other).toBeNull();
    expect(setView).toHaveBeenCalledWith("C:\a", size);
  });

  it("sin orden propio no toca las preferencias de la carpeta", () => {
    toggleDualPane();
    expect(setView).not.toHaveBeenCalled();
  });
});
