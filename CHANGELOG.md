# LFind — Registro de cambios

## Actualización 03: iconos del sistema, vistas globales, seis temas y sistema de diseño

Actualización incremental sobre la 02 (no sustituye la arquitectura). No actualiza dependencias ni añade ninguna.
**Estado de verificación: ver `WINDOWS_VERIFICATION.md`** (el código nativo de Windows y `src-tauri` no se pudieron compilar ni ejecutar en el entorno donde se desarrolló).

### 1. Iconos nativos de Windows (distintos de las miniaturas)
- `nova-core::icons`: servicio de iconos del sistema por **lotes**. Un icono por tipo (`docx`, `mp4`, `zip`, `psd`, `dll`…) se extrae UNA vez por sesión
  y tamaño, aunque haya miles de archivos; los iconos propios (EXE, SCR, LNK, URL, ICO, CPL, appref-ms, raíces de unidad y carpetas cuyo `desktop.ini`
  declara icono) se leen del archivo y se cachean en disco con clave (ruta, tamaño, fecha, px, versión): si el archivo cambia, se vuelve a extraer.
- `nova-core::icons_win`: API nativas (`SHGetFileInfoW` + `SHGetImageList` + `ImageList_GetIcon`). **No ejecuta nada**: el shell solo lee el recurso de icono.
  Respeta los iconos personalizados de accesos directos, carpetas y aplicaciones. No usa `THUMBNAILONLY`: iconos del sistema y miniaturas son servicios y cachés separados.
- Extracción en hilos con tiempo límite (un manejador de iconos colgado no bloquea la interfaz) y concurrencia limitada.
- Interfaz: `iconService` junta las solicitudes visibles en un único lote (≈24 ms), no pide lo que ya tiene (`have`), retira lo que sale de pantalla, descarta la cola al
  cambiar de carpeta y acota la memoria (LRU). Carga progresiva: glifo propio al instante → icono del sistema → miniatura (si procede).
- Respaldo: sin icono del sistema (o fuera de Windows) se conserva el glifo de LFind. Opción «Usar iconos del sistema» en el menú Ver (activada por defecto).
  «Borrar la caché de miniaturas e iconos» vacía ambas.
- Los `.dll` comparten el icono del tipo (igual que el Explorador); no se extrae el primer icono incrustado de cada DLL.

### 2. Persistencia de vistas
- Modo de visualización y tamaño de iconos se guardan como vista **global** cada vez que se cambian: se conservan al navegar, cambiar de unidad, atrás/adelante y al reiniciar
  (guardado inmediato al cerrar la ventana).
- «Recordar esta vista solo para esta carpeta» (menú Ver) es una acción explícita; sin ella, toda carpeta hereda la vista global. «Dejar de recordar…» la libera.
- Orden y agrupación siguen siendo por carpeta, como antes. Las vistas por carpeta guardadas por versiones anteriores conservan su orden y heredan modo/tamaño globales.

### 3. Temas
- Seis temas: **Cyberpunk** (negro profundo, cian y magenta neón), **Hacker/Terminal** (negro, verde fósforo, tipografía monoespaciada), **Minimal** (monocromo, sin acentos),
  **Dark**, **Light** y **System**. Selector en la barra lateral; el cambio es inmediato y se guarda. «System» sigue al tema de Windows.
- Un tema desconocido en `prefs.json` (p. ej. de una versión futura) cae a «System» sin perder el resto de preferencias.
- Mica solo con Dark/Light; el resto conserva su fondo propio. La barra de título nativa sigue el esquema claro/oscuro del tema.

### 4. Sistema de diseño
- `tokens.css`: escalas independientes del tema (tipografía, espaciado, tamaños de componentes, movimiento, capas).
- `themes.css`: los 5 temas concretos con **la misma lista de 62 variables** (superficies, acento, texto, líneas, estados, sombras, radios, tipografías, iconos, categorías de archivo).
  Una prueba comprueba que ningún tema omita o añada variables y que `app.css` no contiene colores ni sombras escritos a mano. Un tema de usuario futuro será otro bloque con esas mismas claves.
- Se mantienen en CSS algunos valores de geometría (altos de fila, anchos de celda) porque la lista virtualizada los duplica en JavaScript.

### 5. Nombre
- «NOVA Explorer» pasa a «LFind» en la interfaz, título de ventana, `productName` y documentación. Se conservan el identificador `app.nova.explorer`, la carpeta de preferencias,
  las claves `nova.*` y los nombres de crate/paquete para no perder configuraciones.

