# UPDATE05_PROGRESS · Motor de búsqueda global (Fase 1)

Base: checkpoint `9eac9b2` (más la corrección 04.3.3). **Fase 1 = núcleo del índice persistente + búsqueda integrada en la interfaz.**
**Fase 2 (pendiente, NO implementada) = observador `ReadDirectoryChangesW`** para actualizar sin reescanear. Decisiones fijadas por el usuario: el almacén va tras un trait (`MemStore` para pruebas + `SqliteStore` con `rusqlite`); Fase 1 sin observador.

## Update 05.1 — Acceso directo «Buscar en todos los archivos»
**Cambio:** botón permanente en la barra superior (lupa + texto) que abre la búsqueda global existente con el cursor en el campo. `Ctrl+Mayús+F` y el icono de globo del campo usan la misma acción. No se tocó el motor, SQLite, el esquema ni las dependencias.
**Arquitectura:** `useSearch.openGlobal()` (store) = `setScope("global")` si hace falta + `focusTick++` (conserva texto y filtros; si ya está abierta, solo vuelve a enfocar). `Toolbar` enfoca/selecciona `#nova-filter` cuando cambia `focusTick`. Volver: Esc (vacía y luego sale), el globo, o `leave()` desde la vista; el estado del índice vive en `useIndex` y no se toca.
**Archivos:** modificados `src/stores/search.ts`, `src/App.tsx`, `src/components/Toolbar.tsx`, `src/styles/app.css`; nuevo `src/stores/search.test.ts`; documentación.
**Responsive:** texto visible desde 1240 px de ancho del panel principal; por debajo queda la lupa (con `aria-label`/tooltip). Campo global: 320 px → 190 px (≤900 px) → 130 px (≤640 px). La ventana mínima de la app es 640 px.
**Verificación REAL (Linux; sin Windows, sin Vitest, sin npm):**
| Qué | Resultado |
|---|---|
| `search.test.ts` (5): `openGlobal` activa el ámbito global y pide foco; repetirlo conserva texto/filtros y vuelve a pedir foco; salir y reabrir; botón, atajo y globo comparten `openGlobal` y no queda `setScope("global")` en `App.tsx`; el botón no usa `tb-hide-narrow` y solo oculta el texto | 5/5 con arnés propio (no Vitest). Mutación: devolver el atajo a `setScope("global")` hace fallar la prueba de cableado |
| Maquetación con el CSS REAL (`tokens/themes/app.css`) y una maqueta HTML de la barra (no la app): 5 temas × {carpeta, global} × anchos 540–1800 px | 0 desbordes, 0 solapes, botón siempre visible y dentro de la barra, migas ≥ 80 px |
| Resto de pruebas TS (arnés propio) | todas pasan salvo limitaciones del arnés: `ops.test` no carga (`vi.mock`) y 1 de `thumbService` (`.resolves`); ambas ajenas a este cambio. `themes.test` 8/8 con el CSS actual |
| Núcleo Rust del índice | 26/26 (sin cambios) |
| Tipos | sin errores propios (con dependencias de React/zustand ausentes) |
**NO verificado (hay que hacerlo en Windows):** que el clic en el botón abra la vista y deje el cursor en el campo con la app real (el efecto de foco está escrito y su cableado probado, pero no se ejecutó en un navegador/Tauri); que el atajo siga funcionando dentro de la app; aspecto del botón en los 6 temas dentro de la app; `npm test`, `npm run typecheck`, `npm run build`, `npm run tauri build`.

## Qué hace la Fase 1 (y qué no)
| Requisito | Estado |
|---|---|
| Índice SQLite persistente (nombre, ruta, extensión, tipo, tamaño, fechas; archivos y carpetas; varias unidades) | ✅ |
| Indexado inicial en segundo plano, por lotes y transacciones, pausable/cancelable, sin bloquear la interfaz | ✅ (probado en Linux; interfaz no ejecutada en Windows) |
| Sin duplicados (clave única por ruta normalizada, upsert) | ✅ |
| Borrados/movidos/renombrados | ✅ por **reconciliación**: cada reescaneo marca con una «generación» lo visto y barra lo demás |
| Persistencia tras reinicio; recuperación si se cierra durante el indexado | ✅ (una unidad no terminada se reanuda; archivo corrupto/versión distinta se recrea) |
| Unidades: detección de fijas, desconectadas se ocultan sin perder el índice | ✅ |
| Exclusiones configurables (rutas, nombres, carpetas solo en raíz, extensiones); por defecto Windows, ProgramData, Recovery, `$Recycle.Bin`, System Volume Information, `node_modules`… | ✅ |
| Errores de acceso registrados sin detener el indexado; sin elevar privilegios; ciclos por enlaces simbólicos evitados | ✅ |
| Filtros combinables (nombre exacto/parcial, extensión, tipo, tamaño mín/máx, fechas, unidad, carpeta, archivos/carpetas) directos sobre el índice | ✅ |
| Relevancia, paginación, cancelación de consultas, debounce, virtualización, doble clic, abrir ubicación | ✅ lógica probada; **interfaz React no ejecutada** |
| Panel de estado (estado, unidades, totales, progreso aproximado, errores, última actualización, pausar/reanudar, reconstruir, exclusiones) | ✅ `IndexDialog.tsx`; **no ejecutado** |
| Actualización en tiempo real con `ReadDirectoryChangesW` | ❌ **Fase 2**. Hoy: reconciliación al arrancar, cada 10 min se comprueba qué toca y por antigüedad (ajuste «minutos», 6 h por defecto) |
| Búsqueda de contenido | ❌ fuera de alcance (según el encargo) |
| Miniaturas en resultados | No se piden durante el indexado; los resultados usan el icono (`EntryIcon`) |

