# LFind · PROJECT_CONTEXT

> **Fuente principal de contexto técnico.** Última auditoría: **Fase 0 — 2026-11-08**, sobre el código de la rama `main` con cambios sin commitear (Update 05 + 05.1). Actualizado con **Update 06.0** (estabilidad; [docs/history/UPDATE06_PROGRESS.md](docs/history/UPDATE06_PROGRESS.md)) , **Update 07** (visor inteligente; [docs/history/UPDATE07_PROGRESS.md](docs/history/UPDATE07_PROGRESS.md)) y **Update 08** (actualizaciones; [docs/history/UPDATE08_PROGRESS.md](docs/history/UPDATE08_PROGRESS.md)).
> Documentos hermanos: [PROJECT_ARCHITECTURE.md](PROJECT_ARCHITECTURE.md) (módulos y flujos) · [PROJECT_STATUS.md](PROJECT_STATUS.md) (inventario, problemas y riesgos) · [DEVELOPMENT_GUIDE.md](DEVELOPMENT_GUIDE.md) (instalar, probar, compilar).
> El historial anterior, intacto: [docs/history/PROJECT_CONTEXT_hasta-05.1.md](docs/history/PROJECT_CONTEXT_hasta-05.1.md) y [docs/history/PROJECT_STATUS_fase-1.1.md](docs/history/PROJECT_STATUS_fase-1.1.md).

## REGLAS OBLIGATORIAS PARA FUTUROS DESARROLLOS

1. Leer primero `PROJECT_CONTEXT.md`.
2. Consultar `PROJECT_STATUS.md`.
3. Revisar los módulos relacionados con la tarea.
4. No asumir que la arquitectura sigue igual sin comprobarlo.
5. No modificar código ajeno a la funcionalidad solicitada.
6. No eliminar características existentes.
7. Priorizar rendimiento y estabilidad.
8. Reutilizar componentes y sistemas existentes.
9. Documentar cambios y dependencias.
10. Ejecutar pruebas y distinguir resultados reales de pruebas pendientes.
11. No realizar commits ni pushes sin autorización.
12. No sobrescribir cambios del usuario.

**Cómo aplicarlas en la práctica**
- El árbol suele tener trabajo sin commitear: mirar `git status` antes de editar y no descartar nada.
- Etiquetar siempre lo afirmado: *ejecutado*, *evidencia indirecta* o *solo leído en código*. Nunca dar por hecho que algo funciona porque el código existe.
- Antes de instalar dependencias, explicar por qué y pedir confirmación.
- Al cerrar cada actualización, actualizar `PROJECT_STATUS.md`, y también este archivo y `PROJECT_ARCHITECTURE.md` si cambió la arquitectura.
- No crear un documento por actualización ni por corrección: el estado va en `PROJECT_STATUS.md` y el contexto técnico aquí. Los informes antiguos están en `docs/history/`.

## 1. Qué es LFind

Explorador de archivos para Windows 10/11 x64, de escritorio, con identidad visual propia. Antes se llamaba *NOVA Explorer*: el nombre visible cambió, pero **se conservan a propósito** el identificador `app.nova.explorer`, las claves `nova.*` de `localStorage` y los nombres `nova-core` / `nova-explorer`, para no perder datos del usuario. No renombrarlos.

**Objetivos del producto:** rápido, ligero y estable · estética premium, minimalista y consistente · nunca bloquear la interfaz · nunca perder datos sin confirmación · no pedir privilegios de administrador.

## 2. Tecnologías (versiones instaladas, comprobadas en la auditoría)

| Capa | Tecnología |
|---|---|
| Escritorio | Tauri 2.12.5 (wry 0.57.1), WebView2, instalador NSIS |
| Backend | Rust stable (toolchain local 1.99.1; `rust-version` declarada 1.77.2 en `src-tauri`, 1.75 en `nova-core`) |
| Frontend | React 18.3.1, TypeScript 5.6.3 (estricto), Vite 5.4.21, zustand 5.0.16, @tanstack/react-virtual 3.14.23, lucide-react 0.460.0 |
| Persistencia | `prefs.json`; SQLite 3 incluido en el binario (rusqlite 0.32.1, FTS5 trigram); caché de miniaturas e iconos en disco |
| Imágenes | crate `image` =0.25.6 (versión fijada), proveedor de miniaturas de Windows por FFI |
| Papelera | crate `trash` 5.2.9 |
| Pruebas | Vitest 2.1.9, `cargo test` |

Tamaño del código: TypeScript 6 716 líneas + 1 999 de pruebas · CSS 954 · Rust 8 238 (`nova-core`, con pruebas) + 1 023 (`src-tauri`).

## 3. Estructura de carpetas

