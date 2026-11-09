# LFind · PROJECT_STATUS

Inventario real del proyecto. **Auditoría Fase 0 — 2026-11-08, actualizado con la Update 06.0.** Sustituye a la versión de la Fase 1.1, archivada en [docs/history/PROJECT_STATUS_fase-1.1.md](docs/history/PROJECT_STATUS_fase-1.1.md).

## Update 11 (personalización de barras)
Implementado y validado localmente, **sin commit ni publicación**. Rust 203+3 con SQLite y 188+3 sin él · Vitest 293/293 · TypeScript sin errores · instalador generado. Sin comprobar: arrastre con ratón físico. Detalle en [UPDATE11_PROGRESS.md](UPDATE11_PROGRESS.md).

## Update 10 (analizador de almacenamiento)
Implementado y validado localmente, **sin commit ni publicación**. Rust 200+3 con SQLite y 185+3 sin él · Vitest 280/280 · TypeScript sin errores · instalador generado. Probado en la app real con C: completa (2,34 M de archivos, 165 MB, bloqueo máximo de la interfaz 19 ms) y OneDrive sin hidratar nada. Sin comprobar: arrastre con ratón físico y discos extraíbles, de red o de Google Drive. Detalle en [UPDATE10_PROGRESS.md](UPDATE10_PROGRESS.md).

## Update 09 (navegación y organización)
Implementado, validado localmente y autorizado para su publicación. Rust 175+3 con SQLite y 174+3 sin él · Vitest 264/264 · TypeScript sin errores · instalador generado. Sin comprobar con hardware real: OneDrive de empresa y unidades extraíbles o de red. Detalle: [UPDATE09_PROGRESS.md](UPDATE09_PROGRESS.md).

## Update 08 (actualizaciones)

Implementado y probado localmente, con una actualización real en una instalación aislada. Rust 164/164 · Vitest 239/239. Activado y publicado: primera Release v0.1.1 generada por el workflow. Detalle: [UPDATE08_PROGRESS.md](UPDATE08_PROGRESS.md).

## Update 07 (visor inteligente)

Terminado con validación real en la app empaquetada. Rust 163/163 · Vitest 217/217 · tipos sin errores · instalador correcto · 51 = 51 comandos. Formatos, límites y pruebas pendientes: [UPDATE07_PROGRESS.md](UPDATE07_PROGRESS.md).

Riesgos nuevos o abiertos: **R-28** PDF con páginas gigantes (1 GB, 1 página): sin pintar a los 150 s y 713 MB en la webview · **R-29** sin gestión de color ICC en PSD y CMYK aproximado · **R-30** HEIC y parte de RAW dependen de proveedores de Windows. **R-26** queda cubierto en el visor: `viewer_open` rechaza rutas de dispositivo.

## Update 06.0 (estabilidad)

Resuelto: **R-13** (pruebas), **R-01** (errores de render) y, parcialmente, **R-02** (pánicos). Detalle en [UPDATE06_PROGRESS.md](UPDATE06_PROGRESS.md).

| Comprobación | Fase 0 | Update 06.0 |
|---|---|---|
| `nova-core`, con y sin SQLite | 125/127 | ✅ 137/137 |
| Vitest | 190/190 | ✅ 205/205 |
| Tipos | sin errores | ✅ sin errores |
| `npm run tauri build` | no ejecutado | ✅ correcto |
| Comandos Rust ↔ TS | 48 = 48 | ✅ 49 = 49 |

Las tablas de §1 conservan la foto de la auditoría; la vigente es esta.

**Leyenda de verificación**
- **E** · Ejecutado en esta auditoría (pruebas automáticas).
- **A** · Evidencia indirecta en artefactos reales de este equipo (binario, índice, cachés).
- **C** · Solo leído en el código: no se observó funcionando.

> La interfaz **no se abrió ni se manejó** en esta auditoría, para no lanzar otro indexado. Lo marcado **C** necesita comprobación manual; las listas están en `WINDOWS_VERIFICATION.md`.

## 1. Resultados reales de la verificación

| Comprobación | Comando | Resultado |
|---|---|---|
| Pruebas de frontend | `npx vitest run` | ✅ **190/190**, 19 archivos |
| Tipos (producción y pruebas) | `npm run typecheck` | ✅ Sin errores |
| Núcleo Rust | `cargo test … --locked --offline` | ❌ **125/127** |
| Núcleo Rust con SQLite | `cargo test … --features sqlite --locked --offline` | ❌ **125/127**, los mismos dos |
| Compilación de `SqliteStore` | `cargo check --features sqlite --locked --offline` | ✅ Compila |
| Paridad de comandos | Comparación `lib.rs` ↔ `tauri.ts` | ✅ 48 = 48 |
| El binario corresponde al código | Ningún archivo fuente más nuevo que `nova-explorer.exe` | ✅ **A** |

