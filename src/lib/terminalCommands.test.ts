import { describe, expect, it } from "vitest";
import { cdCommand, parseCwdReport, terminalTheme, trackInput } from "./terminalCommands";

describe("cambio de carpeta en la terminal", () => {
  it("PowerShell recibe una ruta literal, sin interpretar nada", () => {
    expect(cdCommand("powershell", "C:\\Mis $datos")).toBe("Set-Location -LiteralPath 'C:\\Mis $datos'\r");
    expect(cdCommand("powershell", "C:\\it's")).toBe("Set-Location -LiteralPath 'C:\\it''s'\r");
  });
  it("CMD cambia también de unidad", () => {
    expect(cdCommand("cmd", "D:\\Fotos y más")).toBe('cd /d "D:\\Fotos y más"\r');
  });
  it("no escribe lo que se interpretaría como otra cosa", () => {
    for (const p of ["C:\\100%", 'C:\\a"b', "C:\\a^b", "C:\\a!b"]) expect(cdCommand("cmd", p)).toBeNull();
    expect(cdCommand("powershell", "C:\\a\nrm x")).toBeNull();
    expect(cdCommand("cmd", "C:\\a\rdel x")).toBeNull();
  });
});

describe("carpeta anunciada por el shell", () => {
  it("lee la ruta con o sin comillas e ignora otras secuencias", () => {
    expect(parseCwdReport("9;C:\\Users\\ana")).toBe("C:\\Users\\ana");
    expect(parseCwdReport('9;"D:\\Mi carpeta"')).toBe("D:\\Mi carpeta");
    expect(parseCwdReport("4;3;50")).toBeNull();
    expect(parseCwdReport("9;")).toBeNull();
  });
});

describe("línea a medio escribir", () => {
  it("detecta que se está escribiendo y cuándo se envía", () => {
    const s = { dirty: false };
    trackInput(s, "d");
    expect(s.dirty).toBe(true);
    trackInput(s, "\r");
    expect(s.dirty).toBe(false);
  });
  it("Ctrl+C descarta la línea; las flechas no la crean", () => {
    const s = { dirty: true };
    trackInput(s, "\x03");
    expect(s.dirty).toBe(false);
    trackInput(s, "\x1b[A");
    expect(s.dirty).toBe(false);
  });
});

describe("colores", () => {
  it("salen del tema", () => {
    expect(terminalTheme((n) => n).background).toBe("--bg-content");
  });
});
