# LFind · actualización 04.3 · verificación

## Verificado aquí (Linux; sin Windows, Tauri ni crates.io)
| Qué | Cómo | Resultado |
|---|---|---|
| `trash_items` (lotes de 64, reintento individual, cancelación, inexistentes) | `rustc --test` | OK |
| `pathbar` (normalizar Windows/UNC/variables/POSIX; carpeta, archivo, inexistente, permiso, tiempo límite) | `rustc --test` | 8/8 OK (el de permiso se omite como root) |
| Resto de `nova-core` (`ops`, `names`, `safety`, `undo`) | `rustc --test` (serde desactivado) | 37/37 en total con `pathbar` |
| Eliminación optimista (ocultar, restaurar ante error/cancelación/deshacer, no ocultar duplicados, aviso solo al final) | arnés TS | 7 pruebas nuevas OK |
| Migas, etiquetas de unidad, resultado de Enter | arnés TS | 8/8 OK |
| Iconos de carpeta: glifo real de `FolderGlyph` con `tokens.css`+`themes.css` reales, 5 temas × 9 tamaños | Chromium | cuadrados, tokens resueltos, aspecto correcto |
| Coste de `scan` previo al borrado (programa equivalente) | Linux/ext4, caché caliente | 24 % (1 000 archivos) y 21 % (30 000) del tiempo de borrado |
| Tipos TS | `tsc` sin dependencias | sin errores propios (solo ruido por falta de `react`/`zustand`…) |

## NO verificado (hay que comprobarlo en Windows)
Compilación de `src-tauri` (`commands.rs`: `resolve_path`, papelera por lotes; `lib.rs`) · `trash::delete_all` real (supuesto: una operación por lote) · tiempos reales de eliminación (no hay cifras de Windows) ·
toda la interfaz en ejecución (Pane/Toolbar/EntryIcon) · aspecto de la barra y de los iconos con los seis temas dentro de la app · rutas de red y unidades reales · `%VARIABLES%` reales.

## Aplicar y compilar
0. (04.3.1) Si `tsc` se queja de `nativeMenu`/`shellMenu`/`ShellMenu*`: `powershell -ExecutionPolicy Bypass -File scripts\clean-obsolete.ps1` (borra archivos obsoletos). Después `npm install` (NO `npm ci`: el lock no incluye `@types/node`) y `npm run typecheck`.
1. Descomprime `LFind-update-04.3.zip` sobre la raíz del proyecto (sobrescribe). No hay archivos que borrar. Sin dependencias nuevas.
2. `npm install` · `npx tsc --noEmit` · `npm test` · `cargo test -p nova-core` · `npm run tauri dev`.
3. Si `src-tauri` no compila, pega el error exacto.

## Lista de comprobación en Windows
**Eliminación**
1. Archivo pequeño → Supr: desaparece al instante; el aviso aparece al terminar; está en la Papelera de Windows.
2. Archivo grande (≥ 1 GB), carpeta vacía, carpeta con ≥ 10 000 archivos: la interfaz no se congela; anota cuánto tarda hasta el aviso (compáralo con 04.2).
3. Selección múltiple (3 y 200 elementos): todos desaparecen a la vez; un solo aviso.
4. Error de permisos (archivo abierto/bloqueado o sin permiso): el elemento **reaparece** y sale el error; los demás se eliminan. «Deshacer» recupera de la Papelera.
5. Mayús+Supr (permanente): sin recorrido previo; el panel muestra «N eliminados».
6. Unidad sin Papelera (USB/red): confirma qué ocurre y anótalo.
**Iconos**
7. Carpetas con el icono de LFind en detalles, lista, mosaicos, contenido e iconos (16–256 px) y en los seis temas (Dark, Light, System, Cyberpunk, Hacker, Minimal). Miniaturas de imágenes y iconos de EXE/PDF/PSD sin cambios.
**Barra de dirección**
8. Migas «Este equipo › Disco local (C:) › …»; clic en cada una navega; el chevrón de «Este equipo» lista las unidades.
9. `Ctrl+L` desde cualquier vista (también con el foco en el filtro): campo editable con todo el texto seleccionado. Pegar `C:\Users\Usuario\Downloads` + Enter navega; Escape cancela.
10. Rutas con comillas, con `/`, `C:`, `%USERPROFILE%\Downloads`, otra unidad, carpeta anidada profunda.
11. Ruta inexistente: mensaje claro, campo en rojo, ubicación anterior intacta; corregir y Enter funciona.
12. Ruta de red `\\servidor\recurso\carpeta`; y una inaccesible/caída: la interfaz sigue respondiendo y a los ~6 s da el aviso de tiempo agotado.
13. Escribir la ruta de un archivo: abre su carpeta con aviso. Escribir la carpeta actual: solo recarga.
14. Atrás/Adelante tras varias navegaciones por la barra: el historial es coherente.
**No regresiones**: menú contextual, portapapeles, selección múltiple, búsqueda/filtro, miniaturas, persistencia de vista, actualización de carpetas.

