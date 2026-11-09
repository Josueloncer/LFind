# LFind — Registro de cambios

## Update 13: herramientas profesionales
- Nueva paleta de comandos (Ctrl+K): busca y ejecuta cualquier acción, carpeta o ajuste con el teclado.
- Nuevo inspector de archivos (Ctrl+I): detalles, propiedades multimedia, EXIF, permisos y hash a petición.
- Las operaciones de archivo se pueden pausar y muestran velocidad y tiempo restante.
- Nuevo detector de duplicados por contenido real, que solo trabaja cuando se lo pides, nunca elimina la última copia y se puede pausar y continuar, también después de cerrar LFind.
- Nueva terminal integrada con PowerShell y CMD (Ctrl+J), que puede sincronizarse con el explorador en los dos sentidos.
- Integración con Windows opcional y reversible: abrir carpetas con LFind, menú contextual e inicio con el sistema.
- Papelera de reciclaje dentro de LFind: consulta, busca, restaura y elimina definitivamente, con el espacio que ocupa.
- «Abrir con» muestra las aplicaciones registradas en Windows, como WinRAR, y permite elegir otra con el cuadro nativo.
- Arrastra uno o varios archivos desde LFind al Explorador, al escritorio o a otras aplicaciones. Ahora también se puede soltar en un panel con la carpeta vacía.
- La barra lateral se puede llevar a la izquierda o a la derecha arrastrándola.
- El analizador separa lo local, lo que está solo en la nube y lo sincronizado, y permite filtrar por ello sin descargar nada.
- Los desplegables se leen bien en todos los temas.
- Corregida la línea extraña del doble panel.
- Si la integración con Windows abría una consola o una página inaccesible, se puede corregir desde Ajustes con «Reparar».

## Update 12: doble panel, personalización y analizador inteligente
- Nueva vista de doble panel: cada lado tiene su ruta, historial, pestañas y selección. Copia, mueve o arrastra archivos entre paneles, ajusta el divisor e intercámbialos (Ctrl+Mayús+D).
- Nuevo centro de personalización: fondos con tus imágenes, general o por panel, desenfoque, transparencia, colores, esquinas, densidad, tipografía y temas propios, con vista previa al momento.
- El analizador actualiza sus informes releyendo solo lo que ha cambiado, a mano o automáticamente, e indica cuándo se actualizaron y si los resultados son parciales.
- La documentación del proyecto está más ordenada.

## Update 11: organiza LFind a tu manera
- Arrastra las secciones de la barra lateral para ponerlas en el orden que prefieras.
- Reordena también carpetas fijadas, ubicaciones del sistema, recientes, unidades y pestañas, con una línea que indica dónde quedarán. Escape cancela.
- Nueva opción «Personalizar interfaz»: mueve o oculta las herramientas de la barra superior y vuelve a la distribución original cuando quieras. Fuera de ese modo, mantén Alt para mover una herramienta.
- Tu distribución se conserva al cerrar y abrir LFind, y restaurarla no afecta a tus ubicaciones fijadas.

## Update 09: navegación y organización de carpetas
- Recientes muestra solo las carpetas que has visitado de verdad, sin repetidas y con las últimas primero. Puedes quitar una o limpiar el historial.
- Fija cualquier carpeta en Ubicaciones con clic derecho, cámbiale el nombre visible y reordénala arrastrándola. Si una carpeta fijada desaparece, se marca sin borrarla.
- OneDrive aparece en la barra lateral, con una entrada por cada cuenta. Los archivos que solo están en la nube se distinguen y LFind no los descarga por mostrarlos.
- Nueva vista «Mi equipo» con todas las unidades, su tipo y su espacio libre. Se actualiza sola al conectar o retirar una unidad.
- Ordenar por fecha o tamaño empieza por lo más reciente o lo más grande, y el orden elegido se mantiene al navegar.

## Update 08: actualizaciones automáticas
- Nueva opción «Buscar actualizaciones» en la barra lateral: muestra la versión instalada, la nueva, las novedades y el progreso real de descarga, que se puede cancelar.
- LFind comprueba al iniciar si hay una versión nueva y solo avisa: nunca descarga nada sin tu permiso. Se puede desactivar.
- Las actualizaciones se verifican con firma criptográfica antes de instalarse y conservan tus preferencias y tu índice.

