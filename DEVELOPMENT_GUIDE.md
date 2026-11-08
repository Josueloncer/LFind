# LFind · DEVELOPMENT_GUIDE

Guía práctica para instalar, probar, compilar y añadir funcionalidades. Antes de empezar, leer las **reglas obligatorias** de [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md).

## Requisitos (Windows 10/11 x64)

| Herramienta | Instalación | En el equipo auditado |
|---|---|---|
| Microsoft C++ Build Tools (Desarrollo de escritorio con C++) | `winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` | Disponible |
| WebView2 Runtime | Incluido en Windows 11 | Disponible |
| Rust stable-msvc (≥ 1.77.2) | `winget install Rustlang.Rustup`, luego `rustup default stable-msvc` | 1.99.1 |
| Node.js LTS (≥ 20) | `winget install OpenJS.NodeJS.LTS` | 24.22.1 / npm 11.19.1 |
| Python (opcional, solo para `scripts/index_*.py`) | `winget install Python.Python.3.12` | No disponible en PATH |

MSVC es imprescindible: Tauri lo exige y `rusqlite` compila SQLite en C.

## Instalación

```powershell
cd C:\Dev\nova-explorer
npm ci
```

`package-lock.json` y ambos `Cargo.lock` están actualizados y deben conservarse.

## Ejecución en desarrollo

```powershell
npm run tauri dev   # aplicación completa (puerto 1420)
npm run dev         # solo la interfaz en el navegador, sin disco
```

- La primera compilación de Rust tarda varios minutos.
- El modo desarrollo optimiza `nova-core`, `serde_json` y los decodificadores de imagen (`src-tauri/Cargo.toml`, `src-tauri/.cargo/config.toml`).
- Diagnóstico: consola de DevTools (tabla de marcas de `lib/perf.ts`) y líneas `[nova]` en la terminal.
- **Abrir la app inicia el indexador a los 3 s**, también en desarrollo, y usa la misma base de datos que la versión instalada.

## Pruebas

```powershell
npm test            # Vitest
npm run typecheck   # TypeScript: producción y pruebas
npm run test:core   # nova-core sin SQLite
cargo test --manifest-path crates/nova-core/Cargo.toml --features sqlite --locked --offline
```

**Línea base (Update 07, 2026-11-08):** Vitest 217/217 · typecheck sin errores · `nova-core` 163/163 (160 unitarias + 3 de integración). Cualquier fallo es una regresión.

**Probar el visor con archivos reales:** `cargo run --release --manifest-path crates/nova-core/Cargo.toml --example viewer_probe -- <salida> <archivo>...`. Imprime cómo se resuelve cada archivo y deja las conversiones en la carpeta de salida. Para conducir la app empaquetada: `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` y CDP.

Usar `--locked --offline` al verificar para no alterar `Cargo.lock` ni descargar nada.

| Tipo de prueba | Dónde |
|---|---|
| Lógica pura TS | `src/lib/*.test.ts`, `src/stores/*.test.ts` |
| Contrato de CSS y temas | `src/lib/themes.test.ts`, `src/styles/thumbLayout.test.ts` |
| Rust | `#[cfg(test)]` en cada módulo; índice en `crates/nova-core/src/index/tests.rs` |
| Paridad Rust ↔ TS | `thumbs.rs` (lee `thumbKinds.ts`), `icons.rs` (lee `iconKinds.ts`) |

No hay pruebas de componentes React ni de extremo a extremo: lo visual e interactivo se comprueba a mano con `WINDOWS_VERIFICATION.md`.

**Bancos** (no forman parte de las pruebas):

```powershell
cargo run --release --manifest-path crates/nova-core/Cargo.toml --example bench_dir -- <carpeta>
cargo run --release --manifest-path crates/nova-core/Cargo.toml --example bench_thumb -- <argumentos>
cargo run --manifest-path crates/nova-core/Cargo.toml --example index_xcheck -- <dir>
python scripts/index_xcheck.py <dir> [--bench]
python scripts/index_bench.py <dir> [filas]
```

## Compilación e instalador

```powershell
npm run build        # tsc + Vite → dist/
npm run tauri build  # compilación release + instalador
```

| Salida | Ruta |
|---|---|
| Ejecutable | `src-tauri\target\release\nova-explorer.exe` |
| Instalador NSIS | `src-tauri\target\release\bundle\nsis\LFind_0.1.0_x64-setup.exe` |

La versión se define en tres lugares: `package.json`, `src-tauri/tauri.conf.json` y `src-tauri/Cargo.toml`. El instalador no está firmado.

Iconos: `python scripts/make-icons.py` (requiere Pillow).

## Datos locales

