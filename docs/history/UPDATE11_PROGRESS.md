# Update 11: personalización de barras y organización arrastrando

Estado: **implementado y validado localmente. Sin commit, sin push, sin Release y sin cambio de versión.** Se añade a los cambios de Update 10, que tampoco tienen commit.

## Qué se puede organizar

| Barra | Qué se mueve | Cómo |
| --- | --- | --- |
| Barra lateral: secciones | Ubicaciones, Recientes, Mi equipo, Almacenamiento | Arrastrando el título |
| Barra lateral: filas | Carpetas fijadas, ubicaciones del sistema y OneDrive, Recientes, unidades | Arrastrando la fila |
| Pestañas | Orden de las pestañas (no se guarda, igual que las pestañas) | Arrastrando la pestaña |
| Barra superior | 12 herramientas: orden, lado izquierdo o derecho, ocultar | `Alt` + arrastrar, o el modo de personalización |

Elementos fijos, por motivos funcionales:

- **Barra de dirección:** no se mueve; es la referencia que separa la zona izquierda de la derecha.
- **Campo de filtro:** se mueve pero no se oculta, porque es también el campo de la búsqueda global.
- **«Todas las unidades»:** primera fila de Mi equipo.
- **Pie de la barra lateral** (Personalizar, Tema, Actualizaciones): fijo y siempre al alcance, para poder salir de cualquier distribución.
- **Controles de ventana y arrastre del título:** los gestiona el sistema; reorganizarlos rompería mover y cerrar la ventana.

Barras sin reorganización:

- **Barra de estado:** solo muestra datos y el control de tamaño.
- **Barras de búsqueda, visor y analizador:** son controles propios de cada vista, con pocas acciones relacionadas entre sí. Como alternativa, las secciones del analizador y sus filtros siguen el orden lógico de su contenido.

## Arquitectura

- `src/hooks/useReorder.ts`: motor único con eventos de puntero. Umbral de 5 px, indicador de inserción, copia flotante, autodesplazamiento en la barra lateral, animación al colocar, Escape y pérdida de foco cancelan.
- `src/lib/reorder.ts` y `src/lib/uiLayout.ts`: lógica pura de orden y distribución.
- `src/stores/customize.ts` y `src/components/CustomizeBar.tsx`: modo «Personalizar interfaz».
- `Sidebar.tsx`, `Toolbar.tsx`, `TitleBar.tsx`, `App.tsx`: integración.
- `stores/prefs.ts`, `stores/nav.ts`, `lib/fs/types.ts`, `app.css`, `icons`: estado, tipos y estilos.
- `crates/nova-core/src/prefs.rs`: nuevo campo `layout`.

## Decisiones técnicas

- **Punteros en vez de HTML5 para reordenar.** Permite umbral, Escape y animación iguales en todas las barras. Las filas siguen siendo destino del arrastre de archivos, que no cambia.
- **Sin nueva versión de preferencias.** `layout` se añade a la versión 3 con valor por defecto. Un archivo sin él usa la distribución original; no hace falta migración y la versión anterior de la app sigue pudiendo leer el archivo.
- **Recuperación.** Normalmente un campo dañado invalidaría todo `prefs.json`; `layout` se lee aparte, de modo que si está dañado solo se pierde la distribución, nunca las ubicaciones.
- **Identificadores estables.** Secciones y herramientas por id fijo; ubicaciones y unidades por ruta normalizada. Lo guardado que ya no existe se ignora y lo nuevo aparece al final.
- **Alt fuera del modo.** Las filas de la barra lateral se arrastran directamente; los botones de la barra superior necesitan `Alt` o el modo, para que un clic nunca los desplace.
- **Recientes.** El orden manual cambia la lista y conserva fechas y acciones; una visita nueva sigue entrando arriba.
- **Modo.** Con él activo, los botones de la barra superior no ejecutan su acción. Si la barra lateral está en raíl se expande mientras dura, sin cambiar la preferencia. Restaurar solo afecta a la distribución.

## Pruebas ejecutadas

| Comprobación | Resultado |
| --- | --- |
| Rust con SQLite | 203 + 3 correctas |
| Rust sin SQLite | 188 + 3 correctas |
| Vitest | 293/293 |
| TypeScript | sin errores |
| `npm run build` | correcto |
| `tauri build` del producto, sin artefactos de actualización | instalador generado |

Punta a punta con una instalación aislada (`LFindNavTest`), eliminada al terminar, y entrada de ratón enviada por CDP a su ventana (51 comprobaciones, sin fallos):

