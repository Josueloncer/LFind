# Update 12 · Ronda final de correcciones

Estado: terminado en el árbol de trabajo, **sin commit, push ni publicación**. Estado general del proyecto: [PROJECT_STATUS.md](PROJECT_STATUS.md).

## Correcciones

### 1. Vigilancia del analizador

| Problema | Corrección |
|---|---|
| En una ubicación con actividad constante nunca había calma y la actualización podía no llegar | Espera máxima de 5 minutos desde el primer cambio sin atender |
| Con un archivo abierto, la enumeración de Windows da un tamaño antiguo | Los archivos avisados se leen directamente (hasta 8192 por ciclo) |
| Archivos temporales o cambios de fecha obligaban a copiar el informe entero | Antes se compara cada carpeta avisada con el informe; si nada cambió, solo se actualiza la fecha de comprobación |
| Si una actualización fallaba, se perdían los cambios anotados | Se marca para comparar fechas en el siguiente ciclo |
| Si la vigilancia se caía (carpeta borrada, unidad retirada), no se recuperaba | Se reintenta cada 5 minutos y, al volver, se comparan fechas |
| No había reconciliación periódica | Comparación de fechas cada hora en carpetas y cada 6 horas en unidades, sin marcar el informe como parcial si no se perdieron avisos |
| Los datos propios de LFind contaban como cambios | Se ignora toda su carpeta de datos, sin distinguir mayúsculas |

Límites: 4096 carpetas y 8192 archivos anotados, 400 000 entradas por comprobación, hilos en prioridad de segundo plano. Sigue habiendo un mínimo de 5 minutos entre actualizaciones en carpetas y de 30 en unidades. Archivos: `analyzer/watch.rs`, `analyzer/report.rs`, `analyzer/walk.rs`.

### 2. Orden independiente en doble panel

Cuando los dos paneles muestran la misma carpeta, cada pestaña guarda su propio orden (hasta 32 carpetas) y lo conserva al cambiar de panel o intercambiarlos. Con carpetas distintas, ordenar en un panel ya no cambia el orden general del que dependía el otro. Al volver a un panel, la carpeta visible conserva su orden. Archivos: `lib/viewState.ts`, `stores/nav.ts`, `stores/dual.ts`, `stores/prefs.ts`, `components/Pane.tsx`.

### 3. Avisos duplicados de copiar y mover

Investigación: Rust emite un único «terminado» por operación y no se encontró ningún lanzamiento doble. Lo que se veía repetido tenía tres orígenes, ya eliminados:

- El aviso y la tarjeta «Completado» contaban lo mismo: la tarjeta ya no se muestra si todo fue bien.
- Un aviso con «Deshacer» dura 8 s y el de la operación anterior seguía visible: el mismo texto ya no se apila, sustituye al anterior.
- `finish` no estaba protegido frente a un segundo «terminado»: ahora lo ignora.

Archivos: `stores/ops.ts`, `stores/ui.ts`, `components/Transfers.tsx`.

### 4. Memoria y GPU de los fondos

- El desenfoque y el contraste se aplican una vez sobre una copia pequeña. Ya no hay un `filter` a pantalla completa; queda solo como último recurso si el lienzo no sabe filtrar.
- Una imagen de 1920×1280 con desenfoque 14 en una ventana de 1240 px queda en 320×213 y 9 KB.
- Las posiciones seguidas de un control se resuelven en una o dos preparaciones, y gana la última.
- No se carga dos veces la misma imagen, y lo que deja de usarse se libera.
- Las variables CSS solo se escriben si cambian y las medidas del tema se toman una vez por tema.

Archivos: `lib/backgroundImage.ts`, `stores/backgrounds.ts`, `lib/appearance.ts`, `App.tsx`, `components/AppearancePanel.tsx`, `styles/app.css`.

## Resultados

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| Vitest | 338/338 (22 nuevas) |
| Rust con SQLite, `--locked` | 238 + 3 correctas (12 nuevas) |
| Rust sin SQLite, `--locked` | 208 + 3 correctas |
| `npm run build` | correcto |
| `tauri build` del producto, sin artefactos de actualización | instalador generado |

La prueba `a_noticed_file_reports_its_real_size_while_still_open` falla si se retira la lectura directa: el problema es real, no teórico.