```
crates/nova-core/        Lógica pura en Rust, sin Tauri (se prueba con cargo test)
  src/dir.rs             Lectura por lotes y cancelable de carpetas
  src/ops.rs undo.rs     Copiar/mover/borrar/renombrar/crear y deshacer
  src/safety.rs names.rs Validación de rutas, carpetas protegidas, nombres
  src/thumbs.rs *_win    Miniaturas: proveedores, caché en disco, concurrencia
  src/icons.rs *_win     Iconos del sistema: lotes, caché
  src/preview.rs         Vista previa de texto (8 KiB)
  src/diagnostics.rs     Registro local de errores y frontera de pánicos (`isolate`)
  src/viewer/            Visor: mod.rs (decide y convierte), serve.rs (permisos y protocolo), psd.rs, embedded.rs (EPS y RAW)
  src/index/             Motor de búsqueda global (ver §7.3)
  src/clipboard*.rs archive.rs sendto.rs newmenu.rs shell.rs props.rs pathbar.rs places.rs volumes.rs prefs.rs sys.rs b64.rs
  src/search.rs          Marcador antiguo sin código (el motor real está en index/)
  examples/              bench_dir, bench_thumb, index_xcheck
src-tauri/               Capa fina: comandos, estado, permisos, ventana
  src/lib.rs             Arranque, protocolo `lfview` y registro de los 56 comandos
  src/update_cmd.rs      Actualizador: comprobar, descargar, cancelar, reiniciar
  src/commands.rs *_cmd.rs state.rs
  tauri.conf.json capabilities/default.json .cargo/config.toml
src/                     Interfaz
  main.tsx App.tsx       Arranque, atajos globales, tema
  components/            TitleBar Sidebar Toolbar Pane FileView SearchView PreviewPane ContextMenu Dialogs IndexDialog Modal Transfers StatusBar …
  hooks/                 useDirectory useSortedEntries useFileActions useThumb useSystemIcon useDropTarget
  stores/                zustand: nav prefs app ui ops clipboard shellData search indexStatus
  lib/                   Lógica pura y servicios: fs/ thumbService iconService searchController searchQuery contextMenu menus layout marquee sort* view* themes path …
  styles/                tokens.css themes.css app.css
public/boot.js           Aplica tema/Mica antes del primer pintado
scripts/                 Banco y validación SQL, iconos, limpieza de obsoletos
docs/                    CONTEXT_MENU.md, history/ (informes de cada actualización)
```

## 4. Dependencias

### npm
| Paquete | Uso | Valoración |
|---|---|---|
| `react`, `react-dom` | Interfaz | Esencial |
| `@tauri-apps/api` | IPC y ventana (solo en `lib/fs/tauri.ts` y `TitleBar`) | Esencial |
| `zustand` | Estado global | Esencial |
| `@tanstack/react-virtual` | Listas de archivos y de resultados | Esencial para el rendimiento |
| `lucide-react` | Iconos de interfaz, por importación directa en `components/icons.ts` | Esencial; no importar el paquete completo |
| `pdfjs-dist` 4.10.38 (versión fijada) | Visor de PDF y AI compatible. Se carga solo al abrir un PDF y corre en un Worker | Esencial para el visor: sin código nativo en el proceso principal |
| `vite`, `@vitejs/plugin-react`, `typescript`, `vitest`, `@types/*`, `@tauri-apps/cli` | Desarrollo | Esenciales |

### Rust
| Crate | Uso | Valoración |
|---|---|---|
| `tauri`, `tauri-build` | Aplicación | Esencial |
| `serde`, `serde_json` | IPC y `prefs.json` | Esencial |
| `dirs` | Carpetas conocidas (`places.rs`) | Esencial, pequeña |
| `image` (solo codecs jpeg/png/gif/bmp/tiff/webp/ico, sin rayon) | Miniaturas | Esencial; versión fijada a propósito |
| `rusqlite` (`bundled`, `hooks`, opcional tras `sqlite`) | Índice | Esencial; requiere compilador de C |
| `trash` | Papelera; solo en `src-tauri` | Esencial |
| `tauri-plugin-updater` 2 | Actualizaciones firmadas; solo en `src-tauri` | Esencial; oficial de Tauri |

**Sin duplicidades entre dependencias.** Las duplicidades son de datos: listas repetidas en Rust y TS (ver `PROJECT_STATUS.md`, D-02). La API de Windows se declara a mano por FFI en lugar de usar la crate `windows`: mantener esa decisión salvo necesidad clara.

**Relevantes para el futuro (NO instaladas):** el protocolo de assets de Tauri no requiere crate pero sí cambiar configuración y CSP; `tauri-plugin-single-instance` resolvería el riesgo R-04. Cualquier adición requiere confirmación (regla 9).