**Los dos fallos son errores de las propias pruebas, no del producto:**
- `safety::tests::parse_rejects_bad_input` (`safety.rs:118`): usa `/a/./b`, que no es absoluta en Windows.
- `thumbs_win::tests::iid_matches_the_documented_guid` (`thumbs_win.rs:212`): el formato solo imprime tres grupos y compara con el GUID completo. El GUID del código sí es el correcto.

No se ejecutaron `npm run build` ni `npm run tauri build`, para no generar otra versión. Ya existía una: `dist/` y el instalador son del 2026-11-07.

**Corrección de documentación anterior:** decía «nunca compilado en Windows». Es falso desde el 2026-11-07: existen `nova-explorer.exe` (8,1 MB), `LFind_0.1.0_x64-setup.exe` (2,9 MB) y un índice real generado por la app. También están actualizados `Cargo.lock` y `package-lock.json`.

## 2. Funcionalidades terminadas

«Terminada» significa código completo. La columna **Ver.** indica cuánto se ha comprobado.

| Funcionalidad | Dónde | Cómo funciona | Ver. |
|---|---|---|---|
| Ventana sin marco, Mica, arranque oculto | `lib.rs`, `App.tsx`, `boot.js`, `TitleBar` | Tema antes de pintar; se muestra al estar lista | A (la app se ha ejecutado), C (aspecto) |
| Pestañas e historial | `stores/nav.ts`, `TitleBar` | Historial por pestaña; salto por menú | E lógica, C interfaz |
| Lectura de carpetas | `dir.rs`, `useDirectory` | Lotes por `Channel`, cancelable | E núcleo, C interfaz |
| Vistas (detalles, lista, mosaicos, contenido, iconos) y agrupación | `FileView`, `layout.ts`, `view.ts` | Virtualizadas | E lógica, C interfaz |
| Orden | `sortCore`, `sort.worker` | Worker a partir de 3 000 | E lógica, C Worker en WebView2 |
| Selección, rectángulo, teclado, escritura rápida | `FileView`, `marquee.ts` | Por nombre; geometría calculada | E rectángulo, C resto |
| Barra de dirección | `Toolbar`, `addressBar.ts`, `pathbar.rs` | Valida en Rust con 6 s de límite | E lógica, C interfaz |
| Copiar, mover, papelera, eliminar, renombrar, crear | `ops.rs`, `stores/ops.ts`, `commands.rs` | Cola, conflictos, temporales | E lógica, C papelera real |
| Deshacer | `undo.rs` | Pila de 30 en memoria | E |
| Protección del sistema | `safety.rs` | Listas protegidas, validación | E (salvo el caso de R-13) |
| Arrastrar y soltar interno | `dnd.ts`, `useDropTarget` | Ctrl copia, Mayús mueve | C |
| Portapapeles con Windows | `clipboard*.rs`, `systemClipboard.ts` | `CF_HDROP` | E formato, C real |
| Menú contextual propio | `contextMenu.ts`, `ContextMenu.tsx` | Modelo puro, sin IPC | E modelo, C interfaz |
| Abrir, abrir con, terminal, mostrar en Explorador, accesos directos | `shell.rs`, `system_cmd.rs` | ShellExecute, PowerShell para `.lnk` | C |
| Enviar a, Nuevo | `sendto.rs`, `newmenu.rs` | Por identificador; `reg.exe` | E análisis, C real |
| Propiedades y estadísticas | `props.rs`, `Dialogs` | Recorrido cancelable | C |
| Favoritos, recientes, uso de discos | `Sidebar`, `prefs`, `volumes.rs` | Persistentes | C |
| Preferencias y vista por carpeta | `prefs.rs`, `viewState.ts` | JSON atómico, migración | E |
| Seis temas | `themes.css`, `themes.ts` | Contrato de variables | E contrato, C aspecto |
| Miniaturas | `thumbs*.rs`, `thumbService.ts` | Proveedores + cachés | E lógica; A 1 309 archivos en caché |
| Iconos del sistema | `icons*.rs`, `iconService.ts` | Lotes por tipo | E lógica; A caché de 2,2 MB |
| Visor inteligente (panel y vista ampliada) | `PreviewPane`, `components/viewer/*`, `nova-core::viewer` | Ver Update 07 | E, y batería en la app empaquetada |