---

# LFind · actualización 04.2 · menú contextual propio: verificación

## Verificado aquí (Linux, sin Windows ni crates.io)
| Qué | Cómo | Resultado |
|---|---|---|
| Lógica Rust: `archive`, `newmenu`, `sendto`, `clipboard`, `names` | `rustc --test` | 21/21 OK |
| FFI del portapapeles (`clipboard_win.rs`) | `rustc --cfg nova_winck --emit metadata` | Compila sin errores. No ejecutado |
| Constructor del menú (archivo, carpeta, multi, vacío, «Enviar a», «Pegar», separadores) | arnés TS | `contextMenu.test.ts` 10/10 |
| Coste de construir el menú con 5000 elementos | Node | ≈ 0,009 ms de media (no es una medición en Windows) |
| Tipos TS | `tsc` (sin dependencias instaladas) | Sin errores propios; solo ruido por falta de `react`/`zustand`/`lucide-react` |
| Miniaturas verticales | Chromium (04) | 0/32 solapes; `thumbLayout.test.ts` |

## NO verificado (hay que comprobarlo en Windows)
`src-tauri` completo (`system_cmd.rs`, `clipboard_cmd.rs`, `lib.rs` no se compilaron) · ramas `cfg(windows)` de `shell.rs`, `sendto.rs` y `newmenu.rs` (`CommandExt::creation_flags/raw_arg`, `reg.exe`, PowerShell) ·
`tar.exe` real (zip y extracción) · «Enviar a» y plantillas `ShellNew` reales · `wt.exe` · portapapeles real · los iconos nuevos (`archive`, `package-open`, `square-terminal`, `send`, `app-window`) en `lucide-react` instalado · aspecto visual del menú y los submenús · toda cifra de latencia real.

## Aplicar y compilar
1. Descomprime `LFind-update-04.2.zip` sobre la raíz (sobrescribe) y **borra los archivos obsoletos** con `scripts/obsolete-04.2.txt` (Windows PowerShell: `Get-Content scripts\obsolete-04.2.txt | ForEach-Object { Remove-Item $_ -ErrorAction SilentlyContinue }`). Si no se borran, `src/lib/nativeMenu.ts`, `shellMenu.ts`, `menuPerf.ts` y `shell_menu*.rs` romperán la compilación.
2. Sin dependencias nuevas: `Cargo.toml` y `package.json` no cambian.
3. `cargo test -p nova-core` · `npm test` · `npm run tauri dev`. **Si `src-tauri` no compila, pega el error.**