## Arquitectura
`crates/nova-core/src/index/` (sin Tauri; se prueba con `cargo test`):
`model` tipos serializables · `classify` claves de ruta/extensiones/categorías · `exclude` reglas · `store` trait `IndexStore` + `MemStore` (referencia semántica) · `sqlgen` esquema y consultas (puro) · `sqlite` `SqliteStore` (feature `sqlite`; dos conexiones, escritor y lector, en WAL) · `scanner` recorrido por lotes · `engine` `Indexer` (hilo `lfind-index`, prioridad baja, estado, pausa, reconstrucción, reconciliación, búsqueda).
`src-tauri/src/index_cmd.rs`: abre `app_local_data_dir()/index/lfind-index.db`, guarda ajustes en la tabla `meta` (clave `settings`), comandos `index_search/status/get_settings/set_settings/available_drives/start/pause/resume/rebuild`. Un hilo revisa cada 10 min. Una búsqueda nueva cancela la anterior (bandera `AtomicBool` + `progress_handler` de SQLite). Si el índice no abre, la aplicación arranca igual con la búsqueda desactivada.
Interfaz: `SearchView.tsx` (Ctrl+Mayús+F; integrado en `Pane`/`Toolbar`/`StatusBar`), `IndexDialog.tsx`, `stores/search.ts`, `stores/indexStatus.ts`, `lib/searchController.ts` (debounce, orden de respuestas, cancelación, reintento de «cancelada» una vez), `lib/searchQuery.ts` (filtros → consulta). Tipos espejo en `lib/fs/types.ts`; `browser.ts` (modo navegador) devuelve «no disponible».

## Flujo de indexación
`start(raíces, reglas, opciones)` → hilo → por cada raíz: ¿en línea? → ¿completa y reciente? (si sí, se omite salvo `force`) → `begin_scan` (nueva generación) → recorrido por lotes (upsert por transacción, cede CPU `yield_ms`) → `finish_scan` (barre `gen<>actual`, guarda totales y `last_complete`); cancelado/error → `interrupt_scan` (queda «interrumpida» y se retoma). Cambiar exclusiones o unidades fuerza una pasada completa.

## Esquema SQLite (`user_version` = 1)
`roots(id, path UNIQUE, state, gen, online, files, dirs, skipped, last_complete)` · `entries(id, root_id, key UNIQUE, path, name, name_lc, ext, kind, size, modified, created, accessed, depth, gen)` con índices `(root_id,gen)`, `(ext,size)`, `(modified)` · `issues(id, root_id, path, message)` · `meta(key,value)` · `entries_fts` FTS5 `trigram` sobre `name_lc` (contenido externo + disparadores) para subcadenas de ≥3 caracteres; con 1–2 caracteres o sin FTS5 se usa `instr()`. Versión de esquema distinta → se descartan y recrean las tablas.

## Dependencias nuevas
`rusqlite 0.32` (`bundled`, `hooks`) en `nova-core`, **opcional** tras la característica `sqlite`; `src-tauri` la activa. Necesita compilador de C (ya exigido por Tauri con MSVC). Ningún paquete npm nuevo. **`Cargo.lock` NO está actualizado** (no hay crates.io aquí): en tu equipo `cargo build` lo regenera; conserva el resultado.

## Archivos
Nuevos: `crates/nova-core/src/index/*` (10 archivos, incluye `tests.rs`), `crates/nova-core/examples/index_xcheck.rs`, `scripts/index_xcheck.py`, `scripts/index_bench.py`, `src-tauri/src/index_cmd.rs`, `src/components/{IndexDialog,SearchView,Modal}.tsx`, `src/lib/{searchController,searchQuery}.ts` (+ `.test.ts`), `src/stores/{search,indexStatus}.ts`, este documento.
Modificados: `crates/nova-core/{Cargo.toml,src/lib.rs}`, `src-tauri/{Cargo.toml,src/lib.rs}`, `src/{App.tsx,components/{Dialogs,Pane,StatusBar,Toolbar,icons}.tsx|ts,lib/fs/{browser,tauri,types}.ts,stores/ui.ts,styles/app.css}`, `src/lib/themes.test.ts` (04.3.3), documentación. `app.css`: solo tokens de tema (sin colores a mano; 62 variables×5 temas intactas).
Corregido en esta sesión: `SearchView.tsx` importaba `basename` sin usarlo (con `noUnusedLocals` habría roto `npm run build`).

