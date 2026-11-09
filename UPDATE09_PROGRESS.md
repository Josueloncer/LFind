# Update 09: navegación y organización de carpetas

Estado: **implementado, validado localmente y autorizado para su publicación**. La versión la asigna el workflow de publicación (en el código sigue 0.1.0).

## Causas encontradas

| Problema | Causa real | Corrección |
| --- | --- | --- |
| Orden por fecha empezaba por lo más antiguo | `Pane.onSort` usaba siempre `asc` al elegir columna; el menú conservaba el sentido anterior. No era un fallo de Rust. Además había vistas guardadas en `modified/asc`. | `defaultSortDir` por columna. Migración v3 de las vistas guardadas. |
| Fechas desconocidas quedaban primero | `toColumns` las convertía en 0 | `NaN`, siempre al final; igual en SQL y en `MemStore`. |
| El orden no se mantenía al navegar | Se guardaba solo por carpeta | También pasa a la vista global, que heredan las carpetas sin orden propio. |
| Recientes contenía carpetas no visitadas | `pushRecent` se llamaba en cada carpeta `ready`, incluida la de arranque | Solo visitas reales, con permanencia de 1,5 s (`recordVisit`). |
| Miniaturas, iconos y vistas previas descargarían archivos de OneDrive | Abrían el archivo sin comprobar los atributos de nube | Guardas en `thumbs`, `icons`, `viewer` y `preview`. |

## Qué se hizo

- **Recientes:** carpetas realmente visitadas, sin duplicados, con fecha, máximo 15, 6 visibles con «Mostrar más». Sin carpeta de arranque, recargas ni carpetas de paso. Si una carpeta deja de existir sale sola; una unidad que no responde no borra nada. Se puede quitar una o limpiar todo.
- **Ubicaciones:** una sola sección. Fijar desde el menú de carpeta, el fondo, la búsqueda o la barra. Renombrar la etiqueta, reordenar arrastrando o desde el menú, detectar carpetas inexistentes sin borrarlas. Los duplicados por mayúsculas o barras se unifican.
- **OneDrive:** cuentas leídas de `HKCU\Software\Microsoft\OneDrive\Accounts` (variables de entorno solo como respaldo). Insignia para archivos solo en la nube.
- **Mi equipo:** ruta virtual `lfind://computer` con tarjetas por tipo de unidad. Detección por `GetLogicalDrives` cada 4 s, solo con la ventana visible. Capacidad consultada por unidad, sin acumular llamadas.
- **Migración de preferencias a v3:** convierte `recents` en `recentFolders`, conserva favoritos y vistas.

## Pruebas ejecutadas

- Rust: 174 + 3 sin SQLite; 175 + 3 con SQLite. Vitest: 264/264. TypeScript sin errores.
- Instalador de producción generado con `npm run tauri build` (sin artefactos de actualización).
- Punta a punta con una instalación aislada (`LFindNavTest`) y una copia del `prefs.json` real: migración, historial, fijados, reordenación, rutas ausentes, orden en 6 000 archivos (Worker) y en System32, búsqueda global sobre un índice aislado, unidades que aparecen y desaparecen (`subst`), OneDrive real, reinicio y seis temas.
- OneDrive real: 1 154 archivos solo en la nube siguen igual tras listarlos con miniaturas activas. Miniatura, visor, texto e icono rechazan o evitan la lectura en imagen, PDF, vídeo y texto.
- Índice real (`lfind-index.db`): mismo tamaño (1 998 790 656 bytes) y misma fecha antes y después. `prefs.json` real: mismo hash.

## Revisión final antes de publicar

- **Insignias en la búsqueda:** el índice no guarda el dato y añadirlo obligaría a reconstruirlo. Nuevo comando `cloud_only` (`GetFileAttributesW`, sin abrir el archivo), solo para filas visibles dentro de OneDrive, máximo 128 rutas, con espera al desplazarse y validez de 30 s (`useCloudHits`).
- **Insignia y nombres largos:** en iconos, insignia y nombre comparten una fila flexible y se recorta el nombre. En detalles se añade margen frente a la columna de fecha.
- **Arrastre de Ubicaciones:** identifica por ruta, no por índice. Las posiciones que no cambian nada (`canMovePin`) no aceptan soltar ni muestran indicador. Línea de inserción dibujada sobre la fila, fila de origen atenuada, sin parpadeo al pasar por elementos internos.
- **Comprobado:** 120 combinaciones de carpeta (6 temas, 5 vistas, 4 anchos) y 24 de búsqueda (6 temas, 4 anchos) sin ningún fallo de alineación. Los anchos se simularon con emulación del navegador. Los 21 resultados coincidieron con los atributos de Windows.

## Limitaciones y lo no comprobado

- **OneDrive de empresa:** este equipo no tiene ninguna cuenta con carpeta; solo está cubierto por pruebas unitarias de `cloud::resolve`.
- **Unidades extraíbles, ópticas y de red reales:** no hay ninguna en este equipo. Se probó la aparición con `subst` (tipo fijo) y una ruta UNC caída (3 s, sin bloquear). El estado «sin medio» no se ha visto con hardware real.
- **Cambio de disco en la misma letra:** no altera la máscara; la capacidad se actualiza al entrar en Mi equipo o al recuperar el foco (margen de 20 s).
- **Insignias en la búsqueda:** solo dentro de las carpetas de OneDrive de las cuentas detectadas. Una biblioteca de SharePoint sincronizada fuera de ellas no muestra insignia, aunque el backend tampoco la descarga.
- **Reordenar arrastrando:** probado con entrada real de Windows (`mouse_event`) en la instalación aislada, que recorre el arrastre nativo del sistema. No lo ha hecho una persona con un ratón físico.
- **Búsqueda con el índice real de ~2,9 M:** no se ejecutó, para no tocarlo. El SQL de orden usa el índice existente en descendente.
