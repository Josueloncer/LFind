# LFind · PROJECT_CONTEXT

Registro vivo del desarrollo (se actualiza al terminar **cada fase**). `PROJECT_STATUS.md` es el estado histórico de las fases 1–2;
este archivo recoge desde la actualización 04.2 (menú propio) y la 04.3 (eliminación rápida, iconos de carpeta propios, barra de dirección) y manda cuando haya diferencias. Debe bastar para mover el proyecto a otra carpeta/equipo y seguir sin conversaciones anteriores.

## Cómo retomar
1. Leer este archivo: secciones «04.3-*», «Comandos exactos», «Estado real de compilación en Windows», «Errores conocidos» y «Pendiente».
2. Entorno: contenedor Linux, **sin Windows, sin Tauri compilable, sin crates.io/npm**. Se verifica con `rustc --test`, `rustc --cfg nova_winck`
   (tipos del FFI de Windows), `tsc` con ruido de dependencias ausentes, arnés propio de pruebas TS y Chromium/Playwright para CSS.
3. Nunca declarar terminado lo que no se haya podido ejecutar; separar «verificado» de «escrito».
4. Nada de commits ni pushes.

## Decisión de producto (04.2) — DEFINITIVA
LFind usa **un menú contextual propio** (React/TypeScript + Rust). **No** usa `IContextMenu`, `QueryContextMenu` ni extensiones COM del Shell
para mostrarlo. Los menús nativo y propio ya no conviven: hay **un único camino**. Consecuencia aceptada por el usuario: las entradas que
terceros registran como extensiones COM (WinRAR, Git, Defender, Terminal…) ya no aparecen; las funciones equivalentes se ofrecen con
operaciones propias de Rust (comprimir/extraer, abrir terminal, abrir con, enviar a, nuevo…).

## Fase 0–1 · Auditoría de 04.1: qué se conserva y qué queda obsoleto

| Pieza (04 / 04.1) | Decisión | Motivo |
|---|---|---|
| Arreglo de miniaturas verticales (`app.css`, `thumbLayout.test.ts`) | **Conservar** | No tiene relación con el menú |
| Portapapeles del sistema: `clipboard.rs` (formato `DROPFILES`, probado), `clipboard_win.rs`, `systemClipboard.ts`, `seq` en el almacén, comandos `clipboard_*` | **Conservar** (adaptar) | Es una función real (copiar en LFind / pegar en el Explorador y al revés). El propietario del portapapeles pasa a ser una ventana temporal de solo mensajes creada por el propio módulo |
| `main_hwnd` en `AppState` y su carga en `setup` | **Eliminar** (decisión posterior) | `clipboard_win.rs` ya crea su propia ventana temporal de solo mensajes: no hace falta |
| Mayús+F10 / tecla Menú en `FileView.openKeyboardMenu` + antirrebote de 500 ms del evento `contextmenu` | **Conservar** | Es la entrada de teclado del menú propio |
| `scheduleRefresh` (recarga a 0,4/1,5/4 s) | **Conservar** | Sigue haciendo falta tras acciones que terminan fuera de LFind (comprimir, enviar a, abrir con) |
| Entradas propias que ya añadía LFind (Actualizar, Ordenar por, Deshacer, Abrir en pestaña nueva, Anclar, Copiar ruta) y el menú propio de `Pane.legacyContext` | **Conservar** y pasar a ser el menú | Ya eran el «plan B»; se reconstruyen como modelo puro y testeable |
| `ContextMenu.tsx`, `stores/ui.ts` (`openMenu`) | **Conservar y ampliar** | Faltan submenús (Nuevo, Ordenar, Enviar a), navegación por teclado completa |
| Pruebas de verbos/entradas de `shellMenu.test.ts` | **Reutilizar la idea** | Se reescriben sobre el nuevo modelo |
| `shell_menu.rs`, `shell_menu_win.rs`, `shell_menu_cmd.rs` (show/bench/prewarm), registro en `lib.rs`, `PrewarmItem`… | **Eliminar** | Todo gira alrededor de `IContextMenu` |
| `nativeMenu.ts`, `menuPerf.ts` (medición, banco, `planWarmup`), `shellMenu.ts` nativo (`planNativeMenu`, `applyOutcome`, `nativeDark`) | **Eliminar** | Obsoletos: no hay Shell que medir ni precalentar |
| PIDL rápido, cursor/ppp por Win32, `names`+`dirs` en el IPC, `join_names`, tabla de etapas | **Obsoleto** | Abrir el menú ya no cruza IPC ni toca el Shell. La lección útil —*no consultar nada al abrir*— se aplica de otra forma (ver Fase 3) |
| Preferencia `nativeContextMenu` + conmutador «Usar el menú contextual de Windows» en Ver | **Eliminar** | Es lo que hacía que a veces saliera uno y a veces otro. Los `prefs.json` antiguos que la contengan se ignoran sin error |
| `docs/NATIVE_CONTEXT_MENU.md`, `docs/MENU_PERFORMANCE.md`, `UPDATE04_PROGRESS.md`, `UPDATE041_PROGRESS.md` | **Retirar** (sustituidos por este archivo y `docs/CONTEXT_MENU.md`) | Describen una arquitectura que ya no existe |

