# LFind · PROJECT_STATUS

Estado vigente del proyecto. Es el único documento de estado: las correcciones pequeñas se anotan aquí, sin crear informes nuevos.
Historial completo, incluido el inventario anterior: [docs/history/](docs/history/) ([estado hasta Update 11](docs/history/PROJECT_STATUS_hasta-update-11.md)).

**Publicado:** v0.1.5 (Update 11). **En el árbol, sin commit:** Update 12, con su ronda final de correcciones ([UPDATE12_PROGRESS.md](UPDATE12_PROGRESS.md)).

## Funcionalidades

| Área | Estado |
|---|---|
| Exploración: pestañas, cinco modos de vista con tamaños, orden, agrupación, operaciones de archivo con deshacer | Terminado |
| Doble panel: rutas, historial, pestañas, selección y filtro independientes; copiar y mover entre paneles | Terminado (Update 12) |
| Búsqueda global con índice SQLite | Terminado |
| Visor inteligente y miniaturas | Terminado |
| Analizador de almacenamiento | Terminado (Update 10) |
| Actualización incremental de informes: manual, automática o desactivada | Terminado (Update 12) |
| Reorganizar secciones, filas, herramientas y pestañas | Terminado (Update 11) |
| Personalización visual: fondos, colores, forma, tipografía, temas propios | Terminado (Update 12) |
| Actualizaciones automáticas firmadas | Terminado (Update 08) |

## Última verificación (Update 12)

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` (incluye las pruebas) | sin errores |
| Vitest | 358/358 |
| Rust con SQLite, `--locked` | 247 + 3 correctas |
| Rust sin SQLite | 217 + 3 correctas |
| `npm run build` | correcto |
| `tauri build` del producto, sin artefactos de actualización | instalador generado |

En la app real, con una instalación aislada (preferencias e índice reales intactos):

- **Analizador en C:** análisis completo de 2 349 532 archivos en 198 s. Actualización incremental en 63 s, 50 carpetas releídas, 163 MB de memoria, con el archivo nuevo encontrado.
- **Vigilancia automática:** detecta cambios de tamaño que las fechas de carpeta no ven, también con el archivo abierto y ya volcado a disco (64 s, tamaño exacto). Lo que no cambia nada no reescribe el informe.
- **Un informe actualizado coincide con uno hecho desde cero** (pruebas Rust con altas, bajas, cambios repetidos y carpetas nuevas).
- **Doble panel:** rutas, historial, selección y orden independientes; copiar, mover y arrastrar con archivos reales; divisor; intercambio; búsqueda en un solo panel; reinicio; seis temas; seis anchos sin desbordes.
- **Personalización:** imagen de 3600×2400 reducida a 112 KB en 0,3 s; imagen inválida rechazada; controles en vivo; temas propios; persistencia tras reiniciar; restaurar; preferencias dañadas sin perder ubicaciones; seis temas.
- **Regresión de Update 11:** 51 comprobaciones, sin fallos, y 144 combinaciones visuales sin desbordes.

## Limitaciones y pendientes

- **Ratón físico:** el arrastre, el divisor y las filas se han probado con entrada del navegador y mensajes de Windows, no con un ratón en manos de una persona.
- **Actualización por fechas:** Windows no cambia la fecha de una carpeta cuando un archivo suyo solo cambia de tamaño. Por eso esa vía se marca como «resultados parciales»; la vigilancia automática sí lo ve, pero solo mientras LFind está abierto y una vez que el archivo se vuelca o se cierra. No se usa el diario USN porque exige permisos de administrador.
- **Vigilancia:** una ubicación a la vez, la del informe abierto. En una unidad completa actualiza como mucho cada 30 minutos y reconcilia cada 6 horas (carpetas: 5 minutos y 1 hora).
- **Actualización sin barra de progreso:** en una unidad grande tarda alrededor de un minuto, con solo el botón «Actualizando…».
- **Doble panel:** las pestañas visibles arriba son las del panel activo. El orden propio de cada panel no sobrevive a un reinicio.
- **Archivos en la nube:** solo tienen miniatura si Windows ya la guardó; en una muestra real de OneDrive, 1 de cada 400. El resto muestra el icono con la nube y nunca se descargan.
- **Personalización:** los fondos se limitan a 1920 px. La legibilidad se protege con una opacidad mínima y avisos de contraste, no con un análisis de cada imagen.
- **Sin comprobar con hardware real:** unidades extraíbles, de red y Google Drive en el analizador; OneDrive de empresa.
- **«Deshacer» de un aviso anterior:** siempre deshace la última operación.
- **Notas de versión:** `CHANGELOG.md` no tiene sección de Update 10.

## Riesgos técnicos abiertos

Detalle y ubicación en el código: [estado hasta Update 11, §6](docs/history/PROJECT_STATUS_hasta-update-11.md).

- **R-04:** sin instancia única; dos indexadores pueden cruzarse.
- **R-06 y R-07:** el índice reescribe todas las filas en cada pasada y guarda la ruta tres veces. Corregirlo obliga a reconstruirlo.
- **R-02:** un pánico fuera de las zonas aisladas cierra la aplicación a propósito.
- **R-03:** un fallo al leer la raíz de una unidad se cuenta como omitido y el barrido puede borrar sus filas.
