# LFind · PROJECT_STATUS

Estado vigente del proyecto. Es el único documento de estado: las correcciones pequeñas se anotan aquí, sin crear informes nuevos.
Historial completo, incluido el inventario anterior: [docs/history/](docs/history/) ([estado hasta Update 11](docs/history/PROJECT_STATUS_hasta-update-11.md)).

**Publicado:** Update 12. **En el árbol, sin commit:** Update 13 ([informe](docs/history/UPDATE13_HERRAMIENTAS.md)).

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
| Paleta de comandos (Ctrl+K) | Terminado (Update 13) |
| Inspector de archivos (Ctrl+I) | Terminado (Update 13) |
| Gestor de operaciones: pausa, velocidad, tiempo restante, avance conjunto | Terminado (Update 13) |
| Detector de duplicados | Terminado, con límites a gran escala (Update 13) |
| Terminal integrada: PowerShell y CMD (Ctrl+J) | Terminado (Update 13) |
| Integración con Windows | Terminado dentro de lo que Windows permite (Update 13) |
| Papelera de reciclaje | Terminado; «Vaciar» solo comprobado hasta su confirmación |
| Arrastre nativo hacia otras aplicaciones | Implementado; **sin probar con destino externo** (ver abajo) |

## Última verificación (Update 13)

Resultados, mediciones y limitaciones completas: [informe de Update 13](docs/history/UPDATE13_HERRAMIENTAS.md).

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| `npm test` | 406/406 |
| Rust con SQLite, `--locked` | 302 + 3 correctas |
| Rust sin SQLite, `--locked` | 247 + 3 correctas |
| `npm run build` y `tauri build` | correctos |

### Ronda correctiva de Update 13

**Detector sobre `C:\Dev`** (193 179 archivos, 187 446 candidatos, 13,6 GB leídos, 15 735 grupos, 4,31 GB recuperables):

| | Antes | Ahora |
|---|---|---|
| Análisis completo | sin terminar a los 480 s | 218 s |
| Segunda pasada, con caché | no medida | 126 s, 0 bytes leídos |
| Memoria máxima del motor | 30 MB | 32 MB |

Causas: un solo hilo con cuatro aperturas por archivo, un límite de lectura de 64 MB/s y una apertura más por copia al guardar. El avance se guarda en disco por páginas.

En la app real: pico de 61 MB, ~81 % de un núcleo, 15 GB de disco, IPC con mediana de 3 ms y peor caso de 13 ms. En pausa: 0 MB leídos y 0 % de CPU. Cerrar LFind (con el proceso forzado) durante la lectura completa y reanudar: siguió por la etapa en que estaba y terminó con los mismos 15 735 grupos. Al abrir LFind con un análisis pendiente: 0 MB leídos, no se reanuda solo.

Comprobación independiente: otro programa recalculó el SHA-256 de 400 grupos al azar (4 390 archivos), sin ninguna diferencia.

**Inspector con archivos reales:**

| Tipo | Resultado |
|---|---|
| Vídeo (MP4, MOV, MKV) | duración, resolución, FPS y códec con su nombre; leídos 0 KB |
| PowerPoint, Word, Excel locales | autor, páginas, palabras y aplicación, según lo que guarde el archivo; hasta 503 KB leídos |
| PDF locales | versión siempre (≤ 338 KB leídos, también en uno de 545 MB). Páginas: de 8 PDF comparados con pdf.js, 4 correctas; en los otros 4 no se da el dato porque no se puede saber sin descomprimir o por revisiones posteriores |
| PDF, Office y vídeo solo en OneDrive | sin datos de contenido, 0 KB leídos y atributos intactos |

**Apertura desde Windows** (modo preferido activo, en este equipo):

| Mecanismo | Resultado |
|---|---|
| Doble clic en una carpeta del escritorio | se abre en LFind |
| Doble clic en un acceso directo a carpeta del escritorio | se abre en LFind |
| `start` y `ShellExecute`, también de un `.lnk` | se abre en LFind |
| Acción predeterminada de una carpeta desde una ventana del Explorador | se abre en LFind |
| Sin el modo preferido, el mismo doble clic | se abre en el Explorador |
| `explorer.exe <carpeta>` | sigue siendo el Explorador |
| `Win+E`, diálogos de abrir y guardar | dependen de Windows; no se han probado |

Los dobles clics se enviaron como mensajes a la ventana del escritorio, no con un ratón físico. El doble clic del usuario dentro de una ventana del Explorador no se ha probado.

**Terminal:** en PowerShell y CMD, con la sincronización activa, un `cd` mueve el explorador y navegar mueve la terminal. Desactivada, ninguno afecta al otro.

### Corrección de interfaz, archivos e integración con Windows

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| `npm test` | 411/411 |
| Rust con SQLite, `--locked` | 309 + 3 correctas |
| Rust sin SQLite, `--locked` | 252 + 3 correctas |

Probado en la app real, con una instalación aislada (preferencias e índice reales intactos):