| Prueba | Resultado |
| --- | --- |
| Secciones | arriba y entre dos, con indicador y animación; no navega |
| Carpetas fijadas | ambos sentidos; rutas y nombres personalizados intactos, sin duplicar |
| Ubicaciones del sistema, unidades y Recientes | reordenados; las unidades conservan su identidad y Recientes sus fechas |
| Herramientas | sin `Alt` no se mueven; un clic con 3 px de movimiento abre el menú; con `Alt` o en el modo se mueven, también de lado |
| Ocultar y mostrar | guardado; atenuada en el modo y ausente fuera |
| Reinicio real del proceso | distribución y ubicaciones conservadas |
| Restaurar | distribución original, con ubicaciones y recientes intactos |
| Escape | cancela filas, secciones y herramientas; durante un arrastre no sale del modo; fuera de él, sí |
| Posiciones no válidas | sobre sí misma y adyacente equivalente sin cambios; la barra de dirección no se arrastra; el botón de quitar no inicia arrastre |
| Clics | clic, clic derecho y pestañas funcionan |
| `layout` dañado / archivo sin `layout` | se conservan las 4 ubicaciones, sin `.bak` |
| Raíl | filas movibles; el modo expande sin cambiar la preferencia |
| Aspecto | 144 combinaciones (6 temas, 6 vistas, 4 anchos) con barra personalizada, sin desbordes; anchos emulados |
| Otras funciones | analizador, buscador e índice siguen funcionando; preferencias e índice reales intactos |

La primera pasada dio 2 de 144 avisos a 640 px, medidos durante la animación del filtro; con más margen, 0 de 144.

## Corrección: ubicaciones que no se podían mover

**Causa.** Las carpetas fijadas y las ubicaciones del sistema (Inicio, Escritorio…) se veían como una lista, pero el código las trataba como dos. Con una sola carpeta fijada, esa lista tenía un elemento y no había adónde moverlo. Las pruebas anteriores usaban cuatro fijadas, por lo que no lo detectaron. El motor de arrastre funcionaba.

**Solución.** Ubicaciones es ahora una sola lista, guardada en `layout.places` (`locationOrder`, `moveLocation`). Las carpetas fijadas sin posición guardada siguen arriba; «Subir» y «Bajar en la lista» usan el mismo orden.

**Defecto adicional corregido.** La copia flotante del arrastre quedaba fuera de la pantalla porque `[data-movable]` anulaba su `position: fixed`.

Vitest 298/298, TypeScript sin errores, `npm run build` e instalador correctos. Rust no se volvió a ejecutar porque no cambió.

Prueba con mensajes nativos de ratón de Windows (`PostMessage` a la ventana de la app aislada) y una sola carpeta fijada: primera al último lugar, última al primero, entre dos, varias veces, Escape, sobre sí misma, clic normal, reinicio y quitar, todo correcto. Regresión completa de secciones, herramientas y pestañas: 51 comprobaciones sin fallos.

## Corrección: iconos de EXE y JAR pequeños y desplazados

**Causa.** No son miniaturas: EXE, JAR y similares usan el icono del sistema. Para tamaños de 49 px o más, LFind pedía la lista de 256 px. Si un archivo no incluye un icono de ese tamaño, Windows no lo escala y lo deja en la esquina superior izquierda de un lienzo transparente; al reducir ese lienzo, el icono quedaba diminuto y arrinconado. El CSS no intervenía. Medido: el JAR llegaba como 48 px dentro de 256, y 59 de 549 ejecutables reales tenían el mismo problema.

**Solución.** `without_corner_padding` (`crates/nova-core/src/icons.rs`, aplicada en `icons_win.rs`) recupera el marco original del icono cuando todo su contenido cabe en el cuadrante superior izquierdo. No deforma ni cambia píxeles ni transparencia. Se sube la versión de la caché de iconos para no reutilizar los guardados.

**Resultado.** Con iconos reales a 48, 64, 128 y 256 px: el JAR y los 59 ejecutables arrinconados pasaron a 0. De los 490 ejecutables que ya estaban bien, 487 quedaron idénticos. En los otros 3, Windows entregó esa vez la versión de 48 px; una pasada posterior devolvió los iconos de ese grupo a tamaño completo. Revisado en detalles, lista, mosaicos, contenido e iconos a 64, 128 y 256 px, y en los seis temas (16 iconos en cada uno). La miniatura de una imagen se sigue mostrando, y el código de miniaturas no cambia. Rust 205 + 3 con SQLite y 190 + 3 sin él; Vitest 298/298; TypeScript sin errores; `npm run build` e instalador correctos.

**Ajuste posterior: sin ampliar.** `EntryIcon` forzaba cada icono a llenar su caja, y eso pixelaba los de 48 px. Ahora los iconos del sistema se muestran como máximo a su resolución real, un píxel del icono por píxel de pantalla, centrados en la misma caja (`object-fit: scale-down` y `srcSet` según la escala de pantalla). Los iconos con resolución suficiente siguen llenando la caja. Las miniaturas no cambian.

## Limitaciones y lo no comprobado

- **Ratón físico: sin comprobar.** La entrada se envió por CDP, que recorre el mismo proceso de eventos del navegador, pero ninguna persona ha arrastrado con un ratón real. No se usó el cursor del sistema porque el equipo estaba en uso.
- **Soltar archivos sobre filas:** el código no cambia, pero en esta actualización no se repitió con un arrastre real de archivos.
- **Raíl:** las secciones no se arrastran ahí porque no tiene títulos; se hace desde el modo, que lo expande.
- **Ventanas estrechas:** las herramientas marcadas se ocultan solas igual que antes.
- **Unidad cambiada de letra:** la identidad es la ruta (`c:`), así que otro disco en la misma letra hereda esa posición.

## Pasos pendientes

- Prueba con ratón físico.
- Autorización para commit y publicación. `CHANGELOG.md` no tiene sección de Update 10 y su primera sección son las notas de la versión.