### En la app real (instalación aislada; preferencias e índice reales intactos)

| Caso | Resultado |
|---|---|
| Crear y borrar un temporal en una carpeta vigilada | comprobado a los 64 s; 0 carpetas releídas, sin copiar el informe, no parcial |
| Archivo abierto, escrito y volcado a disco | detectado a los 64 s, tamaño exacto, 1 carpeta releída, no parcial |
| Archivo escrito y cerrado | detectado a los 66 s, tamaño exacto, 1 carpeta releída |
| Archivo abierto, escrito y **sin** volcar | **no detectado en 110 s** (ver limitaciones) |
| Misma carpeta en ambos paneles, órdenes distintos | independientes; se mantienen al cambiar de panel e intercambiar |
| Carpetas distintas | ordenar a la derecha no altera la izquierda ni el orden general |
| Volver a un panel | conserva el orden visible |
| Copiar, mover, mover otra vez y arrastrar con archivos reales | un aviso por texto, 0 tarjetas, archivos correctos, avisos retirados a los 9 s |
| Control de desenfoque, 15 posiciones | 171 ms, 0 cargas de imagen, se guarda el último valor |
| Seis temas | medidas recalculadas y fondo presente |
| Aspecto del fondo preparado frente al filtro en vivo | equivalente en la captura |

Recursos con desenfoque 14, ventana de 1240 px, en MB (dos muestras por modo, media):

| | Sin fondo | Imagen preparada | Filtro en vivo (antes) |
|---|---|---|---|
| Memoria del proceso de interfaz | 65 | 92 | 107 |
| Memoria del proceso GPU | 120 | 153 | 172 |
| Memoria gráfica | 60 | 79 | 104 |
| Uso de GPU al desplazar | 7 % | 7,9 % | 8,7 % |

Es decir, el coste de la memoria gráfica del fondo baja de +44 MB a +19 MB. Son valores orientativos: pocas muestras en un equipo en uso, y se midieron antes del último ajuste, que redujo aún más la imagen (de 458×305 a 320×213).

## Limitaciones pendientes

- **Archivo abierto sin volcar:** Windows no avisa hasta que los datos se vuelcan o el archivo se cierra. Se detecta en ese momento, no antes.
- **Con LFind cerrado:** no hay vigilancia. La comparación de fechas al volver no ve archivos que solo cambiaron de tamaño, y el informe queda marcado como parcial.
- **Una ubicación a la vez**, la del informe abierto.
- **Probado solo con pruebas automáticas:** la espera máxima con actividad constante, el desbordamiento de avisos y la reconciliación periódica. La pérdida de la vigilancia se comprobó con una carpeta real borrada, pero la recuperación completa no se ejercitó en la app.
- **Unidad completa:** esta ronda no repitió la prueba sobre C:.
- **Orden por panel:** se guarda en memoria, como las pestañas; no sobrevive a un reinicio.
- **«Deshacer» de un aviso anterior** siempre deshace la última operación. Ya ocurría antes; no se ha cambiado.
- **Número de operaciones:** la captura de llamadas de la prueba real no registró nada. El «un aviso por operación» se apoya en los avisos, el estado del disco y las pruebas automáticas.
- **Ratón físico:** arrastre y divisor probados con entrada simulada, no con un ratón en manos de una persona.
- **Sin comprobar con hardware real:** unidades extraíbles, de red, Google Drive y OneDrive de empresa.

---

# Update 12 · Pendientes de personalización y nube

Sin commit, push ni publicación. Tampoco cambian firma, secretos, actualizador, `tauri.conf.json`, `package*.json` ni `Cargo.*`.

## Cambios