| Prueba | Resultado |
|---|---|
| «Abrir con» un zip con espacios en la ruta | WinRAR, lanzado directamente por LFind, sin CMD ni PowerShell |
| Lista de «Abrir con» | correcta para zip e imagen; «Elegir otra» abre el cuadro nativo |
| Papelera, 392 elementos | carga en 1,85 s; búsqueda, restaurar, cancelar y confirmar eliminar, sin tocar el resto |
| Analizador en OneDrive real | 197 solo en la nube y 1 sincronizado; ningún atributo cambió, sin descargas |
| Desplegables, contraste mínimo | Sistema y Oscuro 11,9; Claro 16,4; Cyberpunk 17; Hacker 16,6; Minimal 18,9 |
| Doble panel | línea corregida; arrastre con el cursor real a un panel con archivos y a uno vacío |
| Barra lateral | un gesto corto no la mueve, un arrastre sí, y se conserva |
| Doble clic en una carpeta del escritorio (por mensajes) | abierta en 362 ms, sin CMD, una sola instancia |
| Cuadro de abrir de .NET con la integración activa | funciona y no abre LFind |
| Restaurar la integración | Registro idéntico al de antes |
| Reposo | 18 MB nativos y 0,07 % de CPU |

**Sin probar:** arrastre físico al Explorador, al escritorio y a otras aplicaciones, y los cuadros de abrir y guardar de un navegador. Otras aplicaciones ocupaban toda la pantalla y la prueba se negó a soltar sobre ellas. WinRAR es el único compresor probado al abrir: 7-Zip no está instalado y NanaZip solo se comprobó en la lista. «Vaciar la Papelera» solo se comprobó hasta su confirmación.

**Causa de la consola y la página inaccesible:** el Registro apuntaba a una compilación de desarrollo, activada desde `tauri dev`. Esa compilación abre consola y depende del servidor de Vite. LFind ya no escribe rutas de una compilación de desarrollo. En un equipo afectado, la integración se corrige con «Reparar» o volviéndola a activar desde la versión instalada.

## Verificación anterior (Update 12)

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

- **Ratón físico:** el arrastre entre paneles se ha probado con el cursor real del sistema, movido por programa. El divisor y las filas, con entrada del navegador y mensajes de Windows. Nada de esto lo ha hecho una persona con un ratón.
- **Arrastre a otras aplicaciones y cuadros de abrir y guardar de un navegador:** sin probar (ver la corrección de interfaz, arriba).
- **Actualización por fechas:** Windows no cambia la fecha de una carpeta cuando un archivo suyo solo cambia de tamaño. Por eso esa vía se marca como «resultados parciales»; la vigilancia automática sí lo ve, pero solo mientras LFind está abierto y una vez que el archivo se vuelca o se cierra. No se usa el diario USN porque exige permisos de administrador.
- **Vigilancia:** una ubicación a la vez, la del informe abierto. En una unidad completa actualiza como mucho cada 30 minutos y reconcilia cada 6 horas (carpetas: 5 minutos y 1 hora).
- **Actualización sin barra de progreso:** en una unidad grande tarda alrededor de un minuto, con solo el botón «Actualizando…».
- **Doble panel:** las pestañas visibles arriba son las del panel activo. El orden propio de cada panel no sobrevive a un reinicio.
- **Archivos en la nube:** solo tienen miniatura si Windows ya la guardó; en una muestra real de OneDrive, 1 de cada 400. El resto muestra el icono con la nube y nunca se descargan.
- **Personalización:** los fondos se limitan a 1920 px. La legibilidad se protege con una opacidad mínima y avisos de contraste, no con un análisis de cada imagen.
- **Sin comprobar con hardware real:** unidades extraíbles, de red y Google Drive en el analizador; OneDrive de empresa.
- **«Deshacer» de un aviso anterior:** siempre deshace la última operación.
- **Detector de duplicados:** el mayor análisis completado tiene 193 179 archivos. La unidad C: entera (cerca de un millón) solo se midió enumerando. Un análisis solo se reanuda cuando se pide.
- **Terminal:** la sincronización del shell al explorador depende del indicador; un perfil que reemplace `prompt` después de abrirse la anula. Una sesión a la vez.
- **Inspector:** un PDF sin cabecera linealizada ni árbol de páginas visible no muestra el número de páginas.
- **Explorador preferido:** `Win+E`, el Explorador abierto y los diálogos de abrir y guardar siguen siendo de Windows.
- **Notas de versión:** `CHANGELOG.md` no tiene sección de Update 10.

## Riesgos técnicos abiertos

Detalle y ubicación en el código: [estado hasta Update 11, §6](docs/history/PROJECT_STATUS_hasta-update-11.md).

- **R-06 y R-07:** el índice reescribe todas las filas en cada pasada y guarda la ruta tres veces. Corregirlo obliga a reconstruirlo.
- **R-02:** un pánico fuera de las zonas aisladas cierra la aplicación a propósito.
- **R-03:** un fallo al leer la raíz de una unidad se cuenta como omitido y el barrido puede borrar sus filas.