| Qué | Ruta |
|---|---|
| Preferencias | `%APPDATA%\app.nova.explorer\prefs.json` |
| Índice | `%LOCALAPPDATA%\app.nova.explorer\index\` |
| Cachés | `%LOCALAPPDATA%\app.nova.explorer\thumbs`, `icons`, `viewer-v1`, `EBWebView` |
| Registro de errores | `%LOCALAPPDATA%\app.nova.explorer\logs\lfind-errors.log` (JSON por línea; `kind`: `ui` o `panic`). Puede contener rutas; es solo local |

Para inspeccionar el índice, abrirlo siempre en **solo lectura** y con LFind cerrado.

## Procedimiento para desarrollar una funcionalidad

1. **Contexto.** Leer `PROJECT_CONTEXT.md` y `PROJECT_STATUS.md`. Revisar `git status`: puede haber trabajo sin commitear que no se debe tocar.
2. **Investigar.** Leer los módulos implicados y comprobar que la documentación sigue siendo cierta.
3. **Línea base.** Ejecutar las pruebas antes de cambiar nada y anotar el resultado.
4. **Diseñar sobre lo existente.** Buscar el componente, store o servicio que ya resuelva parte del problema. Si hay riesgos relacionados en `PROJECT_STATUS.md`, tenerlos en cuenta.
5. **Dependencias.** Solo con justificación y confirmación del usuario.
6. **Implementar por capas:**
   - Lógica en `nova-core`, con pruebas.
   - Comando en `src-tauri`, en `spawn_blocking` si hace E/S. Registrarlo en `generate_handler!`.
   - Contrato en `lib/fs/types.ts`, implementado en `tauri.ts` y `browser.ts`.
   - Lógica pura en `lib/` con pruebas; estado en `stores/`.
   - Componentes reutilizando clases y tokens.
7. **Revisar el rendimiento.** Sin E/S en el hilo principal, sin IPC por elemento, con virtualización, cancelación y límites.
8. **Revisar el diseño** en los 6 temas y en anchos de 640 px en adelante. Comprobar el foco, el teclado y `prefers-reduced-motion`.
9. **Verificar.** `npm test`, `npm run typecheck`, `npm run test:core`, y `cargo test --features sqlite` si se tocó el índice. Probar la app real cuando el cambio sea de interfaz. Informar de lo que no se pudo ejecutar.
10. **Documentar.** `CHANGELOG.md`, `PROJECT_STATUS.md`, `WINDOWS_VERIFICATION.md` si hay pasos manuales, y `PROJECT_CONTEXT.md` / `PROJECT_ARCHITECTURE.md` si cambió el diseño.
11. **No hacer commit ni push** sin autorización expresa.

### Casos especiales

| Cambio | Hay que tocar además |
|---|---|
| Esquema del índice | Subir `SCHEMA_VERSION` en `sqlgen.rs` (reconstruye el índice), mantener `MemStore`, validar con `index_xcheck` |
| Campo en `Prefs` | `prefs.rs` (con valor por defecto), `types.ts`, `DEFAULT_PREFS` |
| Formato de miniatura | `thumbs.rs` y `thumbKinds.ts`; subir `CACHE_VERSION` si cambia el aspecto |
| Tema nuevo | `themes.css`, `themes.ts`, `boot.js`, `index.html`, `prefs.rs` |
| Altura de filas | CSS y `viewGeometry.ts` / `view.ts` |
| API de ventana nueva | `capabilities/default.json` |
| Mostrar archivos locales en la interfaz | Revisar CSP en `tauri.conf.json` |
| Icono nuevo | Una línea en `components/icons.ts` |
| Formato nuevo en el visor | Añadir la ruta en `viewer/mod.rs`; ejecutar la conversión con `ViewerService::cached` |
| Cambio del aspecto de las conversiones | Cambiar el nombre de la carpeta `viewer-v1` en `src-tauri/src/lib.rs` |
| Decodificador o procesado de archivos nuevo | Ejecutarlo dentro de `diagnostics::isolate`; no usar `panic = "abort"` |
| Componente que pueda fallar con datos de un archivo | Colocarlo bajo el `ErrorBoundary` adecuado |

## Si algo falla al compilar

| Síntoma | Comprobar |
|---|---|
| Error de enlazado o C | Build Tools instaladas; toolchain `stable-msvc` |
| `generate_context!` falla por `dist` | Ejecutar `npm run build` |
| Error de permiso `core:window:*` | `capabilities/default.json` |
| Archivos `nativeMenu`, `shellMenu` reaparecen | `scripts/clean-obsolete.ps1` (solo tras copiar actualizaciones de versiones antiguas) |
| `app.css`: «colores a mano» en pruebas | Usar variables de `themes.css` |
