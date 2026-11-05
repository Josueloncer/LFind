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