Estado: ✅ Fase 0–1 completa (solo análisis; sin cambios de código).

## Fase 2 · Retirar el menú nativo — ✅ (escrita y comprobada por tipos/pruebas; no compilada en Windows)
- Rust: eliminados `shell_menu*.rs` (nova-core y src-tauri), sus comandos/registro/precalentamiento y `main_hwnd`; `prefs.rs` sin `native_context_menu`.
- Nuevo `src-tauri/src/clipboard_cmd.rs` (`clipboard_set_files`, `clipboard_read_files`, `clipboard_sequence`, `clipboard_has_files`).
- TS: eliminados `nativeMenu.ts`, `menuPerf.ts(+test)`, `shellMenu.ts(+test)`; `Pane.tsx` sin ningún rastro del camino nativo (un único `onContext`).
- Decisión: la antigua clave `nativeContextMenu` de `prefs.json` se ignora (serde descarta campos desconocidos).

## Fase 3 · Menú propio (interfaz) — ✅ (probada con el arnés TS; el aspecto visual falta de revisar en Windows)
- `src/lib/contextMenu.ts`: constructor PURO y síncrono (`buildContextMenu`) para archivo, carpeta, selección múltiple y espacio vacío; `isArchive`, `tidy`
  (sin separadores sobrantes), `scheduleRefresh`/`REFRESH_DELAYS_MS`, `ContextPoint`.
- `src/stores/shellData.ts`: «Enviar a» y plantillas de «Nuevo» se cargan UNA vez en reposo tras el arranque (`requestIdleCallback`).
- `src/lib/systemClipboard.ts`: `systemHasFiles()` en memoria (arranque, foco de ventana, tras copiar) → «Pegar» se activa sin consultar al abrir.
- `src/components/ContextMenu.tsx`: submenús (Nuevo, Ordenar y agrupar, Ver, Enviar a), teclado completo (↑↓ ←→ Intro Esc Inicio Fin), colocación medida y recortada.
- `src/lib/menus.ts`: `viewModeItems` extraído (compartido por «Vista» y el menú del fondo). `useFileActions`: `openWith, openTerminal, compress, extract, sendTo, newFromTemplate`.
- **Apertura instantánea**: abrir el menú = construir un array en memoria. Medido en Node (no en Windows): `buildContextMenu` con 5000 elementos seleccionados ≈ 0,009 ms de media (200 repeticiones). Sin IPC, sin Rust, sin Shell al abrir.
- Pruebas: `contextMenu.test.ts` (10) + `menus.test.ts`. Resultado con el arnés: 13/14; el fallo es una carencia del arnés (`toBeUndefined`), no del código.

## Fase 4 · Operaciones del sistema en Rust — ✅ (lógica pura probada; ramas de Windows y `src-tauri` SIN compilar)
- `nova-core::archive` (comprimir/extraer con `%SystemRoot%\System32\tar.exe` por ruta absoluta; `-a -cf x.zip`; nombres únicos; rechaza elementos de otra carpeta), `sendto` (lista `SendTo`, `.lnk` resueltos una vez con PowerShell, invocación por id, límite de línea de comandos), `newmenu` (`HKCR\.ext\ShellNew`: NullFile/FileName/Data; etiquetas con `SHLoadIndirectString`-equivalente por `reg.exe`), `shell::{open_with, open_terminal}`.
- `src-tauri/src/system_cmd.rs`: `open_with`, `open_terminal`, `compress_paths`, `extract_archive`, `list_send_to`, `send_to` (devuelve `launched | copyTo{dest} | zip`; la interfaz copia/comprime con SUS colas), `list_new_templates`, `create_from_template`. Registrados en `lib.rs`. Todo en `spawn_blocking`; rutas validadas con `safety::parse_path`.
- Decisión de seguridad: «Enviar a» y «Nuevo» se invocan por IDENTIFICADOR; nunca se ejecuta una ruta que llegue de la interfaz.
- Pruebas: 21/21 `rustc --test` (archive 4, newmenu 3, sendto 4, clipboard 7, names 3); `clipboard_win.rs` pasa `nova_winck`. Búsqueda de restos (`shell_menu`, `nativeContextMenu`, `native_context`) → 0.
- Pendiente de verificar en Windows: compilación de `src-tauri`, `creation_flags/raw_arg`, `reg.exe`, PowerShell, `tar.exe`, `wt.exe`.

