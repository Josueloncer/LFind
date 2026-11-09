// Lógica de publicación (sin dependencias): versión, manifiesto de actualización y verificación de firma. Probada en `release.test.mjs`.
import { createHash, createPublicKey, verify } from "node:crypto";

/**
 * Versión de una publicación: `MAYOR.MENOR.<número de ejecución>`.
 * El número de ejecución de GitHub Actions crece siempre, no vive en ningún archivo y por eso no hace falta ningún commit automático
 * (que además dispararía el workflow otra vez). El `MAYOR.MENOR` sale de `tauri.conf.json` y se sube a mano en los cambios grandes.
 */
export function computeVersion(baseVersion, runNumber) {
  const m = /^(\d+)\.(\d+)\.\d+/.exec(baseVersion);
  const run = Number(runNumber);
  if (!m || !Number.isSafeInteger(run) || run <= 0) throw new Error(`Versión base o número de ejecución no válidos: ${baseVersion}, ${runNumber}`);
  return `${m[1]}.${m[2]}.${run}`;
}

const parts = (v) => v.replace(/^v/, "").split(".").map(Number);

/** ¿`a` es estrictamente superior a `b`? */
export function isNewer(a, b) {
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return false;
}

/** Primera sección `## …` del registro de cambios: son las notas que verá el usuario. */
export function changelogNotes(markdown) {
  const lines = markdown.split(/\r?\n/);
  const start = lines.findIndex((l) => l.startsWith("## "));
  if (start < 0) return "";
  const end = lines.findIndex((l, i) => i > start && l.startsWith("## "));
  return lines.slice(start, end < 0 ? undefined : end).join("\n").replace(/^## /, "").trim();
}

export function buildManifest({ version, notes, date, url, signature, size, platform = "windows-x86_64" }) {
  if (!version || !url || !signature || !size) throw new Error("Faltan datos para el manifiesto.");
  return { version, notes, pub_date: date, sizeBytes: size, platforms: { [platform]: { signature, url } } };
}

const SPKI_ED25519 = Buffer.from("302a300506032b6570032100", "hex");
const decode = (b64) => Buffer.from(b64.trim(), "base64");
const textOf = (b64) => decode(b64).toString("utf8").split(/\r?\n/);

/**
 * Comprueba una firma de Tauri (formato minisign) contra la clave pública configurada en la aplicación.
 * El plugin hace lo mismo en cada instalación: si la clave con la que se firma no fuese la correcta, ningún usuario podría actualizar.
 * Devuelve el comentario de confianza; lanza si la firma no es válida.
 */
export function verifySignature(file, signatureB64, pubkeyB64) {
  const keyBytes = decode(textOf(pubkeyB64)[1]);
  const [, sigLine, trustedLine, globalLine] = textOf(signatureB64);
  const sig = decode(sigLine);
  if (keyBytes.length !== 42 || sig.length !== 74) throw new Error("Formato de clave o firma no válido.");
  if (!keyBytes.subarray(2, 10).equals(sig.subarray(2, 10))) throw new Error("La firma se hizo con otra clave.");
  const key = createPublicKey({ key: Buffer.concat([SPKI_ED25519, keyBytes.subarray(10)]), format: "der", type: "spki" });
  const hashed = sig.subarray(0, 2).toString("latin1") === "ED";
  const message = hashed ? createHash("blake2b512").update(file).digest() : file;
  if (!verify(null, message, key, sig.subarray(10))) throw new Error("La firma no corresponde al archivo.");
  const trusted = trustedLine.replace(/^trusted comment: /, "");
  if (!verify(null, Buffer.concat([sig.subarray(10), Buffer.from(trusted)]), key, decode(globalLine))) throw new Error("El comentario de la firma fue alterado.");
  return trusted;
}
