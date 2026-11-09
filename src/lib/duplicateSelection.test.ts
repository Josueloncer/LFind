import { describe, expect, it } from "vitest";
import { DUP_FLAG, canSelect, parseExclusions, selectAllButNewest, selectedBytes, warningsOf } from "./duplicateSelection";
import type { DupGroup } from "./fs";

const m = (path: string, modified = 0, flags = 0) => ({ path, modified, flags });
const group = (...members: DupGroup["members"]): DupGroup => ({ id: 1, size: 10, hash: "h", reclaimable: 10, members });

describe("selección de duplicados", () => {
  const g = group(m("a"), m("b"), m("c"));
  it("nunca deja marcar la última copia sin marcar", () => {
    expect(canSelect(g, new Set(), "a")).toBe(true);
    expect(canSelect(g, new Set(["a"]), "b")).toBe(true);
    expect(canSelect(g, new Set(["a", "b"]), "c")).toBe(false);
    expect(canSelect(g, new Set(["a", "b"]), "a"), "se puede desmarcar").toBe(true);
  });
  it("«todas menos la más reciente» conserva una por grupo y no marca enlaces duros", () => {
    const groups = [group(m("old", 1), m("new", 9), m("mid", 5)), group(m("x", 1), m("y", 2, DUP_FLAG.hardLink), m("z", 3))];
    expect([...selectAllButNewest(groups)].sort()).toEqual(["mid", "old", "x"]);
  });
  it("cuenta el espacio de lo marcado", () => {
    expect(selectedBytes([g], new Set(["a", "b"]))).toBe(20);
  });
  it("avisa de lo delicado", () => {
    expect(warningsOf(DUP_FLAG.hardLink | DUP_FLAG.special)).toHaveLength(2);
    expect(warningsOf(0)).toEqual([]);
  });
  it("limpia las exclusiones escritas a mano", () => {
    expect(parseExclusions(" C:\\a \r\n\r\nnode_modules\nnode_modules\n")).toEqual(["C:\\a", "node_modules"]);
  });
});