## 5. Decisiones arquitectónicas que hay que conservar

1. **`nova-core` no depende de Tauri.** La lógica de disco va ahí; `src-tauri` solo valida, saca el trabajo del hilo principal y adapta tipos.
2. **La interfaz nunca llama a Tauri directamente:** todo pasa por `FsProvider` (`src/lib/fs/`). `BrowserProvider` no inventa archivos.
3. **Rust valida todo lo que llega por IPC** (`safety::parse_path`, `check_destructive`, `validate_name`). Nunca se ejecuta una ruta recibida de la interfaz en «Enviar a» y «Nuevo»: se usa un identificador.
4. **Comandos con E/S: `async` + `spawn_blocking`.** Los comandos síncronos de Tauri corren en el hilo de la ventana.
5. **Trabajos largos: `Channel` + id + bandera de cancelación** (`Jobs`). Lectura de carpetas en lotes (primero 64, después 1 500); solo viaja el nombre.
6. **Listas siempre virtualizadas.** Orden en Worker a partir de 3 000 entradas. Durante la carga, orden de llegada; se ordena una vez al terminar.
7. **La ventana nace oculta** (`visible: false`) y se muestra con la interfaz lista; respaldos a 1,2 s (JS) y 6 s (Rust). `boot.js` evita el parpadeo de tema.
8. **Menú contextual propio y único** (decisión definitiva 04.2): ni `IContextMenu` ni extensiones COM. Abrirlo no hace IPC.
9. **Índice:** el almacén siempre tras el trait `IndexStore`; el SQL solo se genera en `sqlgen`; `rusqlite` opcional; `MemStore` es la referencia semántica. Modelo de generaciones para detectar bajas.
10. **Una sola vía para abrir la búsqueda global:** `useSearch.openGlobal()`.
11. **Sistema de temas por contrato** (§8). Ningún componente define colores propios.
12. **Eliminación optimista:** se oculta al confirmar y se restaura si falla. El aviso de éxito solo llega con el resultado real.
13. **Escritura segura:** copias a temporal + renombrado; `prefs.json` atómico con `.bak` si está corrupto.
14. **Miniaturas y vista previa no ejecutan contenido:** SVG solo en `<img>`, texto como texto, nunca HTML.
15. **Nunca pedir administrador.** Por eso no se usa MFT/USN.
16. **Pánicos: fallar rápido salvo en fronteras explícitas** (Update 06.0). Release compila con `panic = "unwind"`, pero el gancho de `nova_core::diagnostics` registra y aborta cualquier pánico fuera de `diagnostics::isolate`. Solo el código que interpreta contenido de archivos ajenos va dentro de una frontera. Restaurar `panic = "abort"` rompe la compilación a propósito.
17. **Errores de render: `ErrorBoundary` a cuatro niveles** (`app` en `main.tsx`, `view` y `panel` en `Pane`, `viewer` en `PreviewPane`). Registran el detalle en el log local y no muestran trazas al usuario.
18. **El contenido de archivos solo llega a la interfaz por el protocolo `lfview`, con un permiso por archivo** (Update 07). Nada de `assetProtocol` con scope amplio ni CSP relajada. El tipo lo fija el backend inspeccionando el archivo.
19. **Sin decodificadores nativos en el proceso principal para el visor** (Update 07). PDF.js corre en un Worker; PSD, EPS y RAW son Rust seguro bajo `isolate`. PostScript jamás se ejecuta.
20. **El panel y la vista ampliada son el mismo elemento DOM**: ampliar no recarga el documento.
21. **Actualizaciones solo por el plugin oficial, firmadas y con el consentimiento del usuario** (Update 08). La clave privada nunca entra en el repositorio. La versión se calcula en CI (`MAYOR.MENOR.<número de ejecución>`): no añadir commits automáticos de versión. Una release solo se publica con todos sus archivos verificados.

## 6. Convenciones de código

**General:** código, comentarios y mensajes de usuario en español. Comentarios que explican el porqué; sin relleno.

**TypeScript:** `strict`, `noUnusedLocals` activo. Lógica pura en `lib/` con su `*.test.ts`, sin React ni IPC. Stores con `create`; fuera de render usar `useX.getState()`. Datos serializados espejo de Rust en `lib/fs/types.ts` (`camelCase`).

**Añadir un comando (cinco lugares):** lógica en `nova-core` → `#[tauri::command]` en `src-tauri` → registrarlo en `generate_handler!` → método en `FsProvider` (`types.ts`) → implementarlo en `tauri.ts` y `browser.ts`. Si usa una API de ventana nueva, también `capabilities/default.json`.