## Lista de comprobación en Windows (en este orden)
1. Compila y arranca. Clic derecho en un archivo: el menú **de LFind** aparece sin retraso perceptible, siempre el mismo (nunca el de Windows).
2. **Archivo**: Abrir, Abrir con… (diálogo «¿Cómo quieres abrir…?»), Mostrar en el Explorador, Copiar ruta, Crear acceso directo, Propiedades.
3. **Carpeta**: Abrir, Abrir en pestaña nueva, **Abrir terminal aquí** (Windows Terminal; sin él, PowerShell), Pegar en la carpeta, Anclar.
4. **Comprimir**: archivo y carpeta → «Comprimir en .zip» crea `nombre.zip` y se selecciona; con nombre repetido crea `nombre (2).zip`; sin ventana de consola; nombres con tildes, espacios y `&`.
5. **Extraer**: un `.zip` (y un `.7z` si el `tar.exe` de tu Windows lo soporta) → «Extraer aquí» crea una carpeta nueva junto a él.
6. **Enviar a ▸**: aparecen los destinos de `shell:sendto`; Escritorio/aplicaciones se lanzan con el archivo; una carpeta copia con la barra de progreso de LFind; «Carpeta comprimida» comprime. Tras ~1,5 s la carpeta se actualiza.
7. **Espacio vacío → Nuevo ▸**: Carpeta, Documento de texto y las plantillas del sistema (Word, Excel, imagen… según lo instalado) crean el archivo con nombre editable; verifica que los `.docx`/`.xlsx` abren sin error.
8. **Selección múltiple** (3 y 500 elementos): copiar, cortar, comprimir, enviar a, eliminar y propiedades incluyen a todos.
9. **Portapapeles**: copiar en LFind → pegar en el Explorador, y al revés (también cortar). «Pegar» está activo tras copiar fuera de LFind y volver a la ventana.
10. **Teclado**: Mayús+F10 y tecla Menú abren el menú **una sola vez**; ↑ ↓ ← → Intro Esc Inicio Fin funcionan, incluidos los submenús.
11. **Posición**: cerca de los bordes y de la esquina inferior derecha, con 100/150/200 % y dos monitores: el menú y los submenús quedan dentro de la ventana.
12. **Temas**: los seis; legibilidad y contraste del menú y los submenús.
13. **Rendimiento**: carpeta de 50 000 archivos, selecciona todo (Ctrl+A) y haz clic derecho: el menú aparece al instante. Mide con DevTools → Performance si dudas (clic → primer pintado).
14. Aceptado: ya no aparecen WinRAR/Git/Defender/«Compartir»; confirma que no falta nada que uses a diario.

---

# LFind · actualización 03 · qué está verificado y qué falta por comprobar en Windows

El código se desarrolló en un entorno Linux sin acceso a npm ni a crates.io y sin Windows. Por eso **no se ha ejecutado** `npm test`, `tsc`, `vite build`, `cargo test`
del proyecto ni ninguna compilación de `src-tauri`. Esta es la lista exacta.

## Verificado aquí

| Qué | Cómo | Resultado |
|---|---|---|
| Lógica TypeScript (vistas globales/fijadas, menús, temas, iconos, rutas, orden, geometría) | Arnés propio que transpila los `*.test.ts` con TypeScript y los ejecuta en Node (sustituto de vitest) | 97 pruebas OK (68 del proyecto original + 29 nuevas). Quedan 4 pruebas que fallan y 2 archivos que no cargan (`nav.test.ts`, `ops.test.ts`) por limitaciones del arnés (temporizadores simulados, `vi.mock`, `zustand` no instalado): **son idénticas con el código original**. |
| `nova-core::icons` (clasificación, claves, caché, hilos, tiempo límite) | `rustc --test` con la codificación PNG sustituida por un stub (la crate `image` no se puede descargar) | 10/10 OK (la prueba del codificador PNG real queda sin ejecutar) |
| `icons_win.rs` (FFI de Windows) | `rustc --cfg nova_winck --emit metadata` en Linux: comprueba tipos y sintaxis | Compila. **No se ha ejecutado.** |
| Tipos TypeScript | `tsc` con stubs de las dependencias, comparando contra el código original | Sin errores nuevos distintos del ruido de los stubs |
| CSS de los 5 temas | Script (mismas 62 variables, ninguna sin definir, sin colores sueltos) + capturas con Chromium de una maqueta con las clases reales de `app.css` | OK; revisado a ojo |

## Falta comprobar en Windows (en este orden)

