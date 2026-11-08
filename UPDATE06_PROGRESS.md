# UPDATE06_PROGRESS · Estabilidad y fiabilidad (06.0)

Base: auditoría Fase 0 (2026-11-08). Actualización pequeña, previa al visor inteligente. **No incluye el visor.** Sin commits, sin dependencias nuevas, `Cargo.lock` y `package-lock.json` intactos.

## 1. Las dos pruebas Rust que fallaban

| Prueba | Qué comprueba | Por qué fallaba | ¿Dónde estaba el error? |
|---|---|---|---|
| `safety::tests::parse_rejects_bad_input` | `parse_path` rechaza entradas peligrosas y normaliza `.` | Usaba `/a/./b`, que en Windows **no es absoluta** (no tiene unidad) | En la prueba, que solo valía para POSIX |
| `thumbs_win::tests::iid_matches_the_documented_guid` | El IID de `IShellItemImageFactory` es el documentado | El formato imprimía 3 grupos y comparaba con el GUID de 5 | En la prueba |

**No hay problema de seguridad ni de funcionamiento.** Sonda con el `safety.rs` real en Windows: `..` se rechaza en rutas de unidad, con `/`, UNC, `\\?\` y `\\?\UNC\`; `.` se normaliza; `/a`, `\a` y `C:foo` se rechazan por no ser absolutas.

**Hallazgo adicional:** la prueba antigua tenía un punto ciego. En Windows, `/a/../etc` se rechazaba por «no absoluta», de modo que el caso de `..` no comprobaba `..`.

**Corrección:** rutas de ejemplo absolutas en cada sistema, la expectativa exige el motivo correcto (`«..»`) y hay una prueba nueva solo de Windows con todas las formas de ruta. En `thumbs_win` se imprime el GUID completo; el valor esperado no cambia. No se tocó código de producción.

**Comprobación por mutación** (copia aislada): dejar pasar `..` hace fallar 2 pruebas; alterar el GUID hace fallar 1.

**Observación, sin cambios:** `parse_path` acepta el espacio de dispositivos (`\\.\PhysicalDrive0`). Ver R-26 en `PROJECT_STATUS.md`.

## 2. ErrorBoundary

| Nivel | Dónde | Efecto |
|---|---|---|
| `app` | `main.tsx`, envolviendo `App` | Última red. Pantalla completa con franja para mover la ventana, Reintentar, Recargar y Cerrar |
| `view` | `Pane`, alrededor del contenido | Barra de título, panel lateral y barra de herramientas siguen funcionando |
| `panel` | `Pane`, alrededor de la vista previa | Un archivo problemático no afecta a la lista |

- **Recuperación segura:** tras 3 fallos en 30 s deja de ofrecer «Reintentar» (`lib/errorRecovery.ts`). Cambiar de carpeta o de elemento reintenta solo. A nivel `app`, reintentar cierra antes diálogos y menús, que suelen ser la causa.
- **Ventana oculta:** si falla el primer render, `App` no llegó a mostrar la ventana; el nivel `app` la muestra sin esperar al respaldo de 6 s.
- **Registro:** `log_ui_error` escribe en `lfind-errors.log` mensaje y pilas, acotados. La interfaz nunca recibe el detalle: el componente no guarda el error en su estado.
- **Sin duplicar:** no sustituye a `toast`/`errMsg`, que siguen tratando errores de eventos y promesas. En la consola sigue informando React.

## 3. Estrategia de pánicos

**Implicaciones comprobadas de `panic = "abort"`:** solo afectaba a release. Hacía inútil cualquier `catch_unwind`, y además desarrollo y producción se comportaban distinto.

**Por qué no bastaba con cambiarlo a `unwind` sin más.** Un pánico en otros puntos dejaría estados engañosos:
- El hilo del indexador moriría con `running = true`: quedaría bloqueado para siempre aparentando indexar.
- Un `Mutex` envenenado haría fallar después cada `lock().unwrap()` (61 usos entre `commands`, `index_cmd`, `engine`, `sqlite`, `store`).
- En miniaturas, un líder muerto dejaría a los que esperan colgados.

**Decisión: `unwind` en compilación + fallar rápido por defecto.**
- El gancho `diagnostics::install_panic_hook` registra todo pánico y **aborta si no está dentro de `isolate`**. Fuera de las fronteras el comportamiento es el de antes, pero ahora queda constancia.
- `diagnostics::isolate` convierte un pánico en `Err`. Hoy hay **una sola frontera**: `ThumbService::compute`, donde `image` interpreta archivos ajenos. El resultado es un `ThumbError::Failed` visible y recordado 5 min, no un éxito aparente.
- Sin `unsafe`. Una aserción en compilación impide volver a `panic = "abort"` sin darse cuenta.

**Coste medido:**

| | Antes | Después | Diferencia |
|---|---|---|---|
| `nova-explorer.exe` | 8 113 152 B | 13 184 000 B | **+5,07 MB (+62 %)** |
| Instalador NSIS | 2 878 891 B | 3 589 423 B | **+0,71 MB (+25 %)** |

Se atribuye casi por completo al desenrollado, pero no se aisló con una compilación de control. No se midió el efecto en memoria. Es reversible: quitar la frontera y la aserción, y restaurar la línea en `src-tauri/Cargo.toml`.

**Lo que `isolate` NO contiene:** falta de memoria, desbordamiento de pila y fallos de código nativo en el proceso, incluidos los manejadores de miniaturas de terceros (`ShellProvider`, COM). El cuerpo del hilo de `thumbs_win` tampoco está dentro de la frontera (no interpreta datos en Rust; un pánico ahí registra y aborta).

**Para el visor:** decodificadores en Rust dentro de `isolate`; bibliotecas nativas (PDF, por ejemplo) en un **proceso aparte**, porque `isolate` no las protege.

## 4. Verificación

| Comprobación | Línea base (Fase 0) | Update 06.0 |
|---|---|---|
| `nova-core` | 125/127 | ✅ **137/137** (134 + 3 de integración) |
| `nova-core --features sqlite` | 125/127 | ✅ **137/137** |
| Frontera en perfil release (`--release --test panic_boundary`) | — | ✅ 3/3 |
| Vitest | 190/190 | ✅ **205/205** |
| `npm run typecheck` | sin errores | ✅ sin errores |
| `npm run tauri build` | no ejecutado | ✅ correcto, sin avisos, 4 min 31 s |
| Paridad de comandos | 48 = 48 | ✅ 49 = 49 |
| Arranque del binario nuevo | — | ✅ vivo a los 2 s; sin entradas en el registro. Se cerró antes de los 3 s del indexador |

**Mutación:** quitar la frontera de `compute` hace fallar las 2 pruebas nuevas de miniaturas.

**Proceso real** (`tests/panic_boundary.rs`): pánico contenido → registrado y el proceso sigue; pánico fuera → registrado y termina con abort, no con el código 101 de una prueba.

**Ciclo real en Edge** (arnés fuera del proyecto, con el componente y el CSS reales): en los 3 niveles, fallo → pantalla → reintento → sin reintento tras repetirse → recuperación al cambiar de clave. Sin fugas de detalles técnicos. Capturas revisadas en dark, light, hacker y cyberpunk; generada también minimal.

**Pendiente, no ejecutado:**
- Provocar un error dentro de la app empaquetada (botones Cerrar y Recargar, escritura real de `lfind-errors.log` desde la interfaz, franja de arrastre).
- Un pánico real de `image` con un archivo malformado: no se dispone de uno.
- Uso real de LFind tras el cambio: solo se comprobó que arranca.
- Efecto del desenrollado en memoria y arranque.

## 5. Archivos

**Nuevos:** `crates/nova-core/src/diagnostics.rs`, `crates/nova-core/tests/panic_boundary.rs`, `src/components/ErrorBoundary.tsx` y su prueba, `src/lib/errorRecovery.ts` y su prueba, este documento.

**Modificados, código:** `crates/nova-core/src/{lib,safety,thumbs,thumbs_win}.rs`, `src-tauri/Cargo.toml`, `src-tauri/src/{lib,commands}.rs`, `src/main.tsx`, `src/components/{Pane.tsx,icons.ts}`, `src/lib/fs/{types,tauri,browser}.ts`, `src/styles/app.css` (solo bloque añadido).

**Modificados, documentación:** `PROJECT_CONTEXT.md`, `PROJECT_STATUS.md`, `PROJECT_ARCHITECTURE.md`, `DEVELOPMENT_GUIDE.md`, `CHANGELOG.md`, `WINDOWS_VERIFICATION.md`.

**Sin tocar:** motor de búsqueda, SQLite, indexador, navegación, menús, operaciones de archivo, temas. En miniaturas, solo la frontera de `compute`.
