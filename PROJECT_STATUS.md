# NOVA Explorer · Estado del proyecto

_Última actualización: Fase 1.1 (auditoría y optimización de arranque)._

## Cómo retomar el trabajo
1. Leer este archivo y `README.md`.
2. Pedir al usuario el resultado de `npm run tauri dev` y `npm run test:core` en Windows (ver "Pendiente de validar").
3. No avanzar a la Fase 3 hasta que la Fase 2 (validación) esté confirmada.

## Entorno de desarrollo
- Claude trabaja en un **contenedor Linux** (sin acceso al PC del usuario, sin Windows, sin WebView2).
- Ahí se pueden ejecutar: `tsc`, `vite build`, `vitest`, y `cargo test` de `nova-core` (Rust 1.75 de apt).
- **No** se puede compilar `src-tauri` (Tauri 2 exige Rust ≥ 1.77.2 y librerías de GUI) ni ver la ventana.
- El usuario compila y prueba en Windows y devuelve errores/capturas.

## Decisiones de arquitectura
- **`nova-core` separado de Tauri**: toda la lógica de disco es testeable sin GUI y reutilizable (CLI, indexador).
- **Lectura por lotes con `Channel`**: primer lote de 64 entradas (primer pintado inmediato), después lotes de 1500. Solo viaja el nombre, no la ruta completa.
- **Cancelación real**: cada lectura tiene un id y una bandera atómica; cambiar de carpeta cancela la lectura anterior y la UI descarta sus eventos.
- **`FsProvider`**: la UI nunca llama a Tauri directamente. Sin Tauri (navegador) no hay archivos y se muestra un aviso; no hay datos ficticios.
- **Virtualización** con `@tanstack/react-virtual` en lista y cuadrícula.
- **Orden diferido**: durante la carga se muestra en orden de llegada; se ordena una vez al terminar.
- **Temas por variables CSS** (`tokens.css`); el modo claro está implementado.
- **Mica** (Windows 11) se aplica en Rust; si falla, la UI usa superficies opacas (`data-vibrancy`).
- Ventana sin decoraciones; barra de título y controles propios.

## Fase 1.1 · Auditoría de arranque y rendimiento
Auditoría hecha sobre el código real. El síntoma ("pantalla blanca" y retraso al abrir) **no se pudo reproducir en el contenedor**: no hay Windows ni WebView2. Las causas siguientes están demostradas **en el código**; su peso real en tu PC está **sin medir** (ver "Pendiente de validar").

| Hallazgo (evidencia en código) | Cambio |
|---|---|
| La ventana se creaba visible: se veía vacía hasta que WebView2 cargaba y pintaba | `"visible": false`; la UI la muestra al tener prefs+ubicaciones (`App.tsx`); respaldo a 1,2 s en JS y a 6 s en Rust |
| `get_places`, `get_prefs`, `set_prefs` eran comandos síncronos: Tauri los ejecuta en el **hilo principal de la ventana**; `get_places` consulta carpetas del shell y hace `is_dir()` (Escritorio/Documentos están en OneDrive) | `async` + `spawn_blocking` |
| Tema y Mica se decidían tras 2–3 llamadas IPC: parpadeo de tema equivocado / fondo opaco→Mica; con SO claro la app nacía clara | `index.html` (fondo inmediato) + `public/boot.js` (último tema/Mica cacheados); el tema no se toca hasta cargar prefs |
| `Pane` mostraba "Sin ubicación inicial" durante el arranque | Estado `starting`: no se muestra nada hasta tener pestaña |
| `lucide-react` pre-empaquetado en dev = **1,1 MB** (~1 500 iconos evaluados al arrancar) para usar 27 | `src/components/icons.ts` con importaciones directas. En build ya se hacía tree-shaking |
| Ordenar por nombre en el hilo de UI: **543 ms con 100 k, 1,7 s con 300 k** (medido en el contenedor) | Worker (`sort.worker.ts`) para listas > 3 000; comparador único en `sortCore.ts`; fallback síncrono |
| El orden se recalculaba al teclear en el filtro | Se ordena la lista completa una vez y el filtro solo filtra; `useDeferredValue` |
| `ListRow` memoizada, pero `onDown`/`pick` cambiaban en cada selección | Refs → callbacks estables |
| `ResizeObserver` re-renderizaba la lista en cada resize aunque solo la cuadrícula usa el ancho | Solo se observa en modo cuadrícula |
| `onResized` hacía una llamada IPC por cada evento de redimensión | Agrupado (120 ms) |
| `hidden.some()` recorría toda la lista en cada render de `Pane` | `useMemo` |
| StrictMode (solo dev) duplicaba `load()`/`boot()` → IPC doble | Promesas idempotentes |
| `tauri dev` = Rust en **debug**: serializar 100 k entradas a JSON = **221 ms** (14 ms con opt-level 2; medido en Linux con `bench_dir`) | `[profile.dev.package.*]` en `src-tauri/Cargo.toml` solo para nova-core, serde_json y la lib propia |