## 3. Búsqueda global e indexación (Update 05)

| Aspecto | Estado | Ver. |
|---|---|---|
| Índice SQLite persistente | Terminado | **A**: 8 unidades, todas `complete` |
| Indexado en segundo plano, por lotes | Terminado | **A** |
| Reconciliación por generaciones | Terminado | E con `MemStore`; **A** `gen` 3–4 |
| Exclusiones configurables | Terminado | E reglas |
| Pausa, reanudar, reconstruir | Terminado | E con `MemStore`, C interfaz |
| Filtros, relevancia, paginación, cancelación | Terminado | E lógica TS y SQL (`sqlgen`), C interfaz |
| Panel de estado | Terminado | C |
| `SqliteStore` | Terminado, **sin pruebas propias** (R-12) | **A** por el índice real |
| Actualización en tiempo real | **Pendiente** (Fase 2) | — |
| Búsqueda por contenido | Fuera de alcance | — |

**Índice real de este equipo (consulta de solo lectura, 2026-11-08):**

| Métrica | Valor |
|---|---|
| Entradas | **2 904 460** (2 633 530 archivos + 270 930 carpetas) en 8 unidades |
| Tamaño | **1 891,5 MB** de base + 81,2 MB de WAL residual → **≈ 683 B por entrada** |
| Reparto | `entries` 992,5 MB · índice único de `key` 509,5 MB · FTS 255,4 MB · índices secundarios 134,1 MB |
| Carpetas omitidas | 26 (79 problemas registrados) |

## 4. Funcionalidades parciales y pendientes

**Parciales**
- **Acciones en resultados de búsqueda:** abrir, abrir ubicación, Explorador, copiar ruta y propiedades. Sin selección múltiple, copiar, cortar, renombrar ni eliminar.
- **Comprimir y extraer:** funcionan fuera de la cola, sin progreso ni cancelación; una extracción fallida puede dejar archivos parciales.
- **Visor:** EPS, RAW, HEIC y MOV con soporte parcial; el texto sigue limitado a 8 KiB; PDF sin miniaturas laterales ni selección de texto.
- **Accesibilidad:** roles ARIA presentes; los modales no retienen el foco.
- **Miniaturas en resultados:** solo iconos.

**Pendientes**
- Observador de cambios (`ReadDirectoryChangesW`), para carpetas abiertas y para el índice.
- Doble panel, duplicados, análisis de almacenamiento (mencionados en planes anteriores).
- Firma del instalador y actualizador.

## 5. Rendimiento y recursos

**Cifras reales conocidas**

| Medida | Valor | Origen |
|---|---|---|
| Binario / instalador | 15,5 MB / 5,6 MB (06.0: 13,2 / 3,6; Fase 0: 8,1 / 2,9) | Este equipo |
| JS / CSS de producción | 345 KB / 43 KB (sin comprimir) | `dist/` |
| Caché: miniaturas / iconos / WebView2 | 9,2 MB / 2,2 MB / 96 MB | Este equipo |
| Reescaneo de la unidad H: | ≈ 101 800 entradas en ≈ 95 s | Derivado de `last_complete` consecutivos; tipo de disco desconocido |
| Consultas sobre 1 M de filas sintéticas | 0,1–621 ms | Linux, Update 05; **no es Windows ni este índice** |
| Orden, serialización | 543 ms con 100 k en hilo principal | Linux, Fase 1.1 |

**No medido en Windows:** tiempo de arranque, CPU y RAM reales, carpetas de 100 k, consultas sobre este índice de 2,9 M, duración de un indexado inicial.

| Área | Riesgo | Cómo comprobarlo |
|---|---|---|
| Disco · indexado | R-06: cada reconciliación reescribe todas las filas | Bytes escritos por proceso durante una pasada (Resource Monitor) |
| Disco · espacio | R-07: 683 B por entrada | `dbstat` tras cambiar el esquema |
| Inicio | R-08: SQLite abre en el hilo principal | Marcas de `perf.ts` + cronómetro en `setup`, con y sin WAL grande |
| Búsqueda | R-05 y coste lineal de la relevancia | `elapsedMs` en la barra de estado, con «a», «fo», «foto», indexando y en reposo |
| Carpetas grandes | R-15: copia completa cada 60 ms | Carpeta de 100 k + perfil de DevTools |
| Renderizado | Por selección: `Pane` recalcula y filtra | Perfil de React con Ctrl+A en 50 k |
| RAM · miniaturas | R-16: hasta 3 imágenes de 100 MP a la vez | Memoria de trabajo con varias fotos enormes |
| RAM · interfaz | `data:` URL en memoria (400 miniaturas + iconos) | Instantánea del montón en DevTools |
| Segundo plano | Sondeo cada 1,5 s + reintento de búsqueda cada 3 s | CPU durante una pasada |