### Archivos nuevos
`crates/nova-core/src/icons.rs`, `icons_win.rs`, `src-tauri/src/icons_cmd.rs`, `src/styles/themes.css`, `src/lib/themes.ts`, `themes.test.ts`, `iconKinds.ts`, `iconService.ts`, `iconService.test.ts`,
`src/hooks/useSystemIcon.ts`, `src/components/EntryIcon.tsx`, `WINDOWS_VERIFICATION.md`, `UPDATE03_PROGRESS.md`.

### Archivos modificados
Rust: `nova-core/{lib,prefs}.rs`, `src-tauri/src/lib.rs`, `src-tauri/{Cargo.toml,tauri.conf.json}`, `nova-core/Cargo.toml`. Interfaz: `App.tsx`, `main.tsx`, `index.html`, `public/boot.js`, `styles/{tokens,app}.css`,
`components/{FileView,Glyphs,Pane,Sidebar,Thumb,TitleBar,Toolbar,icons}`, `lib/{menus,viewState,layout.test,menus.test}`, `lib/fs/{types,tauri,browser}.ts`, `stores/{prefs,clipboard}.ts`, `README.md`, `PROJECT_STATUS.md`.

### Limitaciones conocidas
- Un icono propio cambia con el archivo, pero el icono de un acceso directo no se invalida si solo cambia el icono de su destino (sí si cambia el `.lnk`).
- Si el sistema no tiene icono para un tipo, no se reintenta durante la sesión.
- Las entradas de vista por carpeta creadas antes de esta versión dejan de fijar modo/tamaño (ahora heredan el global): es consecuencia directa de la regla pedida.

---

# LFind — Actualización 02: selección por rectángulo, miniaturas y cola de operaciones

Actualización incremental sobre el proyecto existente (no sustituye la arquitectura). No modifica
`package.json`, `tauri.conf.json` ni `src-tauri/Cargo.toml`.

## Qué se ha añadido

### 1. Selección por rectángulo
- Pulsar en una zona vacía de la lista y arrastrar dibuja un rectángulo (discreto, con el color de acento del tema) que selecciona
  archivos y carpetas. Funciona en las 8 vistas (detalles, lista, mosaicos, contenido y los iconos pequeños/medianos/grandes/extragrandes) y con grupos.
- Mayús añade a la selección actual; Ctrl la invierte; Esc cancela y restaura la selección anterior.
- Un clic en una zona vacía sin arrastrar limpia la selección (umbral de 4 px, así un temblor no cuenta). Pulsar sobre un elemento, el clic
  derecho y las barras de desplazamiento no inician el rectángulo; el arrastrar y soltar de archivos no cambia.
- Desplazamiento automático al arrastrar junto a un borde. El rectángulo queda recortado a la zona de archivos.
- Rendimiento: los impactos se calculan con la geometría de la lista (búsqueda binaria), sin leer el DOM, así que funciona con listas
  virtualizadas y con cientos de miles de elementos.

### 2. Miniaturas
- Servicio central en Rust (`nova-core::thumbs`) con proveedores intercambiables:
  - **Imágenes** (JPG, PNG, GIF —primer fotograma—, BMP, TIFF, WebP, ICO) con la crate `image` (Rust puro), con límites de tamaño y memoria y orientación EXIF.
  - **SVG**: se entrega tal cual y se pinta con `<img>` (sin scripts ni recursos externos).
  - **Miniaturas de Windows** (`IShellItemImageFactory`, la misma API del Explorador) para PDF, PSD/PSB, AI, EPS, vídeo, audio con carátula, Office,
    HEIC/AVIF y RAW de cámara. Solo funcionan si Windows tiene un proveedor instalado para ese formato (ver limitaciones).
- Caché persistente en disco (`%LOCALAPPDATA%\<app>\thumbs`), con clave (ruta, tamaño, fecha de modificación, px, versión): si el archivo cambia, no se sirve una miniatura vieja.
  Límite de 300 MB (se borra lo más antiguo). Los formatos sin proveedor se recuerdan 7 días y los fallos 5 minutos para no reintentar en cada visita.
- Carga diferida y progresiva: se muestra el icono del tipo de archivo al instante y se sustituye por la miniatura cuando llega. Solo se piden
  los elementos montados en pantalla (la lista está virtualizada); si salen de pantalla antes de ser atendidos, se retiran de la cola.
- Concurrencia limitada (4 solicitudes desde la interfaz, 3 generaciones simultáneas en Rust), deduplicación, cancelación al cambiar de carpeta
  («época»), tiempo límite de seguridad y caché LRU en memoria (400). No se piden miniaturas de carpetas, archivos ocultos, archivos vacíos ni en las vistas con icono pequeño.