**Rust:** errores legibles para el usuario (`ops::classify`), nunca `panic!` en rutas normales. FFI de Windows en archivos `*_win.rs` con `#[cfg(any(windows, nova_winck))]`. Hilos que puedan colgarse: con tiempo límite. Un decodificador nuevo se ejecuta dentro de `diagnostics::isolate`, sin tocar estado compartido, y un pánico se traduce en un error visible de ese archivo. Código nativo de terceros: `isolate` no lo protege (ver §10).

**Pruebas:** en Vitest, leer el CSS con `readFileSync`; `?raw` con `.css` devuelve una cadena vacía.

## 7. Sistemas principales (resumen)

Flujos completos y diagramas en `PROJECT_ARCHITECTURE.md`.

### 7.1 Navegación y carpetas
`useNav` guarda pestañas e historial. `Pane` usa `useDirectory(path)` **solo para la pestaña activa**, que recibe lotes por `Channel` y cancela la lectura anterior. `useSortedEntries` ordena; `FileView` virtualiza (5 modos, agrupación, selección por rectángulo, teclado, arrastre). No hay caché de listados ni observador de cambios: lo externo aparece con F5, al volver a pulsar la ubicación abierta o tras una operación.

### 7.2 Operaciones de archivo
`useOps` es una cola (2 en paralelo, deduplicada) → `start_op` → `nova-core::ops`. Conflictos resueltos por canal hacia la interfaz. Deshacer: pila en memoria de Rust (30 entradas, se pierde al cerrar).

### 7.3 Búsqueda global e índice
SQLite en `%LOCALAPPDATA%\app.nova.explorer\index\lfind-index.db`, WAL, conexión escritora y lectora.
- **Indexado:** hilo `lfind-index` de baja prioridad; empieza a los 3 s y comprueba cada 10 min. Lotes de 2 000. Cada pasada estrena una generación y al terminar barre lo no visto.
- **Reconciliación:** por defecto cada 360 min. **Es una pasada completa**, no incremental. No hay observador en tiempo real.
- **Búsqueda:** `sqlgen` genera SQL; FTS5 trigram con ≥ 3 caracteres, `instr()` con menos. Debounce de 120 ms, páginas de 200, tope de 5 000 en memoria. Una búsqueda nueva cancela la anterior en SQLite (`progress_handler`).
- **Exclusiones por defecto:** `$recycle.bin`, `system volume information`, `$winreagent`, `node_modules` a cualquier nivel; `windows`, `programdata`, `recovery`, `$windows.~bt`, `$windows.~ws`, `windows.old`, `config.msi`, `msocache` en la raíz. Ajustes en la tabla `meta`, clave `settings`.

### 7.4 Miniaturas, iconos y vista previa
Dos servicios y dos cachés. Frontend: colas con concurrencia limitada, época por carpeta, LRU. Backend: `RasterProvider` → `SvgProvider` → `ShellProvider`, caché con clave ruta+tamaño+fecha+px (300 MB). Iconos por lotes y clave compartida por tipo.

### 7.4 bis Visor inteligente (Update 07)
`viewer_open` inspecciona el archivo y devuelve una `Source`. `useViewerSource` la pide con 90 ms de espera y una petición nueva cancela la anterior. `PreviewPane` es el contenedor del panel y de la vista ampliada; `stores/viewer.ts` guarda el modo y la navegación.

| Ruta | Formatos | Cómo |
|---|---|---|
| Nativa | JPG, PNG, WebP, GIF, BMP, ICO, AVIF, SVG | WebView2 decodifica el original. Más de 48 MP o 128 MiB: se reduce en Rust |
| Convertida | TIFF, PSD/PSB, EPS (vista incluida), RAW (JPEG incrustado) | Rust seguro, 1 a la vez, lado máx. 3072 px, caché `viewer-v1` de 768 MB |
| PDF | PDF, AI con contenido PDF | PDF.js en Worker, lectura por rangos, solo páginas visibles |
| Multimedia | MP4, WebM, MOV; MP3, M4A, WAV, FLAC, OGG | Reproductor de WebView2 por rangos |
| Sistema | HEIC y lo que tenga proveedor de Windows | Miniatura real reutilizando `ThumbService`, 1024 px, con aviso |

Texto: los primeros 8 KiB, como antes.

### 7.4 ter Actualizaciones (Update 08)
`stores/update.ts` lleva el estado; `UpdateDialog` lo muestra; `src-tauri/src/update_cmd.rs` usa el plugin oficial. Publicación en `.github/workflows/release.yml` con la lógica en `scripts/release/`. Detalle: `docs/history/UPDATE08_PROGRESS.md`.