## Fase 5–6 · Documentación y entrega — ✅
- `CHANGELOG.md` (entrada 04.2), `WINDOWS_VERIFICATION.md` (lista de comprobación del menú propio), nuevo `docs/CONTEXT_MENU.md`, este archivo.
- Retirados `docs/NATIVE_CONTEXT_MENU.md`, `docs/MENU_PERFORMANCE.md`, `UPDATE04_PROGRESS.md`, `UPDATE041_PROGRESS.md`; la lista completa de obsoletos está en `scripts/obsolete-04.2.txt` (el usuario debe borrarlos de su árbol, el ZIP solo trae archivos nuevos/modificados).
- Entrega: `LFind-update-04.2.zip` (solo nuevos/modificados; sin node_modules/target/dist). Sin commits ni pushes.

---
# Update 04.3 — Eliminación rápida · iconos propios de carpeta · barra de ruta editable
(El menú contextual propio de 04.2 NO se toca.)

## 04.3-A · Eliminación — ✅ (Rust y TS probados; compilación de `src-tauri` y papelera real SIN probar en Windows)
**Auditoría (por lectura de código; no hay Windows para cronometrar):** Eliminar → `DeleteDialog.confirm` (cierra el diálogo y llama a `ops.start`) → cola (`MAX_PARALLEL`=2) → `fs.startOp` (Tauri `Channel`) → `start_op` (`spawn_blocking`) → `run_op`.
1. **Papelera (por defecto):** un `trash::delete` POR elemento, en serie. Cada llamada es una operación del sistema completa. Con una selección grande es lo que más tarda.
2. **Eliminar permanente:** `delete_items` recorría TODO el árbol antes de borrar (`scan`) para calcular totales y luego lo recorría otra vez para borrar. Medido en Linux con un programa que replica ambos recorridos (ext4, caché caliente, mediana de 3): `scan` = 24 % (1 000 archivos) y 21 % (30 000) del tiempo de borrado, ANTES de borrar el primero. En NTFS sin medir.
3. **Interfaz:** el elemento seguía visible hasta que llegaba el evento `Done`, y solo entonces se recargaba la carpeta (lectura + orden): aviso y desaparición ocurrían tras el trabajo completo más una recarga. No había demora apreciable en React/IPC propiamente dicha.
**Cambios:**
- `nova-core::ops::trash_items` (nuevo, puro): lotes de `TRASH_BATCH`=64 con una sola llamada del sistema por lote; si un lote falla, se reintenta elemento a elemento para atribuir el error al correcto; comprueba `check_destructive`/existencia antes; cancelación y avance entre lotes. `commands.rs` lo conecta con `trash::delete_all` (batch) y `send_to_trash` (individual). Sigue yendo a la Papelera (no se elimina permanentemente nada que antes fuera a la Papelera).
- `delete_items`: sin `scan` previo (total desconocido → barra indeterminada; `Transfers.tsx` muestra «N eliminados»).
- Optimista: `stores/ops.ts` (`hidden`, `settle`, `pathKey`, `failedSources`). Al confirmar (`start`) los elementos se ocultan de inmediato en `Pane` (y el contador de la barra de estado); si falla un elemento (o algo dentro de la carpeta), si se cancela o se deshace, vuelven a verse al instante; si sale bien siguen ocultos hasta que la recarga confirma que ya no están (`settle`), con un cinturón de 8 s. El aviso de éxito solo sale con `Done` real (nunca antes); los errores siguen mostrando el mensaje de Rust.
- Pruebas: Rust 29/29 (`ops`+`names`+`safety`+`undo`, con serde desactivado por el arnés; incluye 2 nuevas de `trash_items`); TS 28/28 en el arnés (7 nuevas de eliminación optimista + las 10 de `contextMenu` + las 11 previas de `ops`). Los fallos antiguos del arnés (`toMatch`, `mockResolvedValueOnce`…) eran carencias del arnés y se corrigieron en él.
- NO probado: `trash::delete_all` real en Windows (suposición: una sola `IFileOperation` por lote; verificar tiempo con 1 000 archivos), error de permisos real, carpeta con 50 000 archivos real.

