# Update 13 · Herramientas profesionales e integración con Windows

Informe de la implementación. El estado vigente está en [PROJECT_STATUS.md](../../PROJECT_STATUS.md) y las decisiones técnicas en [PROJECT_CONTEXT.md](../../PROJECT_CONTEXT.md).

## Análisis previo

Lo que ya existía y se amplió, sin duplicarlo:

| Pieza | Uso |
|---|---|
| `stores/ops.ts`, `ops.rs` (cola, conflictos, deshacer, papelera) | gestor de operaciones y papelera del detector |
| `PreviewPane` | el inspector es una pestaña de ese panel |
| `viewer::Fact`, `RasterProvider`, `thumbs_win` (COM sin dependencias) | formato de los datos y patrón de FFI |
| `AppState.jobs`, `cancel_job` | hash cancelable |
| `sys::background_thread` | prioridad baja de CPU y disco |
| Rutas virtuales `lfind://` | vista del detector |
| `react-virtual`, `useUi` (avisos y diálogos), zustand | interfaz |

Ninguna herramienta nueva mantiene un proceso o hilo mientras está inactiva.

## Módulos

| Módulo | Archivos principales |
|---|---|
| Paleta de comandos (Ctrl+K) | `lib/commandSearch.ts`, `lib/builtinCommands.ts`, `stores/palette.ts`, `stores/paneActions.ts`, `components/CommandPalette.tsx` |
| Inspector (Ctrl+I) | `nova-core/inspector.rs`, `src-tauri/inspector_cmd.rs`, `lib/inspection.ts`, `components/Inspector.tsx` |
| Gestor de operaciones | `ops.rs` (pausa), `commands.rs` (`pause_job`), `stores/ops.ts`, `components/Transfers.tsx` |
| Detector de duplicados | `nova-core/duplicates.rs`, `src-tauri/duplicates_cmd.rs`, `stores/duplicates.ts`, `lib/duplicateSelection.ts`, `components/DuplicatesView.tsx` |
| Terminal (Ctrl+J) | `nova-core/terminal.rs`, `src-tauri/terminal_cmd.rs`, `lib/terminalCommands.ts`, `stores/terminal.ts`, `components/TerminalPanel.tsx` |
| Integración con Windows | `nova-core/winshell.rs`, `src-tauri/windows_cmd.rs`, `src-tauri/nsis-hooks.nsh`, `components/SettingsDialog.tsx` |

Otros archivos tocados: `App.tsx`, `Pane.tsx`, `PreviewPane.tsx`, `Sidebar.tsx`, `Dialogs.tsx`, `icons.ts`, `useFileActions.ts`, `lib/addressBar.ts`, `lib/menus.ts`, `lib/path.ts`, `lib/fs/*`, `stores/ui.ts`, `styles/app.css`, `nova-core/lib.rs`, `prefs.rs`, `src-tauri/lib.rs`, `state.rs`, `tauri.conf.json` (solo `installerHooks`).

## Dependencias añadidas

| Dependencia | Motivo |
|---|---|
| `sha2` (Rust) | SHA-256 del inspector y del detector. Ya estaba en el árbol de la aplicación. |
| `tauri-plugin-single-instance` | Abrir una carpeta desde Windows la lleva a la ventana existente. Además resuelve el riesgo R-04 (dos indexadores cruzados). |
| `@xterm/xterm`, `@xterm/addon-fit` | Emulador real de terminal. Se cargan solo al abrir la terminal (trozo aparte del paquete). |

La pseudoconsola, los Job Objects, el Registro, el sistema de propiedades y GDI se usan con FFI directa, como el resto del núcleo.

## Decisiones

- **Base propia del detector** (`duplicates/duplicates.db`, más un `scan.tmp` que se borra al acabar): el índice y los informes tienen un escritor propio y no deben esperar a este trabajo.
- **Caché de hashes:** se reutiliza solo si coinciden ruta, tamaño y fecha de escritura (100 ns), y no se guarda si el archivo se modificó hace menos de 2 s. Si cambia durante la lectura, el resultado se descarta.
- **Enlaces duros:** no cuentan como duplicados ni como espacio recuperable.
- **Seguimiento de carpeta en la terminal:** desactivado de serie. Activarlo es la intervención del usuario. Solo escribe un cambio de carpeta y solo si el shell está libre y sin una línea a medias. La ruta se escribe de forma literal, y en CMD se descartan las que tienen `%`, `"`, `^` o `!`.
- **Explorador preferido:** verbo predeterminado de `Directory` y `Drive` en `HKCU`, con el valor anterior guardado. Al desinstalar, un gancho de NSIS retira todo para que las carpetas sigan abriéndose. `Folder` no se toca porque incluye carpetas virtuales como la Papelera.
- **Terminal y salida masiva:** si la interfaz no ha pintado 2 MiB, se deja de leer.

## Comandos

```powershell
npm run typecheck
npm test
cargo test --manifest-path crates/nova-core/Cargo.toml --features sqlite --locked
cargo test --manifest-path crates/nova-core/Cargo.toml --locked
npm run build
npm run tauri build
```

