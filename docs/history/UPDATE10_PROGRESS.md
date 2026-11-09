# Update 10: analizador de almacenamiento, orden por fecha y Ubicaciones

Estado: **implementado y validado localmente. Sin commit, sin push, sin Release y sin cambio de versión.**

## Arquitectura del analizador

| Pieza | Archivo | Función |
| --- | --- | --- |
| Recorrido | `crates/nova-core/src/analyzer/walk.rs` | Un hilo, solo metadatos, identificadores en preorden, totales al cerrar cada carpeta. Se prueba sin SQLite. |
| Informes | `crates/nova-core/src/analyzer/report.rs` | Una base SQLite por ubicación, un trabajo a la vez, consultas paginadas. |
| Comandos | `src-tauri/src/analyzer_cmd.rs` | 14 comandos; las consultas van a hilos de trabajo. |
| Estado | `src/stores/storage.ts` | Trabajo en curso, informes y eventos. |
| Vista | `src/components/StorageView.tsx`, `StorageTreemap.tsx`, `src/lib/treemap.ts` | Portada, informe, progreso y mapa. |

Rutas virtuales: `lfind://storage` y `lfind://storage?root=…&at=a/b` (`src/lib/path.ts`). Por eso Atrás, Adelante, Subir, las pestañas y las migas funcionan como en cualquier carpeta.