## 04.3-B · Iconos propios de carpeta — ✅ (verificado en Chromium con los tokens reales; falta mirarlo en la app en Windows)
**Auditoría:** el recurso propio ya existía: `FolderGlyph` (SVG vectorial en `Glyphs.tsx`, colores por tokens `--fold-back-a/b`, `--fold-front-a/b`, `--glyph-shine`, `--shadow-glyph` definidos en los cinco temas de `themes.css`; «System» resuelve a dark/light). El icono de carpeta de Windows aparecía porque `EntryIcon` (usado por `FileView` en lista/detalles/mosaicos/iconos, y por `Thumb` como respaldo) pedía a `iconService` el icono del sistema también para carpetas y lo pintaba encima del glifo en cuanto llegaba. `PreviewPane`, `EmptyState` y `TitleBar` ya usaban `FolderGlyph`/`EntryGlyph`; la barra lateral usa iconos lucide.
**Cambio:** `EntryIcon` no solicita ni pinta nunca icono del sistema para `kind === "dir"` (`usesSystemIcon`, `iconKinds.ts`): se ahorra además la consulta por carpeta. Archivos (EXE, PDF, PSD, `.lnk`…) y miniaturas no se tocan.
**Verificación (Chromium/Playwright, scripts en el entorno de desarrollo, no incluidos):** el SVG real de `FolderGlyph` con `tokens.css`+`themes.css` reales, 5 temas × tamaños 16/20/22/32/48/64/96/128/256 → todos los glifos son cuadrados y del tamaño pedido, los 4 tokens de degradado resuelven en cada tema y la inspección visual de las capturas es correcta y nítida (vectorial). Prueba TS: `folderIcons.test.ts`.
**Falta/documentado:** no existen variantes de carpetas especiales (Documentos, Descargas, Imágenes…), de carpeta vacía/llena ni insignia de enlace/unión: todas las carpetas usan el mismo glifo (las de `desktop.ini` con icono personalizado ya NO lo muestran, por decisión del usuario). El código Rust que extrae iconos de carpeta (`icons.rs`) queda sin uso desde la interfaz; se deja por ahora (limpieza opcional posterior).

