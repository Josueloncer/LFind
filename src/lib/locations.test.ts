import { insertionFromY } from "./locations";
import { describe, expect, it } from "vitest";
import type { DriveUsage, Place } from "./fs/types";
import { MAX_CLOUD_QUERY, MAX_RECENTS, PASS_THROUGH_MS, applyCloudResults, canMovePin, cloudCandidates, driveKindLabel, dropMissingRecents, groupDrives, isPinned, movePin, recordVisit, removeRecent, renamePin, setPinIcon, togglePin, usedPercent, visibleRecents } from "./locations";
import { COMPUTER, basename, canonicalKey, isDriveRoot, parent, samePath, upOf } from "./path";
import { crumbsFor } from "./addressBar";

const T = 1_000_000_000;
const paths = (l: readonly { path: string }[]) => l.map((x) => x.path);

describe("rutas equivalentes", () => {
  it("no distinguen mayúsculas, barras ni barra final", () => {
    expect(canonicalKey("C:/Users/Ana/")).toBe(canonicalKey("c:\\users\\ANA"));
    expect(samePath("C:/A/b", "c:\\a\\B\\")).toBe(true);
    expect(samePath("C:\\a", "C:\\ab")).toBe(false);
    expect(canonicalKey("C:\\")).toBe("c:");
  });
  it("Mi equipo es el nivel superior de las unidades", () => {
    expect(upOf("C:\\")).toBe(COMPUTER);
    expect(upOf("C:\\Users")).toBe("C:\\");
    expect(upOf(COMPUTER)).toBeNull();
    expect(parent(COMPUTER)).toBeNull();
    expect(basename(COMPUTER)).toBe("Mi equipo");
    expect(isDriveRoot("D:\\")).toBe(true);
    expect(isDriveRoot("D:\\x")).toBe(false);
    expect(crumbsFor(COMPUTER)).toEqual([{ label: "Mi equipo", path: COMPUTER }]);
  });
});

describe("historial", () => {
  it("una carpeta aparece una vez y volver a ella actualiza su fecha", () => {
    let r = recordVisit([], "C:\\a", T);
    r = recordVisit(r, "C:\\b", T + PASS_THROUGH_MS + 1);
    r = recordVisit(r, "c:/A/", T + 2 * PASS_THROUGH_MS);
    expect(paths(r)).toEqual(["c:/A/", "C:\\b"]);
    expect(r[0].at).toBe(T + 2 * PASS_THROUGH_MS);
  });
  it("más reciente primero y con límite", () => {
    let r: readonly { path: string; at: number }[] = [];
    for (let i = 0; i < 40; i++) r = recordVisit(r, `C:\\x${i}`, T + i * (PASS_THROUGH_MS + 1));
    expect(r).toHaveLength(MAX_RECENTS);
    expect(r[0].path).toBe("C:\\x39");
  });
  it("revisitar enseguida no provoca escrituras", () => {
    const r = recordVisit([], "C:\\a", T);
    expect(recordVisit(r, "c:\\A", T + 1000)).toBe(r);
  });
  it("las carpetas de paso al profundizar no ensucian la lista", () => {
    let r = recordVisit([], "C:\\Users", T);
    r = recordVisit(r, "C:\\Users\\Ana", T + 1000);
    r = recordVisit(r, "C:\\Users\\Ana\\Fotos", T + 2000);
    expect(paths(r)).toEqual(["C:\\Users\\Ana\\Fotos"]);
    // Pasado el margen, la carpeta superior fue una visita real.
    const later = recordVisit(recordVisit([], "C:\\Users", T), "C:\\Users\\Ana", T + PASS_THROUGH_MS + 1);
    expect(paths(later)).toEqual(["C:\\Users\\Ana", "C:\\Users"]);
  });
  it("una entrada migrada sin fecha no se considera de paso", () => {
    expect(paths(recordVisit([{ path: "C:\\A", at: 0 }], "C:\\A\\b", T))).toEqual(["C:\\A\\b", "C:\\A"]);
  });
  it("las raíces de unidad no se registran: ya están en Mi equipo", () => {
    const r: readonly { path: string; at: number }[] = [];
    expect(recordVisit(r, "D:\\", T)).toBe(r);
  });
  it("se puede quitar y no repite lo que ya se ve en otra sección", () => {
    const r = [{ path: "C:\\a", at: 1 }, { path: "C:\\b", at: 2 }];
    expect(paths(removeRecent(r, "c:/A"))).toEqual(["C:\\b"]);
    expect(paths(visibleRecents(r, ["c:\\B\\"]))).toEqual(["C:\\a"]);
  });
});