Desde la raíz del proyecto, en PowerShell:

```powershell
npm ci
npm run typecheck                                   # tsc --noEmit
npm test                                            # vitest: debe pasar todo, incluidos themes.test.ts e iconService.test.ts
cargo test --manifest-path crates/nova-core/Cargo.toml   # incluye icons.rs (encode_png real), prefs.rs (tema desconocido, `pinned`)
cargo check --manifest-path src-tauri/Cargo.toml    # compila icons_cmd.rs y el registro en lib.rs
npm run tauri build                                 # compilación completa para Windows
```

Si algo falla, lo más probable (en orden):
1. `icons_win.rs`: enlace con `comctl32` (`ImageList_GetIcon`). Si el enlazador no la encuentra, comprobar que el manifiesto de Tauri está embebido (activa comctl32 v6).
2. `icons_win.rs`: las aserciones de tamaño de estructuras (`const _: () = assert!(...)`) fallarían en compilación, no en ejecución.
3. `cargo test`: `png_encoder_downsizes_and_never_upsizes` usa la crate `image` real.
4. TypeScript: cualquier error de tipos real que los stubs ocultaran.

## Comprobación manual (`npm run tauri dev`)

**Iconos del sistema**
- [ ] Una carpeta con EXE de aplicaciones muestra el icono propio de cada uno (Chrome, Discord, etc.). Ninguno se ejecuta.
- [ ] Un `.lnk` con icono personalizado y un `.url` con `IconFile` muestran ese icono.
- [ ] Una carpeta con icono personalizado (p. ej. Documentos/Imágenes de tu perfil, o una con `desktop.ini` + `IconResource`) lo muestra; una carpeta normal muestra el genérico.
- [ ] `.zip`, `.docx`, `.mp4`, `.webm`, `.psd`, `.ai`, `.mp3`, `.json`, `.ts` muestran el icono de su aplicación asociada.
- [ ] En Vista de iconos grandes, vídeo/PSD/PDF muestran primero el icono y luego la miniatura (si Windows tiene proveedor).
- [ ] Abrir una carpeta de 5 000 archivos y desplazarse rápido: la interfaz no se bloquea; en DevTools → Red no hay una llamada por archivo (el comando `get_icons` sale por lotes).
- [ ] Menú Ver → «Usar iconos del sistema» desactivado → vuelven los glifos de LFind al instante.
- [ ] Cambiar el icono de un `.exe`/`.lnk` (o sustituir el archivo) → el icono se actualiza al volver a la carpeta.
- [ ] Unidad de red lenta o desconectada: la carpeta abre; los iconos que no llegan se quedan como glifo.

**Vistas**
- [ ] Cambiar a «Iconos grandes» en una carpeta → entrar en otra carpeta, otra unidad, atrás y adelante: se mantiene.
- [ ] Cerrar con el botón X y también con Alt+F4 justo después de cambiar → al abrir sigue igual.
- [ ] Menú Ver → «Recordar esta vista solo para esta carpeta»; cambiar la vista global en otra carpeta; volver: la fijada conserva la suya. «Dejar de recordar…» la libera.

**Temas**
- [ ] Sidebar → «Tema»: los seis; el cambio es inmediato; se conserva al reiniciar; «Sistema» sigue al de Windows al cambiarlo con la app abierta.
- [ ] En cada tema revisar: barra de título, pestañas, barra lateral, migas, selección y rectángulo de selección, menús contextuales, diálogos (eliminar, renombrar), notificaciones, panel de transferencias, vista previa.
- [ ] Hacker usa tipografía monoespaciada; Cyberpunk, cian/magenta; Minimal, sin color salvo el rojo de acciones destructivas.
- [ ] Un `prefs.json` con `"theme": "otro"` abre en «Sistema» sin perder el resto.

**Regresión de la actualización 02**
- [ ] Selección por rectángulo, miniaturas de imágenes, panel de vista previa (Alt+P), cola de operaciones, atajos Ctrl+L / Ctrl+F.