| Área | Qué se hizo | Archivos |
|---|---|---|
| Tipografías | Selector con las fuentes instaladas (GDI, solo nombres), buscador y filas virtualizadas. El nombre va en la fuente de la interfaz para que se lea siempre, también en fuentes de símbolos; al lado, una muestra en la propia fuente. Se guarda como `custom:Nombre`; las opciones anteriores siguen siendo válidas. | `nova-core/fonts.rs`, `components/FontPicker.tsx`, `lib/appearance.ts` |
| Selector de imágenes | LFind no tenía un selector propio: solo un `<input type="file">`, que abre el diálogo de Windows. Ahora «Elegir imagen…» usa el explorador de LFind: el panel se retira, una barra indica qué hacer y abrir una imagen la elige. Rust la reduce a 1920 px y rechaza las que solo están en la nube. | `stores/backgroundPick.ts`, `stores/customize.ts`, `components/CustomizeBar.tsx`, `hooks/useFileActions.ts`, `nova-core/backgrounds.rs` |
| Colores de carpetas | Nueva opción «Carpetas» en Colores, con icono de muestra. Colorea las dos caras del icono en toda la interfaz; «Usar la del tema» devuelve el color de cada tema. | `lib/appearance.ts`, `components/AppearancePanel.tsx` |
| Esquinas redondeadas | Causa: la esquina del contenido deja ver lo que hay detrás, y con imagen general era la imagen **sin ningún velo**. Cuanto más redondeo, más superficie. Ahora lleva el mismo velo que la barra lateral. Con doble panel, igual en el divisor y en la esquina del segundo panel. | `styles/app.css` |
| Nube | Dos bloqueos impedían cualquier miniatura. Ahora se consulta solo la caché de miniaturas de Windows (`SIIGBF_INCACHEONLY`): no se ejecuta ningún extractor ni se abre el archivo. Si no hay, se mantiene el icono con la nube. Un fallo no se recuerda, y al descargarse el archivo se genera con normalidad. También en el panel de vista previa. | `nova-core/thumbs.rs`, `nova-core/thumbs_win.rs`, `lib/thumbKinds.ts`, `components/Thumb.tsx`, `components/PreviewPane.tsx` |

## Resultados

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| Vitest | 358/358 (20 nuevas) |
| Rust con SQLite, `--locked` | 247 + 3 correctas (9 nuevas) |
| Rust sin SQLite, `--locked` | 217 + 3 correctas |
| `npm run build` | correcto |
| `tauri build` del producto, sin artefactos de actualización | instalador generado |

### En la app real (instalación aislada; preferencias e índice reales intactos)

| Caso | Resultado |
|---|---|
| Fuentes | 876 detectadas, solo 10 filas pintadas; Wingdings con nombre legible y muestra en símbolos; Georgia aplicada y conservada tras recargar |
| Elegir imagen | panel retirado, 0 diálogos de archivos; `.txt` y PNG dañado avisan y dejan elegir otra; imagen de 2400×1600 reducida a 1920×1280 y aplicada |
| Color de carpetas | se aplica al momento, sobrevive a recargar, funciona en seis temas y «Usar la del tema» recupera el color original |
| Esquina con imagen naranja y redondeo 200 % | sin el velo: color puro `235,120,31` en la esquina. Con el velo: `49,40,35`, como el resto del marco |
| Redondeo 0, 100 y 200 en cada zona de fondo, y seis temas | con imagen azul, ningún naranja en las esquinas |

### OneDrive real (lectura)

| Prueba | Resultado |
|---|---|
| Caché de Windows, 400 archivos solo en la nube | 1 miniatura disponible, 399 sin ella, 0 atributos cambiados, unos 28 ms por archivo |
| Extractor normal de Windows, 5 archivos pequeños | error en menos de 1 s, sin miniatura, sin descarga |
| Carpeta de 10 853 imágenes en la nube, en la app | icono con nube, interfaz fluida, 10 853 siguen solo en la nube |

## Limitaciones

- **Miniaturas en la nube:** dependen de lo que Windows ya tenga guardado, y en este equipo es muy poco (1 de 400). Lo habitual seguirá siendo el icono con la nube. En la app real no llegó a verse una miniatura en la nube: ese caso está cubierto por pruebas automáticas y por la sonda, que obtuvo la miniatura de 1 archivo.
- **Selector de imágenes:** no hay vista filtrada por imágenes; se ven todos los archivos. No se puede elegir una imagen solo en la nube sin descargarla antes desde OneDrive.
- **Mica:** sin imagen general, la esquina muestra lo mismo que el marco (el material de Windows). La prueba automática no puede fotografiar ese material, así que un tono cálido del escritorio no se ha medido.
- **Fuente desinstalada:** se conserva la elección y se usa la de respaldo.
- **Fuentes de mapa de bits:** no se ofrecen; la interfaz web no las pinta.
- **Ratón físico:** las selecciones y el doble clic usaron entrada simulada del navegador.