describe("accesos fijados", () => {
  const pins = [{ path: "C:\\a", label: "A" }, { path: "C:\\b", label: "B" }, { path: "C:\\c", label: "C" }];
  it("fija y quita sin duplicar rutas equivalentes", () => {
    expect(togglePin([], "C:\\Fotos\\")[0]).toEqual({ path: "C:\\Fotos\\", label: "Fotos" });
    expect(togglePin(pins, "c:/A/").map((p) => p.label)).toEqual(["B", "C"]);
    expect(isPinned(pins, "C:/B")).toBe(true);
  });
  it("renombrar cambia solo el nombre visible", () => {
    expect(renamePin(pins, "c:\\b", "  Trabajo ")[1]).toEqual({ path: "C:\\b", label: "Trabajo" });
    expect(renamePin(pins, "C:\\b", "   ")).toBe(pins);
    expect(renamePin(pins, "C:\\no", "x")).toBe(pins);
  });
  it("icono propio: se asigna, se cambia y se restaura sin tocar ruta ni nombre", () => {
    const a = setPinIcon(pins, "c:/B/", "rocket");
    expect(a[1]).toEqual({ path: "C:\\b", label: "B", icon: "rocket" });
    expect(a[0]).toBe(pins[0]);
    expect(setPinIcon(a, "C:\\b", "rocket")).toBe(a);
    expect(setPinIcon(a, "C:\\b", "music")[1].icon).toBe("music");
    expect(setPinIcon(a, "C:\\b", null)[1]).toEqual({ path: "C:\\b", label: "B" });
    expect(setPinIcon(pins, "C:\\no", "rocket")).toBe(pins);
    expect(renamePin(a, "C:\\b", "Trabajo")[1].icon).toBe("rocket");
    expect(movePin(a, 1, 0)[0].icon).toBe("rocket");
  });
  it("reordena", () => {
    expect(movePin(pins, 0, 3).map((p) => p.label)).toEqual(["B", "C", "A"]);
    expect(movePin(pins, 2, 0).map((p) => p.label)).toEqual(["C", "A", "B"]);
    expect(movePin(pins, 1, 1)).toBe(pins);
    expect(movePin(pins, 1, 2)).toBe(pins);
    expect(movePin(pins, 9, 0)).toBe(pins);
  });
});

describe("posiciones de arrastre", () => {
  it("solo acepta las que cambian algo y están dentro de la lista", () => {
    expect(canMovePin(3, 0, 2)).toBe(true);
    expect(canMovePin(3, 0, 3)).toBe(true);
    expect(canMovePin(3, 2, 0)).toBe(true);
    for (const [from, before] of [[1, 1], [1, 2], [0, 4], [0, -1], [-1, 0], [3, 0], [NaN, 0], [0, 1.5]]) expect(canMovePin(3, from, before)).toBe(false);
  });
});

describe("insignias de nube en la búsqueda", () => {
  const roots = ["C:\\Users\\Ana\\OneDrive"];
  const inCloud = "C:\\Users\\Ana\\OneDrive\\a.png";
  it("fuera de OneDrive no se consulta nada", () => {
    expect(cloudCandidates(["D:\\x.png", "C:\\Users\\Ana\\OneDriveNo\\y.png"], roots, new Map(), T, 1000)).toEqual([]);
    expect(cloudCandidates([inCloud], [], new Map(), T, 1000)).toEqual([]);
  });
  it("no repite lo consultado hace poco y respeta el máximo", () => {
    expect(cloudCandidates([inCloud], roots, new Map([[inCloud, T - 500]]), T, 1000)).toEqual([]);
    expect(cloudCandidates([inCloud], roots, new Map([[inCloud, T - 1000]]), T, 1000)).toEqual([inCloud]);
    const many = Array.from({ length: 300 }, (_, i) => `C:\\Users\\Ana\\OneDrive\\${i}`);
    expect(cloudCandidates(many, roots, new Map(), T, 1000)).toHaveLength(MAX_CLOUD_QUERY);
  });
  it("aplica la respuesta y conserva la referencia si no cambia", () => {
    const a = applyCloudResults(new Set(), ["p", "q"], [true, false]);
    expect([...a]).toEqual(["p"]);
    expect(applyCloudResults(a, ["p", "q"], [true, false])).toBe(a);
    expect([...applyCloudResults(a, ["p"], [false])]).toEqual([]);
  });
});

describe("rutas que dejan de existir", () => {
  const list = [{ path: "C:\\a", at: 1 }, { path: "N:\\b", at: 2 }, { path: "C:\\c", at: 3 }];
  it("solo salen las que se sabe que no existen", () => {
    expect(paths(dropMissingRecents(list, { "c:\\a": "missing", "n:\\b": "unavailable", "c:\\c": "ok" }))).toEqual(["N:\\b", "C:\\c"]);
  });
  it("sin cambios devuelve la misma lista (no se escribe)", () => {
    expect(dropMissingRecents(list, {})).toBe(list);
  });
});

describe("agrupación de unidades", () => {
  const d = (path: string, driveKind: DriveUsage["kind"] | null): Place => ({ id: path, label: path, path, kind: "drive", driveKind });
  it("por tipo, en orden fijo y sin grupos vacíos", () => {
    const g = groupDrives([d("N:\\", "network"), d("C:\\", "fixed"), d("E:\\", "removable"), d("D:\\", "fixed"), d("F:\\", null)]);
    expect(g.map((x) => x.title)).toEqual(["Discos locales", "Unidades extraíbles", "Unidades de red", "Otras unidades"]);
    expect(paths(g[0].drives)).toEqual(["C:\\", "D:\\"]);
  });
});

describe("unidades", () => {
  it("etiquetas y porcentaje", () => {
    expect(driveKindLabel("removable")).toBe("Unidad extraíble");
    expect(driveKindLabel(null)).toBe("Unidad");
    expect(usedPercent({ total: 200, used: 50 })).toBe(25);
    expect(usedPercent({ total: 0, used: 0 })).toBeNull();
    expect(usedPercent(undefined)).toBeNull();
  });
});

describe("insertionFromY", () => {
  const centers = [10, 40, 70];
  it("inserta delante de la primera fila cuyo centro queda debajo", () => {
    expect(insertionFromY(centers, 0)).toBe(0);
    expect(insertionFromY(centers, 25)).toBe(1);
    expect(insertionFromY(centers, 69)).toBe(2);
  });
  it("por debajo de todas inserta al final y sin filas devuelve 0", () => {
    expect(insertionFromY(centers, 500)).toBe(3);
    expect(insertionFromY([], 5)).toBe(0);
  });
});