- Tamaños de miniatura por escalones (64/128/256/512) según el tamaño del icono y la densidad de pantalla.
- **Panel de vista previa** (botón de la barra o Alt+P): miniatura grande del archivo seleccionado, vista previa de texto/código (primeros 8 KB, mostrados como texto plano) y datos básicos.
- Menú **Ver**: «Mostrar miniaturas» (se guarda) y «Borrar la caché de miniaturas».

### 3. Operaciones y transferencias (ampliación del sistema existente)
- Cola con máximo 2 operaciones simultáneas; el resto queda «En cola» y avanza en orden. Se puede quitar de la cola sin que llegue al backend.
- No se lanza dos veces la misma operación mientras está en curso o en cola (misma acción, mismo destino y mismos elementos).
- Estados claros: En cola · En curso · Completado · Completado con errores · Error · Cancelado.
- La vista se actualiza al terminar cada operación (ya existía) y se muestran los errores por elemento.

## Archivos nuevos
- `crates/nova-core/examples/bench_thumb.rs`
- `crates/nova-core/src/b64.rs`
- `crates/nova-core/src/preview.rs`
- `crates/nova-core/src/thumbs_win.rs`
- `src-tauri/.cargo/config.toml`
- `src-tauri/src/thumbs_cmd.rs`
- `src/components/PreviewPane.tsx`
- `src/components/Thumb.tsx`
- `src/hooks/useThumb.ts`
- `src/lib/marquee.test.ts`
- `src/lib/marquee.ts`
- `src/lib/menus.test.ts`
- `src/lib/thumbKinds.ts`
- `src/lib/thumbService.test.ts`
- `src/lib/thumbService.ts`
- `src/lib/viewGeometry.ts`
- `src/stores/ops.test.ts`

## Archivos modificados
- `crates/nova-core/Cargo.toml`
- `crates/nova-core/src/lib.rs`
- `crates/nova-core/src/prefs.rs`
- `crates/nova-core/src/thumbs.rs`
- `src-tauri/src/lib.rs`
- `src/App.tsx`
- `src/components/FileView.tsx`
- `src/components/Glyphs.tsx`
- `src/components/Pane.tsx`
- `src/components/Toolbar.tsx`
- `src/components/Transfers.tsx`
- `src/components/icons.ts`
- `src/lib/fileKinds.ts`
- `src/lib/fs/browser.ts`
- `src/lib/fs/tauri.ts`
- `src/lib/fs/types.ts`
- `src/lib/menus.ts`
- `src/stores/ops.ts`
- `src/styles/app.css`

## Formatos
| Formato | Cómo | Requisito |
|---|---|---|
| JPG, PNG, GIF, BMP, TIFF, WebP, ICO | LFind (Rust) | Ninguno |
| SVG | LFind (se muestra con `<img>`) | Ninguno |
| PDF | Windows | Un proveedor instalado (Edge/Acrobat, etc.) |
| PSD, PSB, AI, EPS | Windows | Un proveedor instalado (Photoshop/Illustrator, Affinity, códec…) |
| Vídeo, audio con carátula, Office, HEIC/AVIF, RAW | Windows | Códecs/proveedores del sistema |
| Texto/código | Vista previa de texto (panel) | Ninguno |
| Cualquier otro | Icono del tipo de archivo | — |

## Limitaciones conocidas
- **PDF, PSD y AI dependen de Windows.** Sin un proveedor instalado se muestra el icono del tipo (con la extensión) y, en el panel de vista previa, una explicación. No se ha incorporado
  un renderizador propio de PDF porque exige una dependencia pesada (p. ej. pdfium, varios MB de DLL) o una API de Windows aún sin verificar; queda como ampliación.
- Los archivos AI con compatibilidad PDF no se interpretan por su cuenta.
- Las miniaturas de GIF/WebP animados son estáticas (primer fotograma). Los TIFF/imágenes de más de 100 MP o 200 MB no generan miniatura.
- El código nativo de Windows (miniaturas del sistema, Papelera, abrir archivos, capacidad de unidades) está comprobado a nivel de tipos, pero no se ha ejecutado en Windows.
- La cancelación de operaciones detiene el trabajo **entre elementos**; un archivo de más de 8 MB se interrumpe por bloques y se elimina el temporal, pero uno más pequeño termina antes de parar.
  Lo ya procesado no se revierte (copiar y mover se pueden deshacer con Ctrl+Z; enviar a la Papelera se recupera desde la Papelera; eliminar permanentemente no es reversible).
- Con la lista agrupada no hay selección por rectángulo sobre las cabeceras de grupo (se ignoran), y en el modo Lista no se agrupa.
- Las pruebas de componentes (jsdom) se ejecutaron solo durante el desarrollo; no se incluyen para no añadir dependencias de desarrollo.