## 04.3-C · Barra de dirección editable — ✅ lógica probada; interfaz React y `src-tauri` SIN ejecutar
**Auditoría:** ya existían migas con menú de subcarpetas, edición por clic en el espacio vacío y `Ctrl+L` (`App.tsx` → evento `nova:edit-path`), pero Enter llamaba a `navigate(texto)` SIN validar: una ruta inexistente cambiaba la ubicación y la vista mostraba el error (se perdía la carpeta anterior); no había normalización (comillas, `%VARIABLES%`, `/`, `C:`), ni migas «Este equipo › Disco local (C:)», ni manejo de rutas UNC como `\\srv\share`, ni tiempo límite para rutas de red.
**Diseño (sin segunda implementación de navegación):** la navegación sigue siendo `useNav.navigate` + `open_dir`; solo se añade VALIDACIÓN previa.
- Rust `nova-core::pathbar` (nuevo): `normalize` (pura: comillas, espacios, `%VAR%`, `/`→`\`, `C:`→`C:\`, barra final, `\\?\`, `\\?\UNC\`, UNC con servidor+recurso, solo absolutas) y `resolve` (normaliza + `safety::parse_path` + `metadata` + `read_dir` para comprobar permiso; en un hilo con `CHECK_TIMEOUT`=6 s, de modo que una unidad de red caída no bloquea). Devuelve `Dir`, `File{dir,file}` o un problema con mensaje en español (`notFound`, `denied`, `unreachable`, `timeout`, `invalid`, `empty`). Comando `resolve_path` (async + `spawn_blocking`), registrado en `lib.rs`; `fs.resolvePath` en `types.ts`/`tauri.ts` (en `browser.ts`, `NA`).
- TS `lib/addressBar.ts` (puro): `crumbsFor` (`Este equipo › Disco local (C:) › Users › …`; etiqueta del volumen desde `useApp.usage`, con nombres por defecto por tipo; en red el primer segmento es `\\srv\share`), `driveLabel`, `outcomeFor` (misma carpeta → recargar sin ensuciar el historial; archivo → abre su carpeta y avisa).
- `Toolbar.tsx`: migas desde `crumbsFor`; «Este equipo» no navega: abre el menú propio con las unidades (datos en memoria). Enter valida con `fs.resolvePath` (asíncrono, con `busy`; campo en rojo y aviso si falla, se conserva la ubicación anterior y la barra sigue abierta para corregir); Escape cancela y descarta validaciones pendientes (`check` token); blur cancela salvo mientras valida; el texto se selecciona al entrar; Ctrl+L sin cambios.
- Historial: `navigate` ya ignora la misma ruta y limpia «adelante»; Atrás/Adelante/`jump` sin cambios.
- Pruebas: Rust `pathbar` 8/8 (normalización Windows/UNC/variables/POSIX, carpeta anidada, archivo, inexistente, permiso —se omite si se ejecuta como root—, tiempo límite); TS `addressBar.test.ts` 8/8.
- NO probado: la interfaz en ejecución (foco, selección de texto, Enter/Escape/Ctrl+L, aspecto con los 6 temas), ni rutas de red/unidades reales en Windows.

## 04.3.1 · Corrección de compilación TS — ✅ configuración; sin ejecutar con dependencias reales
Ver `UPDATE04_PROGRESS.md`: `tsconfig` dividido (producción / pruebas), `@types/node`, `vitest/config`, `scripts/clean-obsolete.ps1` para los archivos obsoletos del menú nativo, 39/39 `invoke` ↔ comandos Rust coherentes. Pendiente: `npm install` (regenera el lock) y `npm run typecheck && npm test && npm run build`.

# Update 05.1 — Acceso directo visible a la búsqueda global — ✅ lógica y maquetación probadas; app React/Tauri en ejecución SIN verificar
- **Una sola vía de apertura:** `useSearch.openGlobal()` (`src/stores/search.ts`) cambia a ámbito global, conserva texto/filtros e incrementa `focusTick`; `Toolbar.tsx` atiende `focusTick` con un `useEffect` que enfoca y selecciona `#nova-filter`. La llaman el botón `.tb-global`, `Ctrl+Mayús+F` (`App.tsx`) y el globo del campo. Salir sigue siendo `leave()`/Esc/globo. Regla: no crear otra ruta para abrir la búsqueda global.
- Botón: `tbtn tb-global` antes del campo de filtro; solo icono ≤1240 px de contenedor (`@container main`); campo global más estrecho ≤900 px y ≤640 px. Solo usa tokens de tema.
- Detalle: `UPDATE05_PROGRESS.md` (sección 05.1).

# Update 05 — Motor de búsqueda global (Fase 1) — ✅ lógica y SQL probados; `rusqlite`, `src-tauri` e interfaz SIN compilar/ejecutar
- Código en `crates/nova-core/src/index/` (modelo, clasificación, exclusiones, `IndexStore`/`MemStore`, `sqlgen`, `SqliteStore`, escáner, `Indexer`); comandos Tauri en `src-tauri/src/index_cmd.rs`; interfaz en `SearchView.tsx`/`IndexDialog.tsx`/`stores/{search,indexStatus}.ts`/`lib/search{Controller,Query}.ts`.
- Reglas: el almacén siempre tras el trait `IndexStore`; el SQL solo se genera en `sqlgen` (se valida contra SQLite real con `scripts/index_xcheck.py`); `rusqlite` es opcional (`--features sqlite`). Los ajustes del índice viven en la tabla `meta` (clave `settings`), no en las preferencias existentes. Base de datos: `app_local_data_dir()/index/lfind-index.db`.
- Detalle (esquema, flujo, medidas, comandos, limitaciones): `UPDATE05_PROGRESS.md`. Siguiente paso: Fase 2 (observador `ReadDirectoryChangesW` + reconciliación); antes, verificar la Fase 1 en Windows.

## 04.3.3 · Pruebas de temas — causa raíz (sin Vitest disponible al corregir)
- `import x from "….css?raw"` en Vitest devuelve "" (plugin CSS de Vitest con `test.css` por defecto), no el CSS. `themes.test.ts` ahora usa `readFileSync(new URL("../styles/…", import.meta.url))`. **Regla del proyecto:** en pruebas, leer CSS del disco; no usar `?raw` con `.css`.
- Las pruebas de `app.css` (variables definidas, sin colores a mano) antes pasaban en vacío; ahora comprueban de verdad.
- Detalle y comandos: `UPDATE04_PROGRESS.md`. Discrepancia doc/código: ninguna encontrada; el ZIP trae `package.json` con nombre interno `nova-explorer` (sin cambios).

## Fase en curso
Ninguna. Queda la verificación en Windows (lista en `WINDOWS_VERIFICATION.md`).

## Dependencias y versiones (sin cambios en 04.3; no se añadió ninguna)
Front: react ^18.3.1, zustand ^5.0.1, lucide-react ^0.460.0, vite ^5.4.11, typescript ~5.6.3, vitest ^2.1.5. Rust: tauri 2, trash 5 (`delete_all`), serde 1, image =0.25.6; `rust-version` 1.77.2 (src-tauri) / 1.75 (nova-core).

## Comandos exactos
```
npm install
npm test                         # vitest
npx tsc --noEmit                 # tipos
cargo test -p nova-core          # lógica Rust (requiere crates.io)
npm run tauri dev                # desarrollo
npm run tauri build              # instalador de Windows
```
En este entorno de desarrollo (Linux, sin crates.io ni npm) NO se pueden ejecutar; las pruebas se corrieron con `rustc --test` sobre los módulos y un arnés propio de TypeScript.

## Estado real de compilación en Windows
**Nunca compilado ni ejecutado en Windows** desde la 04.2 (ni `src-tauri`, ni la interfaz con `npm run tauri dev`). Comprobado solo: lógica Rust pura con `rustc --test`, FFI del portapapeles con `nova_winck`, TypeScript con `tsc` (solo ruido por dependencias ausentes), pruebas TS con arnés, CSS/SVG en Chromium.

## Errores conocidos / limitaciones
- `menus.test.ts` falla 1 de 3 en el arnés propio (`toBeUndefined` no estaba implementado; ya añadido al arnés en 04.3). Con vitest real debe pasar.
- WinRAR/Git/Defender/«Compartir» no aparecen en el menú (decisión 04.2).
- Carpetas con icono personalizado (`desktop.ini`) ya no lo muestran (decisión 04.3); no hay variantes de carpetas especiales.
- «Eliminar» envía a la Papelera; en unidades sin Papelera Windows puede eliminar de forma permanente: es el comportamiento del crate `trash`, no se cambió ni se verificó.
- Eliminar permanentemente ya no muestra «N de M» sino «N eliminados» (total desconocido al no hacer el recorrido previo).
- `icons.rs` conserva la extracción de iconos de carpeta que la interfaz ya no pide (limpieza opcional).

## Archivos de 04.3
**Nuevos**: `crates/nova-core/src/pathbar.rs`, `src/lib/addressBar.ts(+test)`, `src/lib/folderIcons.test.ts`, `docs/UPDATE_04.3_REPORT.md`.
**Modificados**: `crates/nova-core/src/{lib,ops}.rs`, `src-tauri/src/{commands,lib}.rs`, `src/components/{EntryIcon,Pane,Toolbar,Transfers}.tsx`, `src/lib/iconKinds.ts`, `src/lib/fs/{browser,tauri,types}.ts` (incluye `SendToItem.kind` con `"folder"`, que 04.2 ya devolvía), `src/stores/ops.ts(+test)`, `src/styles/app.css`, `CHANGELOG.md`, `WINDOWS_VERIFICATION.md`, `PROJECT_CONTEXT.md`.
**Pruebas 04.3**: Rust `ops/names/safety/undo/pathbar` 37/37 (arnés con serde desactivado; `ops` +2, `pathbar` 8) y los 21 de 04.2 siguen; TS 38/38 en el arnés (`ops` 18 con 7 nuevas, `contextMenu` 10, `addressBar` 8, `folderIcons` 2).
**Pendiente**: todo lo de Windows; revisar el aspecto de la barra con los 6 temas; decidir si se quiere autocompletado de rutas.

---
# Historial 04.2 (referencia)