## Update 07: visor inteligente de archivos
- Vista previa integrada: panel lateral redimensionable y vista ampliada (Espacio), que comparten el mismo contenido. ←/→ recorren los archivos de la carpeta o de los resultados de búsqueda; Esc vuelve conservando la selección.
- Formatos: JPG, PNG, WebP, GIF animado, BMP, ICO, AVIF, SVG y TIFF con zoom, ajuste, tamaño real, desplazamiento y rotación; PDF y AI compatible con páginas, zoom y metadatos; PSD/PSB con la composición real, sin Photoshop; MP4, WebM y audio. Soporte parcial, siempre indicado: EPS, RAW, HEIC y MOV.
- Seguridad: protocolo propio con permiso por archivo, sin scopes globales ni CSP relajada. SVG y PDF no ejecutan nada; PostScript nunca se ejecuta.
- Rendimiento: carga bajo demanda, cancelación al cambiar de archivo, caché de 768 MB que se invalida si cambia el archivo, solo páginas visibles y sin procesos activos con el visor cerrado.
- Dependencia nueva: `pdfjs-dist` 4.10.38. El ejecutable pasa de 13,2 a 15,5 MB y el instalador de 3,6 a 5,6 MB.
- Rust 163/163, Vitest 217/217, instalador compilado y batería real en la app empaquetada. Detalle y límites: `UPDATE07_PROGRESS.md`.

## Update 06.0: estabilidad y fiabilidad
- **Errores de interfaz:** un error de renderizado ya no deja la ventana en blanco. `ErrorBoundary` a tres niveles (ventana, contenido, vista previa) con pantalla de recuperación, reintento limitado si el error se repite y recuperación automática al cambiar de carpeta o elemento. El usuario no ve trazas técnicas.
- **Registro local de errores:** `%LOCALAPPDATA%\app.nova.explorer\logs\lfind-errors.log` (errores de interfaz y pánicos de Rust; 512 KiB + un archivo rotado). Nuevo comando `log_ui_error`.
- **Pánicos de Rust:** un archivo malformado que haga entrar en pánico al decodificador de miniaturas cuenta ahora como fallo de ese archivo y no cierra LFind. Cualquier otro pánico se registra y termina la aplicación, como antes. Release pasa de `panic = "abort"` a desenrollado.
- **Coste:** el ejecutable crece de 8,1 a 13,2 MB y el instalador de 2,9 a 3,6 MB.
- **Pruebas:** corregidas dos pruebas Rust que fallaban en Windows por errores de las propias pruebas (el código de producción era correcto); `safety` ahora comprueba `..` de verdad en Windows.
- Resultados: Rust 137/137, Vitest 205/205, compilación de Tauri correcta. Detalle y pendientes: `UPDATE06_PROGRESS.md`. Sin dependencias nuevas.

## Update 05.1: acceso directo visible a la búsqueda global
- Nuevo botón permanente en la barra superior: lupa + «Buscar en todos los archivos» (a partir de 1240 px de ancho del panel principal muestra solo la lupa, con el mismo texto en `aria-label` y tooltip). Al pulsarlo abre la vista de búsqueda global existente y deja el cursor en el campo (con su texto seleccionado). Resalta (acento) mientras la búsqueda global está activa.
- `Ctrl+Mayús+F` se conserva y ahora llama a la MISMA acción (`useSearch.openGlobal()`), igual que el icono de globo del campo: una sola implementación. Filtros, texto, controles del indexador y estado del índice no se tocan; salir (Esc o el globo) devuelve a la carpeta como antes.
- Ventanas estrechas: el campo de búsqueda global cede ancho (190 px ≤900 px; 130 px ≤640 px) para que nada se desborde ni se encime; esto además corrige un desbordamiento de la barra que ya existía en modo global a ~560 px.
- Sin cambios en el motor de indexación, SQLite ni el esquema; sin dependencias nuevas.
- Verificado: pruebas de lógica y de cableado (5 nuevas), maquetación con el CSS real en 5 temas × 2 modos × 12 anchos. NO ejecutado: la app real (React/Tauri), Vitest, Windows.

## Update 05 (Fase 1): motor de búsqueda global con índice persistente
- Índice SQLite local (FTS5 trigram para subcadenas) de archivos y carpetas de todas las unidades fijas; indexado en segundo plano, por lotes, pausable y reanudable; sobrevive a reinicios y a cierres a mitad; exclusiones configurables (Windows, ProgramData, Recovery, papelera… por defecto).
- Búsqueda integrada en el buscador existente (Ctrl+Mayús+F): resultados al escribir, relevancia, filtros combinables (extensión, tipo, tamaño, fechas, unidad, carpeta, archivos/carpetas, exacta/parcial), virtualización, abrir y abrir ubicación; panel de estado del índice.
- Pendiente (Fase 2): observador `ReadDirectoryChangesW`. Hoy los cambios se recogen por reconciliación periódica.
- Corregido: import sin usar (`basename`) en `SearchView.tsx` que rompía `noUnusedLocals`.
- Medido en Linux con 1 M de filas sintéticas: consultas por nombre común 37–620 ms (no instantáneas); exactas y de filtros estructurados <2 ms. Detalle y limitaciones: `UPDATE05_PROGRESS.md`. No compilado en Windows.

