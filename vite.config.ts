import { cpSync, createReadStream, existsSync } from "node:fs";
import { join, normalize, resolve } from "node:path";
import { defineConfig } from "vitest/config";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";

/** Fuentes estándar y mapas de caracteres de PDF.js: hacen falta para los PDF que no incrustan sus fuentes. Se sirven desde la propia aplicación. */
function pdfjsAssets(): Plugin {
  const src = resolve(__dirname, "node_modules/pdfjs-dist");
  const dirs = ["cmaps", "standard_fonts"];
  return {
    name: "lfind-pdfjs-assets",
    configureServer(server) {
      server.middlewares.use("/pdfjs", (req, res, next) => {
        const file = join(src, normalize(decodeURIComponent((req.url ?? "").split("?")[0])));
        if (file.startsWith(src) && existsSync(file)) createReadStream(file).pipe(res);
        else next();
      });
    },
    closeBundle() {
      if (process.env.VITEST) return;
      for (const d of dirs) cpSync(join(src, d), resolve(__dirname, "dist/pdfjs", d), { recursive: true });
    },
  };
}

// Puerto fijo: tauri.conf.json apunta a http://localhost:1420
export default defineConfig({
  plugins: [react(), pdfjsAssets()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/crates/**"] } },
  build: { target: "chrome105", sourcemap: false },
  test: { environment: "node" },
});