Instrumentación nueva (solo desarrollo): marcas de arranque (`src/lib/perf.ts`) que imprimen una tabla en la consola de DevTools al terminar la primera carpeta; línea `[nova] open_dir …` en la terminal de `tauri dev`; banco de pruebas `crates/nova-core/examples/bench_dir.rs`.

**Descartado por no estar justificado** (se dejó como está): fusionar los comandos de arranque en uno (ya van en paralelo; el coste real era el hilo principal), caché de listados, reescribir la ordenación en Rust (ver "Siguientes pasos").

## Qué funciona (implementado)
- Ventana premium: pestañas, navegación, migas editables, búsqueda-filtro, panel lateral, vistas lista/iconos, estado, tema oscuro/claro/sistema.
- Lectura real de directorios, historial por pestaña, favoritos persistentes, ocultos, ordenación por columnas, selección múltiple y teclado.
- Preferencias en `%APPDATA%\app.nova.explorer\prefs.json` (escritura atómica; si está corrupto se guarda `.bak`).
- Filtro del panel: filtra **solo la carpeta abierta** (no es la búsqueda avanzada).

## Verificado en el contenedor (Linux)
| Comprobación | Resultado |
|---|---|
| `cargo test` en `crates/nova-core` | 12 tests OK (en Windows: 10, los otros 2 son solo Unix) |
| `tsc --noEmit` (strict) | Sin errores |
| `vitest` | 25 tests OK (rutas, navegación/pestañas, ordenación, equivalencia worker↔síncrono, acentos) |
| `vite build` | OK; 104 módulos (antes 1 612); worker generado |
| Servidor `vite` dev | Sirve `index.html`, `boot.js`, `main.tsx` y el worker (HTTP 200); el paquete de 1,1 MB de iconos ya no se genera |
| TOML/JSON de `src-tauri` | Sintaxis válida (**no compilado**) |
| `bench_dir` (100 k archivos, Linux) | debug 415 ms (json 221) · release 253 ms (json 16) · debug opt-level 2: 187 ms (json 14) |

## Pendiente de validar en Windows (NO probado)
- **Fase 1.1:** que `src-tauri` siga compilando (cambios en `commands.rs`, `lib.rs`, perfiles de `Cargo.toml`, permiso `core:window:allow-show`).
- Que la ventana oculta **se muestre** y que no haya un fotograma vacío al mostrarla; si no aparece, el respaldo de 6 s la mostrará.
- Que el **Worker** de ordenación funcione dentro de WebView2 (también en `tauri build`, con la CSP). Si falla, se ordena de forma síncrona (no se rompe, pero se congela con carpetas enormes).
- Medir de verdad: tabla de marcas en DevTools (F12) y línea `[nova]` de la terminal; carpetas de 10 k / 100 k.
- Que `src-tauri` compile: las APIs `tauri::window::{Effect, EffectsBuilder}` y `WebviewWindow::set_effects`, `tauri::ipc::Channel` y los identificadores de permisos (`core:window:allow-*`) se escribieron sin poder compilarlos.
- Que la ventana sin decoraciones se pueda **redimensionar y mover**, y que los controles funcionen.
- Efecto Mica, transparencias, esquinas redondeadas, y el aspecto real de ambos temas (la UI **no se ha visto renderizada**).
- Ramas `#[cfg(windows)]` de `nova-core` (atributos de ocultos/sistema, `GetLogicalDrives`).
- Lectura de carpetas reales, accesos denegados, uniones/enlaces, rutas UNC.
- Todo el **rendimiento**: no hay métricas. Pendiente medir con carpetas grandes (10k/100k/1M) en Windows.
- Generación del instalador NSIS.

## Limitaciones conocidas
- Doble clic en archivos no hace nada aún (sin plugin de apertura).
- La ordenación de carpetas grandes ya no bloquea la UI (Worker), pero con ~1 M de entradas copiar/ordenar sigue siendo costoso (copiar nombres ≈ 180 ms en el hilo UI, medido en Node). Si hiciera falta: ordenar en Rust con `StrCmpLogicalW` y enviar solo la permutación.
- Durante la lectura la lista se muestra en orden de llegada; se reordena al terminar.
- Sin caché de listados ni miniaturas (módulos `thumbs`/`search` reservados).
- Sin `Cargo.lock` en `src-tauri` (se genera al compilar; conviene commitearlo).
- Si Mica falla en Windows 10, la ventana queda sin esquinas redondeadas.

## Próximos pasos
**Fase 2 · Validación en Windows** — aplicar la Fase 1.1, compilar, pegar la tabla de arranque y la línea `[nova]`, revisar capturas del diseño, ajustar. Medir carpetas grandes con `bench_dir` y en la app.
**Fase 3** — abrir archivos con la app predeterminada, menú contextual real, copiar/mover/renombrar/eliminar (papelera), crear carpeta/archivo, cola de tareas.
**Fase 4** — miniaturas bajo demanda + panel de vista previa; caché LRU en disco.
**Fase 5** — búsqueda e indexación incremental; doble panel.
Después: duplicados y análisis de almacenamiento, comparación/sincronización, automatizaciones, IA local opcional.