### 7.5 Persistencia
| Dato | Dónde |
|---|---|
| Preferencias | `%APPDATA%\app.nova.explorer\prefs.json` (versión 2, migración y saneado) |
| Índice y ajustes del índice | `%LOCALAPPDATA%\app.nova.explorer\index\` |
| Caché de miniaturas / iconos / visor | `%LOCALAPPDATA%\app.nova.explorer\thumbs`, `icons` y `viewer-v1` |
| Registro de errores (pánicos y errores de render, JSON por línea, 512 KiB + 1 rotado) | `%LOCALAPPDATA%\app.nova.explorer\logs\lfind-errors.log` |
| Tema y Mica para el primer pintado | `localStorage` (`nova.theme`, `nova.vibrancy`) |
| Pestañas, selección, deshacer | Solo en memoria |

### Navegación (Update 09)
Las rutas guardadas se comparan siempre con `canonicalKey`. «Mi equipo» es la ruta virtual `COMPUTER`: el código que recibe la ruta de una pestaña debe contemplarla. Ningún código nuevo debe abrir el contenido de un archivo sin comprobar antes `dir::is_cloud_only`, o hidrataría los archivos de OneDrive.

### Analizador de almacenamiento (Update 10)
Módulo `nova_core::analyzer`, separado del índice de búsqueda: `walk` recorre solo metadatos en un hilo de prioridad baja y `report` guarda cada análisis en `%LOCALAPPDATA%\app.nova.explorer\analyzer\<hash>.db`. Los archivos se guardan en orden de recorrido, de modo que una carpeta es un intervalo de identificadores. Un análisis a la vez; pausar, reanudar y cancelar; cancelar o fallar conserva el informe anterior. La interfaz solo recibe instantáneas de tamaño fijo y páginas. No sigue enlaces ni uniones y no provoca descargas de la nube. Rutas virtuales `lfind://storage` y `lfind://storage?root=…&at=…`. No lee ni escribe el índice ni cambia preferencias.

Orden: las fechas mezclan carpetas y archivos (`mixesFolders`); nombre, tamaño y tipo dejan las carpetas delante; `nextSort` alterna el sentido al repetir un criterio. Ubicaciones: el contenedor `.side-pins` resuelve la posición de inserción y la identidad es la ruta normalizada. Detalle en [docs/history/UPDATE10_PROGRESS.md](docs/history/UPDATE10_PROGRESS.md).

### Personalización de la interfaz (Update 11)
`useReorder` es el único motor de arrastre para reordenar (secciones, filas, herramientas y pestañas), con eventos de puntero; el arrastre HTML5 queda para mover archivos. La distribución vive en `Prefs.layout` (sigue la versión 3, campo con valor por defecto y lectura tolerante en `prefs.rs`): listas de identificadores estables, vacías por defecto. `lib/uiLayout.ts` define secciones y herramientas; una herramienta o sección nueva se añade ahí. El modo «Personalizar interfaz» (`stores/customize.ts`) no se guarda. Detalle en [docs/history/UPDATE11_PROGRESS.md](docs/history/UPDATE11_PROGRESS.md).

### Actualización de informes del analizador (Update 12)
Un informe no se modifica en su sitio: `Analyzer::update` escribe uno nuevo copiando del anterior (base adjunta) lo que no cambió y releyendo solo las carpetas afectadas, para conservar los intervalos contiguos de identificadores; la sustitución sigue siendo atómica. `Sink` tiene métodos opcionales para ello (`visit`, `copy_subtree`, `keep_files`). Hay dos fuentes de cambios: la lista exacta de `analyzer::watch` (`ReadDirectoryChangesW`, una ubicación, memoria acotada) y la comparación de fechas de las carpetas guardadas, que marca el informe como parcial. La fecha de cada carpeta se lee de la propia carpeta: la que da la enumeración de la madre puede ir retrasada. Antes de reescribir, los avisos de la vigilancia se comparan con el informe (`really_changed`) y los archivos avisados se leen directamente, porque la enumeración da tamaños retrasados si siguen abiertos. Modo en `Prefs.analyzerRefresh`.

### Doble panel (Update 12)
`useNav` mantiene `tabs` y `activeId` siempre del panel activo; el otro vive en `other` y `side` indica su lado. Todo lo que ya actuaba sobre «la pestaña activa» funciona sin cambios y `focusSide` intercambia ambos grupos. `Pane` y `Toolbar` reciben `side` y usan `selectPaneTab(side)`; los atajos, el visor y la búsqueda solo se enlazan en el panel activo. `id="nova-filter"` solo existe en el panel activo. Arrastrar entre paneles reutiliza `beginDrag` y `dropInto`. El orden propio de un panel vive en `Tab.orders`, por lo que viaja con la pestaña al intercambiar paneles; solo se usa si comparten carpeta o si ya lo tenía.