## 6. Problemas detectados y riesgos

Prioridad según impacto en estabilidad, datos y ligereza. Verificado leyendo el código salvo indicación.

### Alta
| ID | Problema | Dónde | Consecuencia |
|---|---|---|---|
| ~~R-01~~ | **Resuelto en 06.0:** `ErrorBoundary` a tres niveles | `main.tsx`, `Pane.tsx` | Errores de eventos y promesas siguen fuera (no dejan la ventana en blanco) |
| R-02 | **Parcial en 06.0:** decodificación de miniaturas aislada; cualquier otro pánico registra y cierra a propósito | `diagnostics.rs`, `thumbs.rs` | Fallos nativos y falta de memoria siguen cerrando la app. Coste: +5 MB de ejecutable |
| R-06 | Cada pasada reescribe todas las filas y el índice `(root_id, gen)` | `sqlgen.rs:65-71` | Escritura masiva en cada reconciliación, por defecto cada 6 h con la app abierta |
| R-04 | Sin instancia única; dos indexadores cruzan generaciones | `lib.rs`, `sqlite.rs:152` | `finish_scan` de uno puede borrar filas marcadas por el otro |
| R-07 | La ruta se guarda 3 veces: `path`, `key` y su índice único | `sqlgen.rs:21-36` | La tabla y el índice de `key` ocupan juntos el 79 % de los 1,89 GB |

### Media
| ID | Problema | Dónde | Consecuencia |
|---|---|---|---|
| R-03 | Si falla el `read_dir` de la raíz se cuenta como omitida, no como error, y el barrido borra todo lo de esa unidad | `scanner.rs:87`, `engine.rs:208` | Índice de la unidad vaciado hasta la próxima pasada. También afecta a subcarpetas con fallo transitorio |
| R-05 | Buscar usa la conexión **escritora** para `roots()`, contra lo que indica el diseño | `engine.rs:275`, `sqlite.rs:96` | La búsqueda espera a los lotes y al barrido final |
| R-08 | SQLite abre de forma síncrona en `setup`, con recuperación de WAL y escritura de `user_version` | `lib.rs:24`, `sqlite.rs:53-70` | Puede retrasar el arranque |
| R-09 | Sin `journal_size_limit` ni cierre ordenado | `sqlite.rs:56` | WAL residual de 81 MB observado |
| R-10 | Con «0 minutos», la interfaz promete «solo al abrir LFind», pero el motor **no reconcilia nunca** | `engine.rs:187`, `IndexDialog.tsx:182`, `model.rs:226` | Índice que envejece sin aviso |
| R-11 | «Se retoma sola» en realidad reinicia la pasada desde cero | `IndexDialog.tsx:12`, `engine.rs:197` | Coste alto con millones de entradas |
| R-12 | No hay pruebas de `SqliteStore` | `index/tests.rs` usa `MemStore` | La capa real solo se valida con uso real y `index_xcheck` |
| ~~R-13~~ | **Resuelto en 06.0:** `npm run test:core` en verde en Windows | `safety.rs`, `thumbs_win.rs` | — |
| R-14 | Sin observador de cambios | — | Carpeta e índice desactualizados |
| R-15 | `all.slice()` y recálculo completo en cada lote | `useDirectory.ts:49`, `Pane`, `layout.ts` | Coste cuadrático en carpetas enormes |
| R-16 | Miniaturas: 100 MP a 3 en paralelo, sin decodificación reducida | `thumbs.rs:140,169,478` | Picos de más de 1 GB posibles |
| R-17 | Un hilo de Shell colgado libera el permiso pero sigue vivo | `thumbs_win.rs:183-192` | Acumulación de hilos con un manejador defectuoso |