## 04.3.3: causa raíz de las 3 pruebas de `themes.test.ts`
- **Causa raíz (diagnóstico por evidencia; Vitest no pudo ejecutarse en el entorno de esta corrección, ver `UPDATE04_PROGRESS.md`):** `import … from "….css?raw"` no devuelve el CSS en Vitest. El plugin CSS de Vitest intercepta cualquier id con extensión CSS (también con `?raw`) y, con `test.css` por defecto (CSS sin procesar), lo sustituye por una cadena vacía. El parser recibía `""` → 0 bloques. No era CRLF ni la expresión regular (por eso 04.3.2 no lo arregló).
- **Solución:** `themes.test.ts` lee `app.css`, `themes.css` y `tokens.css` con `readFileSync(new URL(…, import.meta.url))` (como ya hace `thumbLayout.test.ts`, que sí pasaba). Parser y aserciones intactos. Añadida una guarda en la primera prueba que falla si algún CSS llega vacío.
- **Efecto colateral importante:** las pruebas «toda variable de app.css está definida» y «app.css sin colores a mano» pasaban **sin comprobar nada** (leían ""). Ahora comprueban el contenido real; verificado con Node sobre los archivos reales: 0 variables sin definir, 0 colores a mano.
- Solo se modificó `src/lib/themes.test.ts`. `themes.css`, `app.css`, `vite.config.ts` y dependencias sin cambios.

## 04.3.2: pruebas de temas
- `themes.test.ts`: el analizador de `themes.css` ahora no depende del tipo de salto de línea (LF/CRLF/CR) ni de la posición de la llave de cierre; nueva prueba que lo comprueba. Temas, variables y valores sin cambios.
- **Hallazgo real:** en 04.3 puse `#e5484d` como valor de respaldo en `app.css` (barra de dirección) y eso rompe la prueba «app.css no contiene colores… escritos a mano». Corregido: `var(--danger)`.
- No pude reproducir un fallo de la expresión regular anterior con CRLF (daba 5 bloques × 62 variables con LF y con CRLF); si tras este cambio siguen fallando, envía el mensaje exacto. `npm test`/`typecheck`/`build` NO se ejecutaron aquí.

## 04.3.1: corrección de compilación TypeScript
- `tsconfig.json` solo para producción (excluye pruebas); nuevo `tsconfig.test.json` con tipos de Node para pruebas y `vite.config.ts`; `@types/node` añadido; `defineConfig` desde `vitest/config`. `strict` sin tocar.
- Los errores del menú nativo (`nativeMenu.ts`, `shellMenu.ts`…) vienen de archivos obsoletos que quedaron en la carpeta: `scripts/clean-obsolete.ps1` los borra. El menú nativo NO se reintroduce (decisión de 04.2).
- Recreado `UPDATE04_PROGRESS.md` (estado real). `package-lock.json` pendiente de regenerar con `npm install`. Nada de esto se compiló en Windows.

## Actualización 04.3: eliminación rápida, iconos de carpeta propios y barra de dirección

El menú contextual propio de 04.2 no se ha tocado. Detalle: `docs/UPDATE_04.3_REPORT.md` · registro técnico: `PROJECT_CONTEXT.md` · **verificación pendiente en Windows: `WINDOWS_VERIFICATION.md`** (`src-tauri` y la interfaz en ejecución no se pudieron compilar/probar aquí).

### Eliminación
- El elemento **desaparece de la vista al confirmar**, sin esperar al sistema; si falla (o se cancela/deshace) **vuelve a verse al instante** y se muestra el error. El aviso de éxito solo sale cuando Rust confirma el final.
- Papelera por **lotes de 64** (una operación del sistema por lote, con reintento individual para atribuir bien los errores) en vez de una por elemento. Sigue usando la Papelera.
- Eliminar permanentemente ya no recorre el árbol entero antes de empezar (en Linux ese recorrido añadía ~21–24 %); el progreso muestra «N eliminados».

### Iconos
- Las carpetas usan siempre el icono propio de LFind (`FolderGlyph`, vectorial, por tokens de tema); ya no se pide ni se pinta el icono de carpeta de Windows. Archivos (EXE, PDF, PSD…) y miniaturas, sin cambios. Revisado en Chromium: 5 temas × 9 tamaños (System usa dark/light).
- Pendiente de diseño: variantes de carpetas especiales, vacía/llena e insignia de enlace.