### Personalización visual (Update 12)
`Prefs.appearance` y `customThemes` se guardan como JSON y los valida `normalizeAppearance`, campo por campo. `appearanceVars` escribe variables sobre las del tema, solo las que difieren del original; al cambiar de tema se miden de nuevo. Las imágenes se reducen en la interfaz (máximo 1920 px) y las guarda `nova_core::backgrounds` con un identificador derivado del contenido; `prefs.json` solo guarda ese identificador. Las imágenes se eligen en el propio explorador (`useCustomize.picking`, `chooseBackground`); no hay `<input type="file">`. La CSP no cambia: se muestran como `data:`. El desenfoque y el contraste se aplican una vez con `bakeBackground`; el `filter` en vivo solo queda con `data-bg-filter`, si falla el lienzo. La opacidad de los paneles tiene un mínimo y la barra lateral siempre es más opaca. Usar `--bg-content`, no `--bg-chrome`, para velos: `--bg-chrome` es transparente con Mica.

### Herramientas (Update 13)
Ninguna mantiene procesos ni hilos mientras está inactiva. La paleta se amplía con `registerCommands`, y solo consulta a sus proveedores al abrirse. El panel activo se expone con `paneActions`. El inspector es una pestaña de `PreviewPane` y usa el sistema de propiedades de Windows, sin abrir lo que está en la nube. Todas las operaciones de archivo siguen en `stores/ops.ts`; la Papelera del detector pasa por ahí. El detector tiene base propia (`duplicates/duplicates.db`), para no competir con el índice, y `job.db` guarda en WAL un análisis en curso: etapa, avance y carpetas pendientes van en la misma transacción que los datos, de modo que cortar la luz solo pierde la última página. `cancel` tira el trabajo; `suspend` y cerrar LFind lo conservan, y solo `resume` lo continúa. Hasta 4 hilos en SSD y uno si algún origen es mecánico o de red. Un archivo se abre una sola vez y la verificación final no vuelve a abrirlo si ya se leyó. Las terminales usan ConPTY con un Job Object que cierra los procesos hijos, y dentro de `.term` no actúan los atajos globales. El indicador de ambos shells anuncia la carpeta con OSC 9;9, y la interfaz solo le hace caso si `terminalSync` está activo. La integración con Windows escribe solo en `HKCU`, siempre tras confirmación; `nsis-hooks.nsh` debe coincidir con `winshell.rs`, porque deja las carpetas operativas al desinstalar.

### Integración nativa con el shell (corrección de Update 13)
`shellapps.rs` concentra los mecanismos COM: `SHAssocEnumHandlers` e `IAssocHandler::Invoke` para «Abrir con», `SHOpenWithDialog` para «Elegir otra» y `SHDoDragDrop` para el arrastre. Nunca se pasa por CMD ni PowerShell. El arrastre empieza en `Pane.onDragStart`, solo con eventos `isTrusted`. El navegador sigue recibiendo el arrastre nativo, y los paneles leen las rutas de `draggedPaths`, así que también debe aceptarse el soltar en `.empty-wrap`. La Papelera es la ruta virtual `RECYCLE`, con `trash::os_limited`; el tamaño se lee de `$Recycle.Bin`, porque pedirlo al shell por elemento tardaba más de 10 s.

Windows disfraza los placeholders al enumerar: el analizador usa `sys::ExposePlaceholders` por hilo para distinguir lo local, lo que está solo en la nube y lo sincronizado sin hidratar nada. `writable()`, en `src-tauri/src/windows_cmd.rs`, rechaza las compilaciones de desarrollo: escribir su ruta en el Registro abre una consola y una página inaccesible en cuanto Vite no está en marcha. Los desplegables usan `components/Select.tsx`, no `<select>`, porque la lista nativa no respeta los temas.

### Barra lateral, Papelera y desenfoque por zona (Update 14)
Detalle y pruebas: [UPDATE14_PROGRESS.md](docs/history/UPDATE14_PROGRESS.md).

`SidebarResizer` escribe `--sidebar-w-open` una vez por fotograma y guarda `appearance.sidebarWidth` al soltar. Escucha en la ventana: no depender de la captura de puntero evita que un arrastre quede sin terminar.

Windows puede dar por buena una eliminación sin haber borrado lo que está en uso. **Nunca se puede dar por borrado sin volver a listar la Papelera.** Lo que explica un bloqueo pide solo el permiso de borrar, sin leer contenido, para no descargar lo que esté en la nube.

**Los diálogos Abrir y Guardar no se pueden sustituir.** Son un servidor COM dentro del proceso (`comdlg32.dll`), y la única forma de desviarlos es el secuestro de COM. No hay que intentarlo; ver el documento de progreso.

### Vistas, zoom y Papelera por lotes (Update 15)
Detalle y pruebas: [UPDATE15_PROGRESS.md](docs/history/UPDATE15_PROGRESS.md).

