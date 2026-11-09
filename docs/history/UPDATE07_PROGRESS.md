# UPDATE07_PROGRESS · Visor inteligente de archivos

Base: Update 06.0. Sin commits. Dependencia nueva: `pdfjs-dist` 4.10.38 (npm). Ninguna crate nueva: `Cargo.lock` intacto.

## Arquitectura en breve

| Pieza | Archivo | Función |
|---|---|---|
| Decisión y conversión | `crates/nova-core/src/viewer/mod.rs` | Inspecciona el archivo y lo prepara; cancelación, una conversión a la vez, caché |
| Permisos y entrega | `viewer/serve.rs` | Token por archivo, rangos, cabeceras de seguridad |
| PSD/PSB | `viewer/psd.rs` | Composición real reducida mientras se lee; no carga las capas |
| EPS y RAW | `viewer/embedded.rs` | Solo imágenes ya incluidas en el archivo |
| Comandos y protocolo | `src-tauri/src/viewer_cmd.rs` | `viewer_open`, `viewer_close`, `lfview` |
| Controlador | `src/stores/viewer.ts`, `hooks/useViewerSource.ts` | Modo, navegación, carga con cancelación |
| Interfaz | `components/PreviewPane.tsx`, `components/viewer/*` | Panel y vista ampliada (un solo elemento), visualizadores |
| PDF | `lib/pdfEngine.ts`, `viewer/PdfViewer.tsx` | PDF.js por rangos, solo páginas visibles |
| Cálculo puro | `lib/viewerMath.ts` | Zoom, navegación, disposición |

## Seguridad

- **Sin `assetProtocol` ni scopes globales.** Un explorador necesitaría acceso a todo el disco; con `lfview` la webview solo lee lo que `viewer_open` autorizó para ese archivo.
- **Contenido no confiable:** tipo fijado por el backend tras inspeccionar el archivo, `nosniff`, CSP `sandbox` en cada respuesta. Si el archivo cambia, el permiso deja de servir (410).
- **CSP:** solo se añade `http://lfview.localhost` a `img-src`, `media-src` y `connect-src`; se cierran `object-src`, `frame-src`, `base-uri` y `form-action`.
- **SVG:** solo en `<img>`. **PDF:** `isEvalSupported: false`, sin XFA, sin capa de anotaciones. **PostScript:** nunca se ejecuta.
- **Sin bibliotecas nativas nuevas.** PDF.js corre en un Worker de la webview; PSD, EPS y RAW son Rust seguro dentro de `diagnostics::isolate`. Por eso no hizo falta un proceso aparte.
- **Excepción heredada:** HEIC y otros usan el proveedor de miniaturas de Windows ya existente (código nativo de terceros, en un hilo con tiempo límite).

## Compatibilidad comprobada con visualización real

Abiertos en la **app empaquetada** con archivos reales del equipo (salvo los marcados).

| Formato | Estado | Evidencia y límites |
|---|---|---|
| JPG, PNG con transparencia, WebP, BMP, ICO, AVIF | ✅ | Original decodificado por WebView2 |
| GIF animado | ✅ | Dos capturas sucesivas difieren |
| SVG | ✅ | Con `<script>`, `onload` y recurso externo: no ejecutó ni cargó nada |
| PDF | ✅ | Documentos de 1, 125 y 146 páginas, hasta 110 MB |
| AI con contenido PDF | ✅ | 229 de 235 AI legibles de una muestra aleatoria del equipo llevan contenido PDF |
| PSD / PSB | ✅ | Composición real hasta 1,7 GB; 8 bits RGB comprobado con archivos reales |
| TIFF | ✅ | Hasta 200 MB / 100 MP; un TIFF de 512 MB muestra «demasiado grande» |
| MP4, WebM | ✅ | Duración, resolución y reproductor |
| MP3, FLAC, WAV | ✅ | Duración y reproductor |

**Parcial**

| Formato | Situación |
|---|---|
| EPS | Solo si incluye previsualización (TIFF de paleta, EPSI o XMP): 4 de 6 muestras, normalmente a baja resolución. Sin ella, aviso claro |
| AI sin contenido PDF | Se detecta y se avisa; las basadas en EPS siguen la ruta EPS |
| RAW (ARW, NEF, DNG) | Vista JPEG incrustada, no revelado. 3 DNG sin JPEG no se muestran. CR2, ORF, PEF, SRW, SR2: misma ruta, sin muestra para probar |
| HEIC / HEIF | Depende del proveedor de Windows: la muestra salió a 160×120. Las demás muestras eran archivos en la nube no disponibles |
| MOV | Depende del códec: una muestra no reproducible se informa como tal |
| MKV y otros | Solo miniatura de Windows si existe |
| PSD 16/32 bits, CMYK, Lab, indexado | Con pruebas sintéticas, no con archivos reales identificados. Sin gestión de color ICC |
| PS | Sin vista incluida: aviso. Nunca se ejecuta |

## Rendimiento (medido)

| Medida | Resultado |
|---|---|
| PSD 1,7 GB, primera lectura en frío | 95 s (disco y sistema); segunda apertura sin caché propia: 126 ms |
| PSD 79 MB | 2,1 s la primera vez; desde caché, 220 ms en la app (incluye la espera de 90 ms) |
| Cambio rápido (12 pasos seguidos) | Termina en el archivo correcto |
| Workers tras cerrar el visor | 0 |
| Montón JS tras recorrer 37 archivos | 7 MB |
| JS inicial | 364 KB (antes 345 KB); PDF.js solo se descarga al abrir un PDF |
| Ejecutable / instalador | 15,5 MB / 5,6 MB (antes 13,2 / 3,6) |

**Limitación medida:** un PDF de 1 GB y **una sola página** abre el documento, pero la página no se había pintado a los 150 s y la webview llegó a 713 MB. Muestra indicador de carga y se cancela al cambiar de archivo. Las imágenes incrustadas de más de 100 MP se omiten para proteger la memoria.

## Errores encontrados y corregidos durante la validación

| Error | Detectado por |
|---|---|
| Desbordamiento de enteros en PackBits | Prueba de archivos corruptos |
| Vista ampliada desplazada fuera de la ventana | Captura en la app real |
| El teclado no volvía a la lista al cerrar la vista ampliada | Batería CDP |
| PDF ampliado al 243 % en la vista ampliada | Captura |
| Workers de PDF que seguían vivos mientras terminaban una página | Batería CDP |
| Los EPS con TIFF de paleta no se mostraban | Análisis de los 49 EPS reales |

## Pruebas

| Comprobación | Antes (06.0) | Ahora |
|---|---|---|
| Rust (con y sin SQLite) | 137/137 | ✅ 163/163 |
| Vitest | 205/205 | ✅ 217/217 |
| TypeScript | sin errores | ✅ sin errores |
| `npm run tauri build` | correcto | ✅ correcto, sin avisos |
| Paridad de comandos | 49 = 49 | ✅ 51 = 51 |
| Batería en la app empaquetada (CDP, 37 archivos) | — | ✅ 30 muestran contenido; 7 con el aviso correcto |

Teclado y flujos ejecutados en la app real: Espacio, ←, →, Esc, selección conservada, búsqueda global, zoom, ajuste, tamaño real, rotación, cierre y reapertura.

**Pendiente de validación manual:** ver `WINDOWS_VERIFICATION.md`. No ejecutado: Intro y doble clic (abren programas externos), arrastre de la anchura, PDF con contraseña o con imágenes JPEG2000, los otros temas dentro del visor.
