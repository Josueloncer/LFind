import { describe, expect, it } from "vitest";
import { folderFromInner, resolveSources } from "./externalDrop";

const TMP = "C:\\Users\\a\\AppData\\Local\\Temp\\Rar$DRa65200.26303.rartemp";

describe("arrastre desde otras aplicaciones", () => {
  it("deduce la carpeta soltada a partir de uno de sus archivos", () => {
    expect(folderFromInner(`${TMP}\\muestra\\a.txt`, "a.txt")).toBe(`${TMP}\\muestra`);
    expect(folderFromInner(`${TMP}\\muestra\\sub\\c.txt`, "sub/c.txt")).toBe(`${TMP}\\muestra`);
    expect(folderFromInner(`${TMP}\\Muestra\\A.TXT`, "a.txt")).toBe(`${TMP}\\Muestra`);
    expect(folderFromInner(`${TMP}\\otra\\x.txt`, "a.txt")).toBeNull();
    expect(folderFromInner("a.txt", "a.txt")).toBeNull();
  });
  it("empareja en orden archivos, carpetas y carpetas vacías", () => {
    const r = resolveSources(
      [{ kind: "file", name: "nota.txt" }, { kind: "empty", name: "vacía" }, { kind: "dir", name: "muestra", rel: "sub/c.txt" }],
      [`${TMP}\\nota.txt`, `${TMP}\\muestra\\sub\\c.txt`],
    );
    expect(r.sources).toEqual([`${TMP}\\nota.txt`, `${TMP}\\muestra`]);
    expect(r.unresolved).toEqual(["vacía"]);
  });
  it("lo que WebView2 no devolvió se informa, no se inventa", () => {
    const r = resolveSources([{ kind: "file", name: "a" }, { kind: "file", name: "b" }], ["C:\\x\\a"]);
    expect(r.sources).toEqual(["C:\\x\\a"]);
    expect(r.unresolved).toEqual(["b"]);
  });
});
