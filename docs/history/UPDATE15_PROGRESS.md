# Update 15: navegación, vistas, zoom y Papelera

Sin commit, push ni publicación. La versión no se ha cambiado.

## Cambios

**Actualizar al volver a pulsar.** Pulsar la ubicación ya abierta la actualiza: barra lateral, última miga de la ruta, y pestaña activa. Un doble clic solo lee una vez. Si nada cambió, la lista no se toca. F5 sigue igual, y también funciona en la Papelera.

**Vistas compactas.** Detalles: filas de 32 a 26 px. Lista: de 30 a 26 px de separación. Iconos: menos relleno y celdas más estrechas, repartiendo el ancho sobrante. Los nombres largos ahora ocupan dos líneas en vez de cortarse en una.

**Zoom continuo.** De 33 % a 267 % (32–256 px; el 100 % son 96 px), con el porcentaje visible junto al control. Ctrl+rueda y el control ajustan píxel a píxel. El tamaño se conserva al reiniciar. Detalles y lista siguen en los extremos del control.

**Animación.** Los iconos se desplazan y escalan hasta su sitio en 170 ms. Lo que se estaba mirando sigue a la vista. Respeta «reducir movimiento» de Windows.

**Papelera.** Progreso real, archivo a archivo dentro de cada carpeta, con procesados y pendientes. Cancelar ya no deja carpetas a medias ni trabajo oculto.

**Desenfoque conjunto.** Se retiraron los controles, capas, copias de imagen y filtros por zona. Se conservan el fondo y las demás preferencias.

## Causas

**Papelera con elementos pesados.** Reproducido con datos desechables: archivos de 3 GB y carpetas anidadas de 15 000 archivos.

| Caso | Resultado |
|---|---|
| 27 elementos pesados en un lote | terminó bien en 28,3 s |
| Una carpeta de 15 000 archivos | 11,5–12,4 s |
| Archivos de 3 GB | menos de 0,8 s cada uno |
| Lote con un archivo bloqueado | el resto se borró |

No había bloqueos ni esperas infinitas, ni el hilo principal se detenía. **El defecto era de información:** el progreso solo avanzaba al acabar un lote de 25, así que un lote pesado se quedaba en «0 de 27» durante casi medio minuto.

También había un defecto de la corrección anterior: al cancelar, el lote en curso seguía borrando en segundo plano después de decir «cancelado».

**Corrección.** Se usa `IFileOperation` con su receptor de progreso, la API oficial, que notifica cada archivo. Con ella, una carpeta de 15 000 archivos tardó 8 s. Cada carpeta va en su propia operación y se termina aunque se cancele. Si Windows pasa 120 s sin avanzar, el lote se da por perdido y se informa.

**Pulsar la ubicación abierta no hacía nada** porque `navigate` ignora la misma ruta, lo cual es correcto para las rutas programáticas. Se añadió una entrada propia para los clics del usuario.

**Nombres cortados en iconos:** la regla general de nombres impedía el salto de línea dentro de la cuadrícula.

## Archivos

| Zona | Archivos |
|---|---|
| Nuevos | `src/hooks/useZoomAnimation.ts`, `src/lib/sameEntries.ts` y su prueba |
| Rust | `crates/nova-core/src/recycle.rs`, `src-tauri/src/shellapps_cmd.rs` |
| Vistas y zoom | `src/lib/view.ts`, `src/lib/viewGeometry.ts`, `src/lib/marquee.ts`, `src/components/FileView.tsx`, `src/components/StatusBar.tsx`, `src/components/Pane.tsx`, `src/styles/app.css` |
| Navegación | `src/stores/nav.ts`, `src/hooks/useDirectory.ts`, `src/components/Sidebar.tsx`, `src/components/Toolbar.tsx`, `src/components/TitleBar.tsx` |
| Papelera | `src/components/RecycleView.tsx`, `src/lib/recycle.ts`, `src/lib/fs/{types,tauri}.ts` |
| Desenfoque conjunto | `src/lib/appearance.ts`, `src/App.tsx`, `src/components/AppearancePanel.tsx`, `src/stores/backgrounds.ts` |
| Pruebas | `src/lib/view.test.ts`, `src/lib/marquee.test.ts`, `src/lib/recycle.test.ts`, `src/lib/zones.test.ts`, `src/stores/nav.test.ts`, `src/stores/backgrounds.test.ts` |
| Documentación | `PROJECT_CONTEXT.md`, este documento |

