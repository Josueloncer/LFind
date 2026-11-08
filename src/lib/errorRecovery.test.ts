import { describe, expect, it } from "vitest";
import { FAILURE_WINDOW_MS, MAX_RECENT_FAILURES, MAX_REPORT_CHARS, canRetry, errorReport, registerFailure } from "./errorRecovery";

describe("recuperación tras un error de renderizado", () => {
  it("permite reintentar hasta que el error se repite demasiado", () => {
    let h: number[] = [];
    for (let i = 1; i < MAX_RECENT_FAILURES; i++) {
      h = registerFailure(h, i * 100);
      expect(canRetry(h)).toBe(true);
    }
    h = registerFailure(h, 1000);
    expect(canRetry(h)).toBe(false);
  });

  it("los fallos antiguos dejan de contar", () => {
    let h: number[] = [];
    for (let i = 0; i < MAX_RECENT_FAILURES; i++) h = registerFailure(h, i);
    expect(canRetry(h)).toBe(false);
    h = registerFailure(h, MAX_RECENT_FAILURES + FAILURE_WINDOW_MS);
    expect(h).toHaveLength(1);
    expect(canRetry(h)).toBe(true);
  });

  it("el informe conserva el detalle técnico y lo acota", () => {
    const e = new TypeError("x no es una función");
    e.stack = "pila";
    expect(errorReport(e, "\n  en FileView")).toEqual({ message: "TypeError: x no es una función", stack: "pila\n--- componentes ---\n\n  en FileView" });
    expect(errorReport(new Error("m".repeat(MAX_REPORT_CHARS + 50))).message.length).toBeLessThanOrEqual(MAX_REPORT_CHARS + "Error: ".length);
  });

  it("acepta cualquier valor lanzado", () => {
    expect(errorReport("texto")).toEqual({ message: "texto", stack: null });
    expect(errorReport(undefined).message).toBe("undefined");
    expect(errorReport(Object.create(null)).message).toBe("Error no representable");
  });
});