## Resultados

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| `npm test` | 401/401 |
| Rust con SQLite, `--locked` | 291 + 3 correctas |
| Rust sin SQLite, `--locked` | 242 + 3 correctas |
| `npm run build` | correcto; la terminal queda en un trozo aparte |
| `tauri build` del producto, sin artefactos de actualización | instalador generado |

### En la app real (instalación aislada; preferencias e índice reales intactos)

| Módulo | Caso | Resultado |
|---|---|---|
| Paleta | Ctrl+K, buscar, Intro, ruta escrita, flechas, Escape | correcto |
| Inspector | WAV generado | duración 00:00:03, 705 kbps, 44,1 kHz, mono |
| Inspector | SHA-256 de un archivo real | igual al calculado aparte |
| Inspector | archivo real solo en OneDrive | descrito sin cambiar sus atributos |
| Operaciones | pausa de una copia de 3 GB | el archivo de destino no crece; al reanudar llega a 3 GB, con velocidad y tiempo restante |
| Operaciones | cancelar a mitad | no queda nada en el destino |
| Operaciones | archivo bloqueado por otro proceso | error claro, destino vacío |
| Duplicados | abrir la herramienta | no analiza; mismos hilos |
| Duplicados | árbol real con copias y un archivo de igual tamaño y otro contenido | 1 grupo con las 3 copias reales |
| Duplicados | marcar copias | la última queda bloqueada |
| Duplicados | enviar a la Papelera, con confirmación | solo la marcada; el grupo se actualiza |
| Duplicados | comparar byte a byte y abrir ubicación | correcto |
| Terminal | PowerShell, CMD, `Read-Host` interactivo, errores | correcto |
| Terminal | Retroceso dentro de la terminal | no navega en LFind |
| Terminal | seguir la carpeta | cambia al navegar |
| Terminal | cerrar el panel o matar LFind con `ping` en marcha | el proceso hijo desaparece en ambos casos |
| Windows | confirmación antes de cada cambio | sin confirmar no se escribe nada |
| Windows | explorador preferido y `start "" carpeta` | se abre en LFind, en una pestaña nueva de la misma ventana |
| Windows | restaurar | Registro idéntico al inicial |
| Windows | actualizar con `/UPDATE` | la integración se conserva |
| Windows | desinstalar con todo activo | Registro idéntico al inicial |

### Consumo medido

Instalación aislada en un equipo en uso, índice desactivado. Memoria privada del proceso nativo.

| Situación | Resultado |
|---|---|
| LFind en reposo, 20 s | 11 MB nativos, 236 MB de interfaz (WebView2), 0,1 % de un núcleo, 0 MB leídos |
| Enumerar C: completa con exclusiones | 964 308 archivos en 73 s, 29 MB al acabar; 229 444 en la nube sin tocar |
| Comparar `C:\Dev` (187 374 candidatos, sin terminar en ~350 s) | pico de 37 MB, ~25 % de un núcleo, 1 GB leído |
| Respuesta de la app durante ese análisis | IPC con mediana de 3 ms y peor caso de 5 ms |
| Cancelar | 74 ms, sin archivo temporal |
| Terminal abierta | +0 MB nativos, +20 MB de interfaz, 69 MB entre shell y consola |

## Limitaciones

- **Duplicados a gran escala:** la enumeración se midió con casi un millón de archivos, pero ningún análisis completo de esa escala llegó al final. Con muchos archivos pequeños candidatos es lento: `C:\Dev` no terminó en casi 6 minutos.
- **Reanudar tras cerrar LFind:** pausar y reanudar funciona en la misma sesión. Si se cierra, hay que empezar de nuevo; se enumera otra vez y se reutilizan los hashes ya guardados.
- **Archivos en la nube:** el detector los omite, y el inspector solo muestra lo que Windows sabe sin abrirlos.
- **Inspector:** los datos de contenido dependen de los manejadores de propiedades instalados en Windows. El número de páginas de un PDF no siempre está disponible.
- **Permisos:** se muestran propietario, atributos y solo lectura, no la lista de control de acceso completa.
- **Pausa:** actúa entre bloques o archivos. Un archivo de hasta 8 MB se termina antes de pausar, y en una Papelera por lotes, el lote en curso.
- **Deshacer:** solo lo que ya era reversible. No se recupera lo eliminado de forma permanente.
- **Terminal:** una sesión a la vez. El seguimiento va de LFind a la terminal; un `cd` en la terminal no mueve el explorador. Una ruta con `%`, `"`, `^` o `!` no se sigue en CMD.
- **Windows:** `Win+E`, el Explorador ya abierto, el escritorio y los diálogos de abrir y guardar siguen siendo de Windows. La prueba real usó `start`; el doble clic en el escritorio con ratón no se ha probado. Una reinstalación manual, a diferencia de una actualización, puede retirar la integración.
- **Sin probar con hardware:** unidades USB y rutas de red en estas herramientas, y ratón físico.
