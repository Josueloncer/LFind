# Informe de cambios · LFind 04.3

## 1. Eliminación
**Dónde estaba la demora (por lectura del código; sin Windows no hay cronometraje real):** confirmar → cola → `start_op` → `trash::delete` **uno por elemento** (una operación del sistema cada uno) → solo al terminar llegaba el evento final, y recién entonces se mostraba el aviso y se recargaba la carpeta; hasta ese momento el elemento seguía en pantalla. En la eliminación permanente, además, se recorría todo el árbol antes de borrar (en Linux, +21–24 %).
**Qué cambió:** ocultado inmediato en la vista (`stores/ops.ts` → `hidden`; `Pane.tsx` filtra); papelera por lotes de 64 con reintento individual (`ops::trash_items`); sin recorrido previo en el borrado permanente; restauración al instante ante error/cancelación/deshacer; aviso de éxito solo con la confirmación real de Rust. Sigue yendo a la Papelera.
**Probado:** Rust (lotes, atribución de errores, inexistentes, cancelación) y TS (7 pruebas). **No probado:** tiempos y papelera reales en Windows.

## 2. Iconos de carpeta
`EntryIcon` pedía el icono del sistema también para carpetas y lo pintaba sobre el glifo propio. Ahora las carpetas nunca lo piden: usan `FolderGlyph` (vectorial, tokens de los temas). Verificado en Chromium (5 temas × 9 tamaños). Falta: variantes de carpetas especiales/vacía/llena/enlace (documentado; no se inventaron).

## 3. Barra de dirección
Migas «Este equipo › Disco local (C:) › …», edición con clic/`Ctrl+L`, validación en Rust (`pathbar`, `resolve_path`) con tiempo límite, mensajes claros, conservación de la ubicación ante error, soporte de comillas/variables/UNC. Navegación reutilizada (`navigate` + `open_dir`).

## Archivos
Nuevos: `crates/nova-core/src/pathbar.rs`, `src/lib/addressBar.ts(+test)`, `src/lib/folderIcons.test.ts`, este informe.
Modificados: `nova-core/src/{lib,ops}.rs`, `src-tauri/src/{commands,lib}.rs`, `src/components/{EntryIcon,Pane,Toolbar,Transfers}.tsx`, `src/lib/iconKinds.ts`, `src/lib/fs/{browser,tauri,types}.ts`, `src/stores/ops.ts(+test)`, `src/styles/app.css`, `CHANGELOG.md`, `WINDOWS_VERIFICATION.md`, `PROJECT_CONTEXT.md`.

## Pruebas realizadas y pendientes
Realizadas: ver `WINDOWS_VERIFICATION.md` («Verificado aquí»). Pendientes: toda la lista de Windows (compilación, eliminación real, interfaz, rutas de red).

## Aplicar y compilar
Descomprimir sobre la raíz (sin borrados) → `npm install` → `npx tsc --noEmit` → `npm test` → `cargo test -p nova-core` → `npm run tauri dev`.