### Barra de dirección
- Migas «Este equipo › Disco local (C:) › Users › …» (rutas de red: `\\srv\share › …`); «Este equipo» abre la lista de unidades.
- Clic en el espacio vacío o `Ctrl+L`: campo editable con el texto seleccionado; Enter **valida en Rust** (existe, es accesible, tiempo límite 6 s para red) sin bloquear; si falla, mensaje claro, campo en rojo y se conserva la ubicación anterior; Escape cancela. Acepta comillas, `%VARIABLES%`, `/`, `C:` y rutas UNC. Un archivo abre su carpeta. Misma carpeta → recarga sin ensuciar el historial.
- Navegación: sigue siendo `navigate` + `open_dir` (no hay segunda implementación).

### Pruebas
Rust 37/37 (`ops`, `names`, `safety`, `undo`, `pathbar`) + 21 de 04.2; TypeScript 38/38 en el arnés propio; SVG/CSS de carpetas en Chromium. Nada probado en Windows.

---
## Actualización 04.2: menú contextual PROPIO (sustituye al nativo de 04/04.1)

Decisión de producto: LFind dibuja su menú; **no** usa `IContextMenu`/`QueryContextMenu` ni extensiones COM. Un único camino de código (ya no hay «a veces nativo, a veces propio»).
Detalle: `docs/CONTEXT_MENU.md`. Registro de desarrollo: `PROJECT_CONTEXT.md`. **Verificación: `WINDOWS_VERIFICATION.md` (`src-tauri` y las llamadas de Windows no se pudieron compilar ni ejecutar aquí).**

### Nuevo
- Menú propio en React/TypeScript (`contextMenu.ts` puro + `ContextMenu.tsx` con submenús y teclado completo) y operaciones de sistema en Rust (`system_cmd.rs`, `archive`, `sendto`, `newmenu`).
- Archivo / carpeta / selección múltiple / espacio vacío con acciones reales: **Abrir con…, Abrir terminal aquí, Comprimir en .zip, Extraer aquí, Enviar a ▸, Nuevo ▸ (plantillas del sistema)**, además de lo que ya había (cortar, copiar, pegar, renombrar, eliminar, propiedades, favoritos, ordenar, ver…).
- **Apertura instantánea**: el menú se construye de forma síncrona con datos ya en memoria (sin IPC ni consultas). «Enviar a», plantillas y estado del portapapeles se precargan en reposo.

### Conservado de 04/04.1
- Arreglo de miniaturas verticales · portapapeles de archivos compatible con el Explorador · Mayús+F10/tecla Menú con antirrebote · refresco tras acciones externas (0,4/1,5/4 s).

### Eliminado
- Todo el código del menú nativo: `shell_menu*.rs`, `shell_menu_cmd.rs`, `nativeMenu.ts`, `menuPerf.ts`, `shellMenu.ts` (+ pruebas), precalentamiento, medición y banco de pruebas, preferencia `nativeContextMenu` y su conmutador en Ver (las preferencias antiguas con esa clave se ignoran sin error).
- Docs obsoletos: `docs/NATIVE_CONTEXT_MENU.md`, `docs/MENU_PERFORMANCE.md`, `UPDATE04_PROGRESS.md`, `UPDATE041_PROGRESS.md` (lista en `scripts/obsolete-04.2.txt`).

### Consecuencia a tener en cuenta
- Las entradas de WinRAR, Git, Defender, «Compartir», etc. (extensiones COM de terceros) ya no aparecen en el menú. Sin «Compartir».

### Pruebas
- Rust (`rustc --test`): 21 OK (`archive` 4 —incluye extracción tar.gz real—, `newmenu` 3, `sendto` 4, `clipboard` 7, `names` 3); tipos FFI con `nova_winck`.
- TypeScript (arnés propio): `contextMenu.test.ts` 10/10 · `menus.test.ts` 2/3 (la tercera falla por una carencia del arnés —`toBeUndefined`—, no del código).

---
(Historial: las entradas 04.1 y 04 describen el menú nativo, **ya retirado**; se conservan solo como historia, salvo las miniaturas verticales y el portapapeles, que siguen vigentes.)

## Actualización 04.1: rendimiento del menú contextual nativo

Sobre la 04. Sin cambios en las entradas del menú, la compatibilidad con extensiones, el portapapeles ni el refresco. Detalle y metodología: `docs/MENU_PERFORMANCE.md`.
**Las mejoras de latencia del Shell NO se han podido medir (sin Windows en el entorno de desarrollo); el informe incluye el procedimiento y la tabla a rellenar.**