**Medidas.** `lib/viewGeometry.ts` e `iconGeometry` son la única fuente: el pintado y la selección por rectángulo las comparten. `iconCells` reparte el ancho sobrante entre las celdas, y `makeGeometry` debe usar el mismo cálculo.

**Zoom.** `iconSize` sigue siendo un entero en píxeles (32–256), y el 100 % equivale a 96 px. El control va de 0 a `LADDER_MAX`: 0 detalles, 1 lista, y de 2 en adelante iconos en escala logarítmica. Ctrl+rueda aplica como mucho un cambio por fotograma. `useZoomAnimation` mide antes de que React aplique el tamaño y anima solo `transform`, y no anima con más de 350 elementos a la vista.

**Trackpad (corrección para portátiles, [detalle](docs/history/LAPTOP_PROGRESS.md)).**
- **Pinzado.** Llega como Ctrl+rueda sintético, sin la tecla Ctrl: `FileView` sigue la tecla real con `keydown`/`keyup`. El pinzado usa `PINCH_GAIN` para que el tamaño siga a los dedos.
- **Tamaño exacto.** Se acumula con decimales (`nextExactSize`) y se redondea solo al pintar. Así no depende de cuántos eventos lleguen, y nunca cambia más de `MAX_FRAME_RATIO` por fotograma.
- **Animación.** Solo se anima un punto suelto de la rueda (`isWheelNotch` con una pausa previa de más de `GESTURE_GAP`). Un gesto continuo se sigue sin animar.
- **Ancla.** Se calcula con la distribución que estaba en pantalla, no con `getBoundingClientRect`, que incluye la animación en curso.
- **Inercia y cambio de vista.** La inercia sin Ctrl se descarta durante `GESTURE_TAIL`, y un gesto cambia de vista una sola vez hasta una pausa de `GESTURE_PAUSE`.

**Barra superior.** `--titlebar-h` es de 36 px, en píxeles CSS que escalan con el DPI como la barra nativa. No hay que fijarla en píxeles físicos. Con tamaños continuos, las filas se colocan con `rowTops` de la distribución actual, no con `vr.start`, que va un fotograma por detrás.

**Actualizar al reseleccionar.** `useNav.openLocation` es la entrada de los clics del usuario; `navigate` no actualiza por sí solo, para que las rutas programáticas no provoquen lecturas. `useDirectory` no publica una recarga silenciosa idéntica.

**Papelera.** `nova_core::recycle` usa `IFileOperation` con un receptor de progreso, porque notifica cada archivo dentro de una carpeta. Cada carpeta va en su propia operación y no se interrumpe: cancelar se aplica entre elementos, y si Windows deja de dar señales durante `STALL` el lote se da por perdido.

El desenfoque del fondo es **conjunto** (`Appearance.blur` y `surface`), y las preferencias con `zones` se migran desde la zona de contenido en `normalizeAppearance`.

### Auditoría final
Informe: [FINAL_AUDIT_REPORT.md](FINAL_AUDIT_REPORT.md). Reglas que deja:
- **La biblioteca `trash` no es fiable para saber qué pasó.** Un lote que falla a medias ya ha enviado el resto, así que `ops::trash_items` solo reintenta lo que sigue en su sitio. `recycle_restore` comprueba en disco que cada elemento haya vuelto. Los motivos se traducen con `recycle::trash_blocked_reason` y con los códigos `COPYENGINE_E_SHARING_VIOLATION_*`.
- **Todo proceso de consola lanzado desde LFind lleva `CREATE_NO_WINDOW`** (`0x0800_0000`), salvo los que abren una ventana a propósito: la terminal externa y los destinos de «Enviar a».
- **Integración con Windows:** al desactivar el explorador preferido se borra la copia del valor anterior, y `Software\LFind` se retira solo si queda sin valores ni subclaves (`imp::remove_if_empty`).
- **No cambiar `bundle.publisher` ni `identifier`.** El instalador NSIS guarda la carpeta de instalación en `HKCU\Software\<editor>\<producto>` (hoy `Software\nova\LFind`), y cambiar el editor haría que una actualización no la encontrara.

## 8. Reglas de diseño

LFind debe conservar una identidad **premium, minimalista, moderna y consistente**. Ningún componente nuevo puede tener una apariencia distinta al resto.

