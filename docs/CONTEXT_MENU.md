# Menú contextual propio de LFind (actualización 04.2)

LFind dibuja su **propio** menú contextual. No usa `IContextMenu`, `QueryContextMenu` ni extensiones COM del Shell, y ya no existe un menú nativo
alternativo: hay un único camino de código.

## Arquitectura

| Capa | Archivo | Qué hace |
|---|---|---|
| Modelo | `src/lib/contextMenu.ts` | `buildContextMenu(estado, acciones)`: función PURA y síncrona. Entrada: selección, ¿se puede pegar?, vista, destinos de «Enviar a», plantillas de «Nuevo». Salida: `MenuItem[]` (con submenús). Sin React, sin Tauri |
| Datos en memoria | `src/stores/shellData.ts`, `src/lib/systemClipboard.ts` | «Enviar a» y plantillas de «Nuevo» se cargan una vez, en reposo, tras el arranque. «¿Hay archivos en el portapapeles del sistema?» se mantiene al arrancar, al recuperar el foco y tras copiar |
| Interfaz | `src/components/ContextMenu.tsx` | Panel + submenús, teclado (↑ ↓ ← → Intro Esc Inicio Fin), colocación medida y recortada a la ventana |
| Acciones | `src/hooks/useFileActions.ts` | Cada entrada llama a una acción real (colas de operaciones, deshacer, renombrado en línea, diálogos de LFind) |
| Sistema (Rust) | `src-tauri/src/system_cmd.rs`, `clipboard_cmd.rs` y `nova-core::{shell, archive, sendto, newmenu, clipboard}` | Operaciones del sistema, siempre validando rutas en Rust |

## Por qué abre al instante
Abrir el menú es construir un array con datos que ya están en la memoria de la interfaz: **cero IPC, cero llamadas a Rust, cero consultas al
Shell, cero esperas**. Todo lo que cuesta tiempo (leer «Enviar a», leer plantillas, mirar el portapapeles) se hace antes, en segundo plano.
Medido en Node (no en Windows): construir el menú con 5000 elementos seleccionados ≈ 0,009 ms de media. El coste de pintarlo no se ha medido en Windows.

## Contenido
- **Archivo**: Abrir · Abrir con… · Mostrar en el Explorador · Cortar · Copiar · Copiar ruta · Crear acceso directo · Comprimir en .zip · Extraer aquí (si es un comprimido) · Enviar a ▸ · Cambiar nombre · Eliminar · Eliminar permanentemente · Propiedades.
- **Carpeta**: lo anterior sin «Abrir con…», más Abrir en pestaña nueva · Abrir terminal aquí · Pegar en la carpeta · Anclar/Quitar de favoritos.
- **Selección múltiple**: acciones sobre TODA la selección (copiar, cortar, comprimir, enviar a, eliminar, propiedades, copiar rutas). Sin Abrir ni Cambiar nombre.
- **Espacio vacío**: Nuevo ▸ (Carpeta, Documento de texto y las plantillas `ShellNew` del sistema) · Pegar · Deshacer · Ordenar y agrupar ▸ · Ver ▸ · Actualizar · Abrir terminal aquí · Abrir en el Explorador · Copiar ruta · Propiedades de la carpeta.
- Entrada por teclado: Mayús+F10 y tecla Menú (con antirrebote del evento `contextmenu` del navegador).

## Operaciones del sistema (Rust)
- **Abrir con…**: `ShellExecuteW` con el verbo `openas`. **Terminal**: `wt.exe -d <carpeta>` y, si no existe, PowerShell en consola nueva.
- **Comprimir / Extraer**: `%SystemRoot%\System32\tar.exe` (bsdtar) por ruta absoluta (el `tar` del PATH puede ser el de Git y no crea zip); `-a -cf nombre.zip`, sin ventana de consola. Nombres únicos (`names::unique_name`).
- **Enviar a**: lista `%APPDATA%\Microsoft\Windows\SendTo`; los `.lnk` se resuelven una vez, en segundo plano, con un PowerShell. Se invoca por **identificador**; jamás se ejecuta una ruta llegada de la interfaz. Carpetas → copia con la cola de LFind; «Carpeta comprimida» → compresión propia.
- **Nuevo**: plantillas de `HKCR\.ext\ShellNew` (`NullFile`, `FileName`, `Data`); la interfaz envía solo el identificador (la extensión).
- **Portapapeles de archivos**: `CF_HDROP` + *Preferred DropEffect*, compatible con el Explorador.
- Tras acciones que terminan fuera de LFind (enviar a, comprimir): recarga a 0,4 / 1,5 / 4 s.

## Limitaciones asumidas
1. Las entradas que terceros añaden como extensiones COM (WinRAR, Git, Defender, «Compartir», OneDrive…) **ya no aparecen**: es el precio de no depender del Shell. Se ofrecen equivalentes propios (comprimir/extraer, terminal, abrir con, enviar a).
2. «Compartir» de Windows 11 no es implementable sin `IContextMenu`/WinRT: no se ofrece.
3. Los archivos `.7z`/`.rar` se extraen con lo que `tar.exe` de la versión de Windows soporte (libarchive); comprimir solo crea `.zip`.
4. Los comprimidos con contraseña no se tratan.