## Pruebas y resultados REALES (Linux; sin Windows, sin crates.io, sin npm)
| Qué | Cómo | Resultado |
|---|---|---|
| Núcleo del índice: inicial, incremental, duplicados, borrados, renombrado/movimiento, consultas parciales, filtros combinados, varias unidades/desconectada, raíz inaccesible, persistencia y reinicio, interrupción, cancelación, páginas sin solapes, exclusiones, ciclos de enlaces, pausa/reanudar/parar, reconstrucción | `rustc --test --cfg nova_noserde` sobre `src/index` (MemStore) | **26/26** |
| SQL real: el SQL de `sqlgen` ejecutado en SQLite 3.53.1 (FTS5 trigram disponible) frente a `MemStore`, 30 000 entradas | `index_xcheck` (Rust) + `scripts/index_xcheck.py` | **180 casos coinciden, 0 difieren**; barrido, borrado de árboles y persistencia al reabrir OK |
| Lógica TS (searchController 14, searchQuery 23, y las demás de `lib/`) | compilada con `tsc --noCheck` y ejecutada con un **arnés propio** (no es Vitest) | todas pasan; `iconService`/`thumbService` no se pudieron cargar (falta `@tauri-apps/api`) |
| Tipos de producción | `tsc` con tipos de React/zustand ausentes | solo ruido por dependencias ausentes; tras quitar `basename`, sin errores propios (TS6133 etc.) |
| `cargo test` con `rusqlite`, `SqliteStore`, `src-tauri`, `npm test`, `npm run typecheck`, `npm run build`, `npm run tauri build`, la interfaz en ejecución | — | **NO ejecutados** |

### Rendimiento medido (SQLite real, datos sintéticos controlados, caché caliente, ESTE servidor Linux; NO son cifras de Windows)
1 000 000 de filas con FTS: ingesta 103,9 s (≈9 600 filas/s), archivo 348 MB (+62 MB WAL).
| Consulta | mediana |
|---|---|
| «foto» (61 resultados mostrados, ordenado por relevancia) | 149 ms |
| «foto vacaciones» | 37 ms |
| «fo» (2 caracteres, sin FTS) | 255 ms |
| «a» (1 carácter) | 578 ms |
| extensión PSD | 135 ms |
| PSD de más de 1 GB | 0,1 ms |
| «info» + rango de fechas | 144 ms |
| carpeta (subárbol) | 621 ms |
| «e», hasta 5 000 resultados | 569 ms |
| sin texto: lo más reciente | 1,4 ms |
| exacta «informe.pdf» | 0,1 ms |
**Lectura honesta:** con 1 M de entradas, las consultas por nombre común NO son instantáneas (100–600 ms); las exactas y las de filtros estructurados sí lo son. Con índices más pequeños el coste baja, pero no hay medición de eso ni de Windows. Posible mejora (no hecha): limitar/estimar relevancia sin recorrer todos los candidatos, precalcular rangos por prefijo y un tope de candidatos para 1–2 caracteres.

## Comandos para continuar (otro equipo)
```powershell
npm install            # no `npm ci` hasta regenerar package-lock.json
npm test ; npm run typecheck ; npm run build
cargo test --manifest-path crates/nova-core/Cargo.toml --features sqlite   # regenera Cargo.lock; prueba SqliteStore
cargo test --manifest-path crates/nova-core/Cargo.toml
npm run tauri build
```
Banco SQL (opcional): `cargo run --manifest-path crates/nova-core/Cargo.toml --example index_xcheck -- <dir>` y `python scripts/index_xcheck.py <dir> [--bench]`; `python scripts/index_bench.py <dir> [filas]`.

## Limitaciones conocidas / pendiente
1. **Sin observador en tiempo real** (Fase 2): un archivo nuevo aparece en la siguiente reconciliación, no al instante. Está preparada `remove_tree` para los eventos.
2. Nunca compilado ni ejecutado en Windows: `src-tauri` (`index_cmd.rs`), `SqliteStore` con `rusqlite`, `detect_fixed_drives`, prioridad de hilo, interfaz.
3. Sin soporte de MFT/USN (alternativa más rápida para NTFS, requeriría privilegios elevados: descartada por la regla de no pedir administrador).
4. Fechas de acceso dependen de que Windows las mantenga actualizadas.
5. Cifras de rendimiento solo de Linux; repetir en el equipo de destino.
