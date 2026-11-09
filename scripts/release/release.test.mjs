import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { describe, expect, it } from "vitest";
import { buildManifest, changelogNotes, computeVersion, isNewer, verifySignature } from "./lib.mjs";

describe("versión", () => {
  it("usa el número de ejecución como parche", () => {
    expect(computeVersion("0.1.0", "42")).toBe("0.1.42");
    expect(computeVersion("2.5.9", 7)).toBe("2.5.7");
  });
  it("rechaza datos no válidos", () => {
    for (const [b, r] of [["x", 1], ["0.1.0", 0], ["0.1.0", "abc"], ["0.1.0", undefined]]) expect(() => computeVersion(b, r)).toThrow();
  });
  it("cada ejecución supera a la anterior, y al instalador sin actualizador (0.1.0)", () => {
    expect(isNewer(computeVersion("0.1.0", 1), "0.1.0")).toBe(true);
    expect(isNewer("0.1.10", "0.1.9")).toBe(true);
    expect(isNewer("v0.2.3", "v0.1.900")).toBe(true);
    expect(isNewer("0.1.5", "0.1.5")).toBe(false);
    expect(isNewer("0.1.4", "v0.1.5")).toBe(false);
  });
});

describe("notas y manifiesto", () => {
  it("toma solo la primera sección del registro", () => {
    expect(changelogNotes("# Título\r\n\r\n## Update 8: algo\r\n- uno\r\n\r\n## Update 7\r\n- dos")).toBe("Update 8: algo\n- uno");
    expect(changelogNotes("nada")).toBe("");
  });
  it("describe la plataforma y exige todos los datos", () => {
    const m = buildManifest({ version: "0.1.2", notes: "n", date: "d", url: "https://x/a.exe", signature: "s", size: 9 });
    expect(m.platforms["windows-x86_64"]).toEqual({ signature: "s", url: "https://x/a.exe" });
    expect(m.sizeBytes).toBe(9);
    expect(() => buildManifest({ version: "0.1.2", url: "https://x", signature: "", size: 9 })).toThrow();
  });
});

describe("verificación de firma", () => {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(12);
  const id = Buffer.from("0123456789abcdef", "hex");
  const b64 = (t) => Buffer.from(t).toString("base64");
  const pub = b64(`untrusted comment: clave\n${Buffer.concat([Buffer.from("Ed"), id, raw]).toString("base64")}\n`);

  function signed(file, opts = {}) {
    const digest = createHash("blake2b512").update(file).digest();
    const s = sign(null, digest, privateKey);
    const trusted = "timestamp:1\tfile:a.exe";
    const global = sign(null, Buffer.concat([s, Buffer.from(trusted)]), privateKey);
    const keyId = opts.keyId ?? id;
    return b64(`untrusted comment: firma\n${Buffer.concat([Buffer.from("ED"), keyId, s]).toString("base64")}\ntrusted comment: ${opts.trusted ?? trusted}\n${global.toString("base64")}\n`);
  }

  const file = Buffer.from("contenido del instalador");

  it("acepta una firma correcta", () => expect(verifySignature(file, signed(file), pub)).toContain("file:a.exe"));
  it("rechaza un paquete alterado", () => expect(() => verifySignature(Buffer.from("otro"), signed(file), pub)).toThrow());
  it("rechaza otra clave", () => expect(() => verifySignature(file, signed(file, { keyId: Buffer.alloc(8, 1) }), pub)).toThrow(/otra clave/));
  it("rechaza un comentario manipulado", () => expect(() => verifySignature(file, signed(file, { trusted: "version:9" }), pub)).toThrow());
  it("rechaza datos sin formato", () => expect(() => verifySignature(file, b64("nada"), pub)).toThrow());
});
