import { describe, expect, it, vi } from "vitest";
import { InspectionCache, inspectionKey } from "./inspection";
import type { Inspection } from "./fs";

const result = (name: string) => ({ name }) as Inspection;

describe("caché del inspector", () => {
  it("no repite la consulta de lo mismo, ni mientras está en marcha", async () => {
    const load = vi.fn(async (p: string) => result(p));
    const c = new InspectionCache(load);
    const k = inspectionKey("a", 1, 2);
    await Promise.all([c.get(k, "a"), c.get(k, "a")]);
    await c.get(k, "a");
    expect(load).toHaveBeenCalledTimes(1);
  });
  it("si el archivo cambia de tamaño o fecha, se consulta de nuevo", async () => {
    const load = vi.fn(async (p: string) => result(p));
    const c = new InspectionCache(load);
    await c.get(inspectionKey("a", 1, 2), "a");
    await c.get(inspectionKey("a", 9, 2), "a");
    await c.get(inspectionKey("a", 1, 3), "a");
    expect(load).toHaveBeenCalledTimes(3);
  });
  it("un fallo no se guarda", async () => {
    const load = vi.fn<(p: string) => Promise<Inspection>>().mockRejectedValueOnce("no").mockResolvedValue(result("a"));
    const c = new InspectionCache(load);
    await expect(c.get("k", "a")).rejects.toBe("no");
    await expect(c.get("k", "a")).resolves.toEqual(result("a"));
  });
  it("tiene un tamaño máximo", async () => {
    const c = new InspectionCache(async (p) => result(p), 2);
    for (const k of ["1", "2", "3"]) await c.get(k, k);
    expect(c.peek("1")).toBeUndefined();
    expect(c.peek("3")).toBeDefined();
  });
});