- **Medición por etapas siempre activa** (IPC, cola, PIDL, enlace, objeto, `QueryContextMenu`, `QueryInterface`, total hasta mostrar). Consola: `localStorage.setItem("lfind.menuTiming","1")`; resumen: `lfindMenuTimings()`.
- **Banco de pruebas** `lfindMenuBench()` (DevTools): construye menús sin mostrarlos para 1 archivo, 1 carpeta, 50 archivos, mixtos y espacio vacío, con PIDL clásico y rápido.
- **Precalentamiento** de las extensiones del Shell: al arrancar (1,5 s después, en un hilo) y por tipo de archivo al abrir carpetas (inactividad; una muestra por tipo, máx. 3 por carpeta y 40 por sesión). Un clic real adelanta a lo que no haya empezado. Desactivable: variable de entorno `LFIND_NO_PREWARM=1`.
- **PIDL rápido** (sin consultar el disco por elemento) con vuelta automática al método clásico; la interfaz envía qué elementos son carpetas. Desactivable: `localStorage.setItem("lfind.menuSlowPidl","1")`.
- **Posición** del menú: cursor real y origen/ppp de la ventana por Win32 directo; ya no se pregunta al hilo principal de Tauri.
- **Mensaje IPC** con carpeta + nombres (−60 % de bytes con ≥100 elementos, medido); `join_names` valida los nombres.
- Pruebas nuevas: Rust (`join_names`; 20 pruebas en total: 14 de `shell_menu` + 6 de `clipboard`), TypeScript (`menuPerf.test.ts`: medición y planificador de precalentamiento; `shellMenu.test.ts` actualizado).
- Corrige un desajuste de la 04: el código Rust de medición/precalentamiento existía pero no estaba registrado ni usado por la interfaz.

## Actualización 04: menú contextual nativo de Windows y miniaturas verticales

Incremental sobre la 03; no sustituye nada ni actualiza dependencias. Detalles técnicos: `docs/NATIVE_CONTEXT_MENU.md`.
**Estado de verificación: ver `WINDOWS_VERIFICATION.md`** (el menú nativo no se pudo ejecutar en Windows en el entorno de desarrollo).

### 1. Miniaturas verticales (verificado en Chromium)
- Causa: `.thumb` era `display:grid` y su imagen usaba `max-height:100%` (porcentaje cíclico); la imagen vertical se salía de su caja y tapaba el nombre.
- Arreglo general (no específico de imágenes verticales): `.thumb` flex con `overflow:hidden`, imagen con `max-width/max-height:100%` + `object-fit:contain`;
  `.gname` con `min-width:0` y `overflow-wrap:anywhere` (los nombres largos no ensanchan la celda). Comprobado con 4 proporciones × 8 tamaños: antes 16/32 solapes, ahora 0/32.
- Prueba de regresión: `src/styles/thumbLayout.test.ts`.

### 2. Menú contextual nativo (implementado; pendiente de verificar en Windows)
- Clic derecho en archivos, carpetas, selección múltiple y espacio vacío → menú REAL del Shell (`IContextMenu`/`2`/`3`) con las extensiones instaladas;
  nada de WinRAR/Git/Defender/Terminal está escrito a mano. Mayús+F10 y tecla Menú. Submenús nativos (`HandleMenuMsg2`), posición del cursor, modo oscuro/claro (mejor esfuerzo).
- Hilo del Shell propio con COM STA, ventana propietaria oculta y bucle de mensajes: la interfaz nunca espera al Shell; todo COM se libera (RAII).
- LFind añade arriba sus entradas propias y ejecuta él mismo solo `delete`, `rename`, `cut`, `copy`, `paste` y `open` en carpetas (colas, deshacer, renombrado en línea, pestañas).
- Portapapeles de archivos sincronizado con el de Windows (`CF_HDROP` + *Preferred DropEffect*).
- Refresco tras acciones externas (0,4 s / 1,5 s / 4 s). Si el menú nativo no se puede mostrar → menú propio anterior, intacto.
- Preferencia `nativeContextMenu` (activada por defecto): Ver › «Usar el menú contextual de Windows».
- Limitaciones documentadas: menú clásico en Windows 11, extensiones solo `IExplorerCommand`, sin «site» de vista, extensiones dentro del proceso.

### Pruebas añadidas
- Rust (`rustc --test`): `shell_menu` (10) y `clipboard` (6). TypeScript: `shellMenu.test.ts` (16), menú Ver (1), `thumbLayout.test.ts` (3).

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