## Pruebas

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| `npm test` | 450/450 |
| Rust con SQLite, `--locked` | 327 + 3 correctas |
| Rust con SQLite y carpeta temporal en formato corto, como en GitHub | 327 + 3 correctas |
| Rust sin SQLite, `--locked` | 270 + 3 correctas |
| `npm run build` y `tauri build` de producción | correctos |

En la app real, con una instalación aislada (preferencias e índice reales intactos) y datos desechables:

| Prueba | Resultado |
|---|---|
| Pulsar miga, barra lateral y pestaña activa | cada una muestra el archivo nuevo |
| Elegir un archivo | no recarga |
| Doble clic en la miga | ninguna fila retirada, misma lista en pantalla |
| Selección y desplazamiento al recargar | se conservan |
| F5 | funciona |
| Doble panel | solo se actualiza el panel activo; zoom sin desbordes |
| Detalles, en los 6 temas | 26 px y texto legible |
| Iconos, 7 combinaciones de tamaño y ancho (48–256 px, 640–1900 px) | 0 cortes, 0 solapes, sin desbordes |
| Zoom: 6 puntos de rueda | de 100 % a 201 % |
| Rueda fina | 136, 138, 140 px |
| Zoom tras reiniciar | conservado |
| Animación | solo `transform`, nada de filtros; al acabar no queda nada aplicado |
| Reducir movimiento | sin animación |
| Preferencias por zonas, migradas | desenfoque 22, opacidad 88; sin restos; fondo y ancho de barra intactos |
| **17 elementos pesados** (6 GB, 2 carpetas de 12 000 archivos) | 6,5 s, progreso continuo, interfaz respondiendo en 12 ms como máximo |
| Cancelar con una carpeta grande en curso | la termina, avisa, y no se borra nada más después |
| Lote con un archivo bloqueado | los demás borrados; nombra el bloqueado y el motivo |
| Elementos ajenos de la Papelera, uno a uno | 0 desaparecidos |
| «Vaciar», cancelando | no borra nada; usa el mismo camino que una selección |

### Memoria y fluidez

Misma ventana, 1500×900, con fondo, la misma carpeta de 2 540 elementos y pantalla de 180 Hz. El «después» se midió antes del último ajuste de espacios.

| Medida | Antes | Después |
|---|---|---|
| WebView2 en reposo, con fondo | 488–504 MB | 473–483 MB |
| WebView2 tras hacer zoom | 561 MB | 527 MB |
| Filas visibles en detalles | 23 | 28 |
| Iconos por fila / visibles, a 96 px | 4 / 88 | 5 / 109 |
| Desplazamiento | 179 fps, peor fotograma 17 ms | 179 fps, peor fotograma 17 ms |
| Ctrl+rueda continuo | 177 fps, peor fotograma 28 ms (sin animación) | 160 fps, peor fotograma 50 ms (con animación) |

## Limitaciones

- **Sin desenfoque en la animación.** Pedía usarlo solo si mejoraba el resultado. Un filtro exige repintar cada fotograma una capa del tamaño de la lista, y el movimiento ya es suave con `transform`. Es la opción más segura frente a parpadeos y borrosidad al terminar.
- **El zoom cuesta algo de fluidez:** el peor fotograma pasa de 28 a 50 ms al girar la rueda sin parar. Con más de 350 elementos a la vista no se anima.
- **Cancelar una carpeta muy grande** no es inmediato: se termina primero, para no dejarla a medias, y la pantalla lo indica.
- **El ajuste continuo es en píxeles enteros.** El paso de 33 px a 32 px cambia al modo en línea, como hacía el zoom anterior.
- **Sin probar:** confirmar «Vaciar» sobre la Papelera real; elementos de OneDrive solo en la nube dentro de la Papelera; un ratón físico en manos de una persona. La rueda se simuló con entrada del navegador, sobre la ventana real.
- La comparación de «iconos visibles» depende del tamaño de ventana y de la carpeta.

## Cómo ejecutar y verificar

```powershell
npm run typecheck
npm test
cd crates/nova-core; cargo test --locked --features sqlite; cargo test --locked; cd ../..
npm run build
npx tauri build --config '{\"bundle\":{\"createUpdaterArtifacts\":false}}'
```

1. Crea un archivo en una carpeta abierta desde fuera de LFind y pulsa esa carpeta en la barra lateral: debe aparecer.
2. En iconos, gira Ctrl+rueda poco a poco y comprueba el porcentaje junto al control. Reinicia: debe conservarse.
3. Envía a la Papelera una carpeta grande de prueba y elimínala: la pantalla debe ir contando sus archivos.
