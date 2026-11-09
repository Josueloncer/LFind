// Órdenes que usa `.github/workflows/release.yml`. Cualquier problema termina con error y detiene la publicación.
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { buildManifest, changelogNotes, computeVersion, isNewer, verifySignature } from "./lib.mjs";

const conf = () => JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const [cmd, ...a] = process.argv.slice(2);

switch (cmd) {
  case "version": {
    // version <número de ejecución> [última etiqueta publicada]
    const v = computeVersion(conf().version, a[0]);
    if (a[1] && !isNewer(v, a[1])) throw new Error(`La versión ${v} no es superior a la publicada (${a[1]}).`);
    console.log(v);
    break;
  }
  case "verify": {
    // verify <instalador> <firma>
    const trusted = verifySignature(readFileSync(a[0]), readFileSync(a[1], "utf8"), conf().plugins.updater.pubkey);
    console.log(`Firma válida. ${trusted}`);
    break;
  }
  case "manifest": {
    // manifest <versión> <instalador> <firma> <url> <salida>
    const [version, installer, sig, url, out] = a;
    const m = buildManifest({ version, url, notes: changelogNotes(readFileSync("CHANGELOG.md", "utf8")), date: new Date().toISOString(), signature: readFileSync(sig, "utf8").trim(), size: statSync(installer).size });
    writeFileSync(out, JSON.stringify(m, null, 2));
    break;
  }
  default:
    throw new Error(`Orden desconocida: ${cmd}`);
}