**Tokens**
- `styles/tokens.css`: lo que no cambia con el tema (escalas tipográficas `--fs-*`, espaciado `--sp-*`, tamaños de controles, movimiento, capas).
- `styles/themes.css`: 5 bloques (`dark`, `light`, `cyberpunk`, `hacker`, `minimal`) con **exactamente las mismas 62 variables**. `system` se resuelve a dark o light. `themes.test.ts` lo vigila.
- **Prohibido escribir colores a mano** en `app.css`; una prueba lo comprueba. La regla «tampoco tamaños a mano» es una aspiración: `app.css` tiene unos 140 valores `px` literales frente a 639 `var(--…)`. En código nuevo, usar tokens.
- Si un tema nuevo cambia la lista: añadirlo también en `lib/themes.ts`, en `public/boot.js`, en `index.html` (fondo inmediato) y en `prefs.rs`.

**Lenguaje visual**
| Elemento | Regla |
|---|---|
| Tipografía | Segoe UI Variable (`--font`, `--font-display`); Hacker es monoespaciada. Base 13 px; escala 11–16 px. Números con `tabular-nums` |
| Superficies | `--bg-chrome` (marco) → `--bg-content` (contenido) → `--bg-raised` (menús, modales, toasts). Campos con `--bg-field`. Mica opcional |
| Jerarquía de texto | `--text` / `--text-2` / `--text-3`. Títulos de sección en mayúsculas pequeñas con `--ls-caps` |
| Radios y sombras | Siempre `--r-*` y `--shadow-*`; cada tema tiene los suyos (Hacker casi recto) |
| Selección y foco | `--sel` + `--sel-edge`; foco `--focus-ring` de 1,5–2 px; resalte activo con `--accent-soft` |
| Iconografía | lucide con trazo 1,7–2 en `--icon-s/m/l`, importado en `components/icons.ts`. Carpetas: siempre `FolderGlyph`, nunca el icono de Windows |
| Movimiento | `--t-fast` (110 ms), `--t-med` (200 ms), `--ease`. Entradas sutiles (3–8 px, escala ≥ 0,97). Se respeta `prefers-reduced-motion` |
| Capas | `--z-marquee` < `--z-overlay` < `--z-menu` < `--z-modal` |

**Componentes que hay que reutilizar antes de crear otros:** `ErrorBoundary`, `Modal`, `.btn` (`primary`, `danger`), `.ibtn`, `.tbtn`, `ContextMenu` + `MenuItem`, `useUi().toast`, `EmptyState`, `EntryIcon`, `Thumb`, `.sv-seg`, `.sv-field`, `.check`, `.progress`, `.bar`, `.hint`, `.warn`, `.mono`.

**Responsive:** ventana mínima de 640×420. Se usan consultas de contenedor (`@container main` / `side`), no de ventana: barra lateral en modo raíl, textos de botón ocultos y campo de búsqueda que cede ancho.

**Geometría acoplada:** la altura de filas está duplicada en CSS y en `lib/viewGeometry.ts` / `view.ts`, para la virtualización y el rectángulo de selección. Si cambia una, debe cambiar la otra.

**Puntos débiles actuales (no imitar):** `<select>`, fecha y casillas nativos en filtros y `IndexDialog`; SVG ad hoc en `SearchView`.

## 9. Consideraciones de rendimiento

- No hacer E/S en el hilo principal ni en comandos síncronos.
- No añadir IPC por elemento: agrupar en lotes.
- No enviar imágenes grandes como `data:` por IPC.
- Mantener bajo el coste por render de `Pane` y `FileView`: `Item` está memoizado y usa callbacks estables por `useRef`.
- Cualquier trabajo en segundo plano: baja prioridad, cancelable y sin competir con la navegación.
- Medir antes de afirmar: `lib/perf.ts` (solo en desarrollo), `[nova]` en la terminal, `bench_dir`, `bench_thumb`, `scripts/index_bench.py`.

Riesgos y cifras reales: `PROJECT_STATUS.md` §5.

## 10. Limitaciones conocidas

PDF con páginas gigantes pueden tardar minutos y usar cientos de MB en la webview · HEIC, parte de RAW y otros dependen de proveedores de Windows · el visor no ejecuta PostScript, así que un EPS sin vista incluida no se muestra · `isolate` no contiene falta de memoria, desbordamiento de pila ni fallos de código nativo cargado en el proceso (manejadores de miniaturas de terceros): para eso haría falta un proceso aparte · los `ErrorBoundary` solo capturan errores de render, no de eventos ni promesas · Sin observador de cambios · sin caché de listados por pestaña · deshacer solo en memoria · búsqueda solo por nombre, no por contenido · resultados de búsqueda con acciones limitadas · comprimir y extraer sin progreso ni cancelación · WinRAR, Git y otras extensiones COM no aparecen en el menú (decisión 04.2) · carpetas con `desktop.ini` no muestran icono propio (decisión 04.3) · instalador sin firma Authenticode: SmartScreen avisa en la primera instalación (las actualizaciones sí van firmadas con la clave del actualizador) · solo Windows x64.
