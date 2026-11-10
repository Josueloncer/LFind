# Portátiles: barra superior y zoom con trackpad

Sin commit, push ni publicación. Sin librerías nuevas.

## Causas

**Barra superior.** `--titlebar-h` era un valor fijo de 46 px y los botones de ventana se estiraban a esa altura. Windows 11 usa 32 DIP para su barra de título. La medida está en píxeles CSS, que escalan con el DPI igual que la barra nativa: a 150 % ocupaba 69 px físicos. No había nada que la hiciera crecer más; el problema era el valor de partida.

**Zoom con trackpad.** Chromium entrega el pinzado como una ráfaga de eventos Ctrl+rueda sintéticos, con deltas pequeños y a la frecuencia de la pantalla. El código estaba pensado para la rueda del ratón:

1. **Sensibilidad ligada a los fotogramas.** Cada fotograma redondeaba a píxeles enteros y forzaba al menos 1 px de cambio. Un pinzado lento avanzaba 1 px por fotograma, fuera cual fuera el gesto, y lo fraccionario se perdía.
2. **Una animación por fotograma.** Cada cambio relanzaba una animación de 170 ms en todos los iconos visibles.
3. **El ancla medía el DOM animado.** La posición que conserva la vista se leía con `getBoundingClientRect`, que incluye la animación en curso.
4. **La inercia.** Al soltar Ctrl, los eventos de inercia del trackpad llegaban como desplazamiento normal.
5. **Varios saltos de vista en un gesto.** Un solo pinzado pasaba de detalles a lista y de lista a iconos.

## Corrección

| Archivo | Cambio |
|---|---|
| `src/styles/tokens.css` | `--titlebar-h` de 46 a 36 px |
| `src/styles/app.css` | Relleno vertical de las pestañas de 7 a 4 px: siguen midiendo 28 px |
| `src/lib/view.ts` | `wheelUnits`, `isWheelNotch`, `nextExactSize`, `PINCH_GAIN`, `MAX_FRAME_RATIO` y `zoomTo`; retirado `zoomBy`, que ya no se usaba |
| `src/components/FileView.tsx` | El manejador de la rueda sigue la tecla Ctrl real, acumula el tamaño con decimales, descarta la inercia, cambia de vista una vez por gesto y decide cuándo animar |
| `src/hooks/useZoomAnimation.ts` | Ancla con la distribución anterior; sin animación durante un gesto continuo, y lo que quedara animándose se detiene |
| `src/components/Pane.tsx` | `onZoomTo` en lugar de `onZoomBy` |
| `src/lib/view.test.ts` | Pruebas de proporcionalidad, pinzado, límite por fotograma y clasificación rueda/trackpad |

La rueda del ratón no cambia: un punto vale lo mismo que antes (un 12 %) y un punto suelto se sigue animando.

## Pruebas

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| `npm test` | 453/453 |
| `npm run build` | correcto |

En la app real, con la instalación aislada y una carpeta desechable de 160 archivos. Se usó el mismo guion sobre la compilación anterior y sobre la nueva. La rueda y el pinzado se simulan con eventos de entrada de Chromium (CDP), que es como Chromium entrega un pinzado: Ctrl+rueda sin la tecla.

| Prueba | Antes | Después |
|---|---|---|
| Barra a 1366×768 y 1280×720, al 100 % | 46 px; botones 46×46 | 36 px; botones 46×36, alineados |
| Barra a 1536×864 al 125 % y a 1280×720 y 1024×640 al 150 % (emulado) | 46 px CSS; 58 y 69 px físicos | 36 px CSS; 45 y 54 px físicos; sin desbordes |
| Maximizar y restaurar | funciona | funciona |
| Pinzar al doble (30 eventos) | 126 px; 3 260 animaciones; peor fotograma 50 ms | **192 px**; 0 animaciones; peor fotograma 11 ms |
| Pinzado lento (60 eventos diminutos; siguiendo a los dedos serían 115 px) | 156 px; 4 442 animaciones | **115 px**; 0 animaciones |
| Un punto de la rueda | 108 px, animado | 108 px, animado |
| Rueda rápida, 6 puntos | 193 px; 390 animaciones | 192 px; 90 (solo el primer punto) |
| Inercia tras soltar Ctrl | la lista se desplaza 90 px | 0 px; la rueda normal sigue funcionando |
| Un pinzado desde detalles | salta a iconos | pasa a lista; el siguiente gesto, a iconos |
| Reducir movimiento | (prueba mal planteada en esa pasada) | 0 animaciones; el tamaño cambia igual |

Las preferencias y el índice reales quedaron intactos, y la instalación de pruebas se desinstaló.

## Limitaciones

- **Sin trackpad físico.** Los gestos se simularon con eventos de Chromium; ninguna persona ha usado un trackpad real con esta versión.
- **Sin escalado real de 125 % y 150 %.** Este equipo está al 100 %, así que el escalado se emuló dentro de la página y no se cambió la configuración de pantalla.
- **Pinzar con escalado emulado al 150 %.** Pinzar al doble dio 152 px en vez de 192, porque la emulación divide el desplazamiento por la escala. Con un escalado real no se sabe si Chromium hace lo mismo. Si lo hace, el pinzado sería un tercio más suave, pero igual de progresivo y sin saltos.
- **Clasificación rueda/trackpad.** Se hace por la forma del evento: una rueda da saltos enteros de 50 o más, o va por líneas. Una rueda de alta resolución con pasos pequeños se trataría como un trackpad: zoom directo, sin la animación del punto suelto.
- **El primer elemento visible puede cambiar.** Se conserva la posición de la fila que estaba arriba, pero al cambiar cuántos iconos caben por fila, otros elementos pasan a ocupar esa fila.
