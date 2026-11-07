# UPDATE04_PROGRESS · estado real de la Update 04 (menú contextual) y su compilación

> **El menú contextual NATIVO de Windows (04/04.1) fue sustituido en 04.2 por el menú propio de LFind, por decisión del usuario.**
> No existe ya `nativeMenu.ts`, `shellMenu.ts`, `menuPerf.ts`, `shell_menu*.rs`, ni los tipos `ShellMenuOutcome`, `NativeExtra`,
> `ShellMenuArgs`, `ShellMenuErrKind` ni `FsProvider.shellMenuAvailable/showShellMenu`. Quien vea errores de TypeScript sobre ellos tiene en su
> carpeta archivos antiguos que sobrevivieron al aplicar el ZIP (un ZIP de «solo nuevos/modificados» no borra nada).
> **Arreglo:** `powershell -ExecutionPolicy Bypass -File scripts\clean-obsolete.ps1` desde la raíz (lista en `scripts/obsolete-04.2.txt`).

## Arquitectura vigente (lo que sigue de la Update 04)
| Parte de la 04 | Estado |
|---|---|
| Menú contextual | **Propio** (`src/lib/contextMenu.ts`, `src/components/ContextMenu.tsx`, comandos en `src-tauri/src/system_cmd.rs`). Detalle: `docs/CONTEXT_MENU.md` |
| Portapapeles del sistema (CF_HDROP + Preferred DropEffect) | Conservado (`clipboard.rs`, `clipboard_win.rs`, `clipboard_cmd.rs`, `systemClipboard.ts`) |
| Actualización automática tras acciones externas | Conservada (`scheduleRefresh`: 0,4 / 1,5 / 4 s) |
| Miniaturas verticales | Conservado el arreglo (`.thumb`, `.gname` en `app.css`; prueba `thumbLayout.test.ts`) |
| Mayús+F10 / tecla Menú | Conservado (con antirrebote de 500 ms) |

## Cambios de esta corrección (04.3.1) — compilación TypeScript
| Problema | Causa | Corrección |
|---|---|---|
| `node:fs` no reconocido en `thumbLayout.test.ts` (TS2591) | `tsconfig.json` limita `types` a `vite/client` y `@types/node` no estaba | `@types/node` en devDependencies; **`tsconfig.test.json`** (extiende al principal, `strict` intacto, añade `node`) para pruebas y `vite.config.ts` |
| Pruebas mezcladas con la compilación de producción | un único `tsconfig` incluía `*.test.ts` | `tsconfig.json` = solo código de la aplicación (excluye `*.test.ts(x)`; no puede usar APIs de Node); `tsconfig.test.json` = pruebas |
| `test` en `vite.config.ts` | `defineConfig` de `vite` no tipa la clave `test` | `defineConfig` importado de `vitest/config` |
| Errores 1–5 del informe (menú nativo) | archivos obsoletos de 04/04.1 aún en la carpeta del usuario | `scripts/clean-obsolete.ps1` borra exactamente esos archivos; los símbolos ya no existen en el código |
No se usó `any`, no se desactivó `strict`, no se ignoró ningún error ni se eliminó ninguna prueba.

## Corrección 04.3.2 — `themes.test.ts`
- Analizador de bloques de tema (`parseThemeBlocks`): `:root[data-theme="x"]\s*\{([^}]*)\}` y variables con `(?:^|[\s;{])(--[\w-]+)\s*:`; independiente de LF/CRLF/CR. Prueba nueva: mismo resultado con LF, CRLF y CR. Las dos pruebas originales se mantienen sin cambios en su intención.
- Verificación de que las pruebas comprueban de verdad (arnés propio, `themes.css` real): 8/8 con LF y con CRLF; **mutaciones**: quitar una variable a «hacker» → falla «mismas variables» (`--bg-hover`); renombrar el bloque «minimal» → falla «un bloque por tema»; añadir una variable a «light» → falla «mismas variables». Cinco temas intactos: 5 bloques × 62 variables.
- La expresión anterior ya daba 5×62 con CRLF en mi comprobación: **no pude reproducir el fallo ni confirmar su causa**.
- Defecto propio hallado y corregido: `#e5484d` en `app.css` (04.3) incumplía «sin colores a mano» → `var(--danger)`.
- Estado: `npm test`, `npm run typecheck`, `npm run build` **no ejecutados** (sin npm/vitest aquí).

## Scripts (`package.json`)
- `npm run build` → `tsc --noEmit -p tsconfig.json && vite build` (solo código de producción).
- `npm run typecheck` → producción **y** pruebas (`tsconfig.test.json`).
- `npm test` → `vitest run`. `npm run test:core` → `cargo test` de `nova-core`.
- `npx tsc --noEmit` (sin `-p`) comprueba producción (usa `tsconfig.json`).

## Dependencias
Nueva: `@types/node ^20.17.6` (devDependency). **`package-lock.json` NO está actualizado** (no hay npm/registro aquí): ejecuta `npm install` (no `npm ci`) y conserva el `package-lock.json` resultante.

## Coherencia frontend ↔ Tauri
Comprobado con un script: los 39 `invoke(...)` de `src/lib/fs/tauri.ts` tienen su `#[tauri::command]` registrado en `generate_handler!` y los mismos nombres de argumentos (camelCase ↔ snake_case): 0 discrepancias. (Comprobación por texto: no sustituye a compilar `src-tauri`.)

## Estado real de verificación
| Paso | Estado |
|---|---|
| `npx tsc --noEmit` | **NO ejecutado con las dependencias reales** (no hay npm/registro aquí). Comprobado con las que sí hay (`typescript`, `@types/node`, `react`): el error `node:fs` (TS2591) se reproduce sin tipos de Node y desaparece con `tsconfig.test.json`; el código de producción no incluye ningún `*.test.*` |
| `npm run test` | **NO ejecutado** (sin `vitest`). Las pruebas se corren en un arnés propio: 38/38 de las suites de 04.3 |
| `npm run build` | **NO ejecutado** |
| `npm run tauri build` | **NO ejecutado: no hay Windows** |
Cuando lo ejecutes: `npm install` → `npm run typecheck` → `npm test` → `npm run build` → `npm run tauri build`, y pega cualquier error exacto.