### Baja
| ID | Problema | Dónde |
|---|---|---|
| R-18 | Atajos globales activos con modal abierto; `ConflictDialog` no cuenta como diálogo; `Modal` sin retención de foco | `App.tsx`, `Pane.tsx:213`, `Modal.tsx` |
| R-19 | Comprimir/extraer bloquean un hilo sin cancelación | `archive.rs:77` |
| R-20 | `undo_last` saca la entrada antes de aplicar: si falla, se pierde | `commands.rs:341` |
| R-21 | `stop()` en la ventana entre `swap` y guardar el hilo pierde la cancelación; el ticker puede colarse en `rebuild` | `engine.rs:123-148,249` |
| R-22 | `open_dir` y `get_drive_usage` no pasan por `parse_path` (solo lectura) | `commands.rs:39,421` |
| R-23 | Worker expirado (15 s) → orden síncrono que congela la interfaz | `useSortedEntries.ts:33` |
| R-24 | Un archivo «demasiado grande» en caché se informa después como «sin proveedor» | `thumbs.rs:321,355` |
| R-25 | PowerShell con `-ExecutionPolicy Bypass` puede alertar a un antivirus | `shell.rs:59`, `sendto.rs:106` |
| R-26 | `parse_path` acepta el espacio de dispositivos (`\\.\PhysicalDrive0`). Solo alcanzable con IPC manipulado; las operaciones de archivo fallarían, pero conviene rechazarlo al añadir lectura de contenido | `safety.rs:7` |
| R-27 | Un pánico fuera de frontera cierra la app sin avisar al volver a abrirla; el motivo solo está en `lfind-errors.log` | `diagnostics.rs` |

## 7. Deuda técnica

| ID | Deuda |
|---|---|
| D-01 | Documentación obsoleta: `README.md` dice «Fase 1» y «módulos reservados»; `CHANGELOG.md`, `WINDOWS_VERIFICATION.md` y `UPDATE05_PROGRESS.md` dicen que no se compiló. Se conservan como historial |
| D-02 | Listas duplicadas sin prueba de paridad: categorías (`classify.rs` ↔ `fileKinds.ts`, hoy idénticas), texto previsualizable (`preview.rs` ↔ `thumbKinds.ts`), comprimidos (`archive.rs` ↔ `contextMenu.ts`), temas (`boot.js` ↔ `themes.ts`). Miniaturas e iconos sí tienen prueba |
| D-03 | Código sin uso: `search.rs`, 8 iconos exportados (`MonitorCog`, `Moon`, `Sun`, `ChevronDown`, `Layers`, `Columns3`, `Folder`, `ImageOff`), extracción de iconos de carpeta en `icons.rs`, `first` en `Dialogs.tsx:147` |
| D-04 | Utilidades repetidas: `expand_env` (`pathbar`, `newmenu`), `ms()` (varios), claves de ruta en TS (`pathKey`, `folderKey`, `samePath`) |
| D-05 | `thumbService.test.ts:53` tiene una aserción sin `await` que fallará con Vitest 3 |
| D-06 | Controles nativos en filtros e índice sin integrar con el tema; 140 `px` literales en `app.css` |
| D-07 | Tipos Rust↔TS mantenidos a mano |
| D-08 | La versión sigue en 0.1.0 pese a las actualizaciones |

## 8. Riesgos para las próximas actualizaciones

- La CSP y la falta de protocolo de assets impiden mostrar archivos grandes, vídeo, audio o PDF sin cambios de configuración.
- `data:` por IPC no escala a imágenes a resolución completa.
- Todo decodificador nuevo en Rust debe ir dentro de `diagnostics::isolate`; las bibliotecas nativas, en un proceso aparte (R-02). `parse_path` debería rechazar dispositivos antes de leer contenido (R-26).
- El cambio de esquema del índice (R-06, R-07) debe subir `SCHEMA_VERSION`, lo que **reconstruye** el índice en todos los equipos.

## 9. Lista priorizada recomendada

1. ~~Corregir las dos pruebas (R-13).~~ Hecho en 06.0.
2. ~~`ErrorBoundary` y estrategia de pánicos (R-01, R-02).~~ Hecho en 06.0; para bibliotecas nativas hará falta un proceso aparte.
3. Instancia única (R-04).
4. Evitar reescrituras sin cambios y reducir duplicación de rutas (R-06, R-07).
5. Búsqueda sin pasar por el escritor (R-05) y apertura diferida de SQLite (R-08, R-09).
6. Corregir el barrido ante raíz fallida y «0 minutos» (R-03, R-10).
7. Pruebas de `SqliteStore` (R-12).
8. Medir en Windows la tabla de §5.