Informes en `%LOCALAPPDATA%\app.nova.explorer\analyzer\<hash>.db`, fuera del índice de búsqueda (`index\`). Se escribe en `<hash>.tmp` y solo se sustituye el informe anterior al terminar bien.

## Decisiones técnicas

- **Preorden.** Los archivos se guardan en orden de recorrido, así que el subárbol de una carpeta es un intervalo de identificadores. No hace falta un índice por carpeta y en memoria solo está la rama actual.
- **Sin leer contenido.** `DirEntry::metadata` en Windows devuelve lo que trae la enumeración de la carpeta; ningún archivo se abre.
- **Enlaces.** Enlaces simbólicos, uniones y puntos de montaje se cuentan, pero no se siguen ni suman tamaño. Analizar directamente un enlace se rechaza.
- **Nube.** Los archivos solo en la nube se suman aparte. Las carpetas con `RECALL_ON_OPEN` o `RECALL_ON_DATA_ACCESS` no se enumeran, porque hacerlo traería su contenido de la red; se informa cuántas son.
- **Prioridad.** `THREAD_MODE_BACKGROUND_BEGIN` (CPU y disco) y pausas: cada 1024 entradas en unidades completas y cada 4096 en carpetas.
- **Progreso.** Instantánea de tamaño fijo cada 250 ms (24 archivos, 32 carpetas, 12 categorías).
- **Identidad del informe.** Hash de la ruta normalizada, comprobado al abrir, y número de serie del volumen para unidades con letra.
- **Caché limitada.** Se conservan 24 informes; al superarlos se retira el analizado hace más tiempo. Se pueden quitar a mano desde la portada.
- **Mapa de un solo tono.** La intensidad refuerza el tamaño y el nombre identifica cada bloque; así no depende de distinguir colores en seis temas.
- **Orden.** Las dos fechas (modificación y creación) mezclan carpetas y archivos. Nombre, tamaño y tipo mantienen las carpetas delante, porque una carpeta no tiene tamaño propio en esas listas.
- **Icono de quitar.** Solo con el cursor encima y no justo después de abrir la ubicación; reaparece al salir y volver. Sin el puntero sobre la fila, tampoco recibe clics.

## Archivos

- Nuevos: `analyzer/mod.rs`, `analyzer/walk.rs`, `analyzer/report.rs`, `analyzer_cmd.rs`, `StorageView.tsx`, `StorageTreemap.tsx`, `stores/storage.ts`, `lib/treemap.ts`, `lib/treemap.test.ts`.
- Modificados: `nova-core/src/lib.rs`, `sys.rs`, `src-tauri/src/lib.rs`, `App.tsx`, `ComputerView.tsx`, `Pane.tsx`, `SearchView.tsx`, `Sidebar.tsx`, `Toolbar.tsx`, `icons.ts`, `useDropTarget.ts`, `addressBar.ts`, `contextMenu.ts`, `fs/{browser,tauri,types}.ts`, `locations.ts`, `menus.ts`, `path.ts`, `sortCore.ts`, `viewState.ts`, `app.css` y sus pruebas.

## Comandos

```
npx tsc --noEmit -p .
npx vitest run
cd crates/nova-core && cargo test --features sqlite && cargo test
npm run build
npx tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

## Pruebas ejecutadas

| Comprobación | Resultado |
| --- | --- |
| Rust con SQLite | 200 + 3 correctas |
| Rust sin SQLite | 185 + 3 correctas |
| Vitest | 280/280 |
| TypeScript | sin errores |
| `npm run build` | correcto |
| `tauri build` del producto, sin artefactos de actualización | instalador generado |

Punta a punta con una instalación aislada (`LFindNavTest`, otro identificador), eliminada al terminar:

| Prueba | Resultado |
| --- | --- |
| Carpeta pequeña | 13 archivos, 8 carpetas, suma exacta (7 827 924 bytes), sin contar enlaces ni la carpeta sin permiso |
| Carpeta de 60 000 archivos | 195 ms, informe de 2,6 MB, 56 MB de memoria |
| `C:\Windows` | 185 323 archivos, 127 711 carpetas, 42,4 GB, 53 sin permiso, 73 s, informe de 31 MB |
| Pausa, reanudar, cancelar (`C:\Windows`) | la pausa detuvo el recuento (3279 → 3279), reanudar continuó (→ 5641), cancelar paró en 76 ms; 0 temporales, sin informe y sin hilos de más (43 → 43) |
| Unidad C: completa | 2 342 435 archivos, 357 655 carpetas, 1,5 TB, 129 sin permiso, 118 enlaces, 4 min 21 s, informe de 197 MB, 165 MB de memoria, 0,41 núcleos de media, bloqueo máximo de la interfaz 19 ms |
| Consultas sobre C: | páginas de archivos 5-29 ms; filtros 10-11 ms; mapa 4 ms; todas las carpetas 67-68 ms; distribución de `Users` 507-610 ms |
| OneDrive real | 233 655 archivos, 219 894 solo en la nube (627 GB): el mismo número antes y después |
| Uniones con ciclo y hacia fuera | no se siguieron |
| Archivo solo en la nube (`attrib +O`) | contado aparte, atributo intacto |
| Orden mezclado | correcto en detalles, lista, mosaicos, contenido e iconos en dos tamaños |
| Alternancia | reciente → antiguo → reciente, por menú y por cabecera |
| Persistencia del orden | correcta al navegar y tras recargar |
| Icono de quitar | oculto tras entrar con el cursor encima y sin cursor; visible al salir y volver a pasar |
| Ubicaciones | reordenar, etiqueta distinta, mayúsculas, ruta ausente, soltar sobre sí misma (no se acepta); orden conservado tras cerrar y abrir el proceso; quitar desde el menú |
| Filtros, profundizar, Subir, abrir ubicación con el archivo seleccionado, reabrir sin escanear, paginación | correctos |
| Aspecto | 96 combinaciones (6 temas, 4 anchos, 4 secciones) sin desbordamientos; anchos emulados |

## Limitaciones y lo no comprobado

- **Arrastre con ratón físico:** no. El equipo estaba en uso, así que se dejó de mover el cursor real. Los eventos `DragEvent` se dispararon sobre las filas reales de la app; el puntero, con entrada del navegador por CDP. El arrastre nativo se comprobó en Update 09, antes de pasar la lógica de soltar al contenedor de la lista.
- **Tamaños:** son lógicos. Los enlaces duros se cuentan una vez por ruta y la compresión no se refleja, por lo que la suma puede superar el espacio usado (en C: pasó). La interfaz lo explica.
- **Discos de otros tipos:** no se analizaron unidades extraíbles, de red ni de Google Drive. El aviso de «otro disco en esta letra» solo tiene la lógica; no se probó cambiando un disco.
- **Carpetas de nube sin explorar:** comprobado el atributo con pruebas unitarias; en OneDrive real no apareció ninguna.
- **Distribución de carpetas muy grandes:** se calcula al entrar (unos 0,5-0,6 s en `Users`).
- **Sin ejecutar en la interfaz:** quitar un informe y la retirada al superar 24 informes (quitar sí tiene prueba Rust). Cancelar justo mientras se guarda el informe.

## Pasos pendientes

- Revisión del usuario y prueba con ratón físico.
- Autorización para commit y publicación. Antes de publicar hay que añadir la sección de Update 10 a `CHANGELOG.md`, cuya primera sección son las notas de la versión.
