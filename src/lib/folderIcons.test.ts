import { describe, expect, it } from "vitest";
import { usesSystemIcon } from "./iconKinds";

describe("iconos de carpeta propios", () => {
  it("las carpetas nunca piden el icono del sistema (ni las ocultas ni las de sistema)", () => {
    expect(usesSystemIcon({ kind: "dir" })).toBe(false);
  });
  it("los archivos y «otros» siguen usando el icono del sistema (EXE, PDF, PSD, .lnk…)", () => {
    expect(usesSystemIcon({ kind: "file" })).toBe(true);
    expect(usesSystemIcon({ kind: "other" })).toBe(true);
  });
});
