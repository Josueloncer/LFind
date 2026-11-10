# LFind: auditoría final

9 de noviembre de 2026. Sin commit, push ni publicación. La versión, las claves de firma y la configuración del actualizador no se han tocado.

## 1. Estado general

LFind está **listo para distribuirse**. No hay hallazgos críticos. Los cuatro problemas reales encontrados están corregidos y comprobados en la app real. Lo que queda son detalles menores documentados y pruebas que exigen hardware o datos que no se podían usar aquí.

**Recomendación:** cerrar el desarrollo de funcionalidades y pasar a la publicación.

## 2. Hallazgos

| Gravedad | Hallazgo | Estado |
|---|---|---|
| CRÍTICO | Ninguno | — |
| IMPORTANTE | Enviar a la Papelera un lote con un elemento en uso se informaba como **fallido entero**, aunque el resto ya estaba en la Papelera | Corregido |
| IMPORTANTE | «Crear acceso directo» abría un instante una **ventana de consola** | Corregido |
| IMPORTANTE | Restaurar fallaba si la carpeta original ya no existía, y los errores llegaban en inglés técnico («0x80270027», volcados de `RestoreTwins`) | Corregido |
| MENOR | Al desactivar la integración con Windows quedaba la clave vacía `HKCU\Software\LFind\Integration` | Corregido |
| MENOR | El editor del instalador figura como «nova» | Documentado: cambiarlo rompería las actualizaciones (§8) |
| MENOR | Instalador sin firma Authenticode: SmartScreen avisa en la primera instalación | Documentado (requiere certificado) |
| MENOR | El desinstalador de Tauri deja `HKCU\Software\nova\LFind` con la ruta de instalación, salvo si se marca «eliminar datos» | Documentado (comportamiento de Tauri) |
| MENOR | Mover entre unidades y cancelar deja en el destino lo ya copiado; el original no se toca | Documentado (por lectura de código, sin probar) |
| MENOR | `requireSignedVersion` en la configuración del actualizador no es una opción de Tauri y no tiene efecto. La firma se exige siempre | Sin tocar (restricción sobre el actualizador) |
| MENOR | Varias pruebas de Rust (analizador, duplicados, fondos) no borran sus carpetas temporales: en este equipo quedan unas 2 000 `lfind-*` en `%TEMP%`. Las pruebas del Registro dejan vacía `HKCU\Software\LFindTests`. Solo afecta a quien ejecuta las pruebas | Documentado; limpieza en §11 |
| MENOR | En este equipo hay una clave antigua `HKCU\Software\nova\NOVA Explorer` que apunta a `C:\NOVA Explorer`, que no existe | Sin tocar (no es de las pruebas) |

### Causas

**Papelera por lotes.** La biblioteca `trash` manda el lote entero a Windows. Si un elemento está en uso, Windows envía todos los demás y después informa del fallo. LFind reintentaba entonces uno a uno, y los que ya estaban en la Papelera fallaban con «no se encuentra el archivo». Reproducido con la API real: 6 archivos con 1 bloqueado daban «0 completados, 6 errores» con 5 de ellos ya en la Papelera.

**Consola.** `powershell.exe` se lanzaba sin `CREATE_NO_WINDOW`. Desde una app de ventanas, Windows crea una consola antes de que `-WindowStyle Hidden` pueda ocultarla. Era el único proceso de consola sin esa opción.

**Restaurar.** La biblioteca no vuelve a crear la carpeta de origen, y sus errores no se traducían. En ningún caso daba un falso éxito: todos los fallos devolvían error.

## 3. Correcciones

| Archivo | Cambio |
|---|---|
| `crates/nova-core/src/ops.rs` | Tras un lote fallido, lo que ya no está en su sitio cuenta como enviado. Solo se reintenta lo que sigue ahí. Con prueba de regresión |
| `crates/nova-core/src/recycle.rs` | `trash_blocked_reason`: el motivo en español, y para una carpeta, qué archivo la bloquea |
| `src-tauri/src/commands.rs` | El envío individual usa ese motivo |
| `src-tauri/src/shellapps_cmd.rs` | Restaurar vuelve a crear la carpeta original (solo si nada choca), traduce los errores y **comprueba en disco** que cada elemento ha vuelto |
| `crates/nova-core/src/shell.rs` | `CREATE_NO_WINDOW` al crear accesos directos |
| `crates/nova-core/src/winshell.rs` | Al desactivar el explorador preferido se borra la copia de seguridad, y `Software\LFind` solo si queda sin valores ni subclaves. Con prueba de regresión |

## 4. Archivos modificados

Código: los seis de la tabla anterior. Documentación: `CHANGELOG.md` (Updates 14 y 15, y estas correcciones), `PROJECT_CONTEXT.md`, `PROJECT_STATUS.md` y este informe.

## 5. Optimizaciones

No se ha optimizado nada. Las mediciones (§7) muestran consumo en reposo prácticamente nulo y sin crecimiento sostenido de memoria ni de handles. Sin un problema medido, tocar renders, cachés o SQLite solo añadía riesgo. Revisado y correcto:

- Los temporizadores y listeners se retiran, o se instalan una sola vez.
- El sondeo de unidades se detiene con la ventana oculta.
- Las operaciones de disco corren fuera del hilo principal.
- La terminal integrada mete sus procesos en un Job Object que los cierra con LFind.

## 6. Pruebas

### Automatizadas

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| `npm test` (Vitest) | 450/450 |
| Rust con SQLite, `--locked` | 329 + 3 correctas |
| Rust con SQLite y carpeta temporal en formato corto, como en GitHub | 329 + 3 correctas |
| Rust sin SQLite, `--locked` | 272 + 3 correctas |
| `npm run build` | correcto |
| `tauri build` de producción (instalador NSIS) | correcto, `LFind_0.1.0_x64-setup.exe`, 7,4 MB |

### En la aplicación real

Instalación aislada (`LFindNavTest`, con sus propias preferencias e índice), manejada con eventos de entrada del navegador sobre la ventana real. Se usaron datos desechables en una carpeta de prueba, y la Papelera se limpió después elemento a elemento, solo con lo que venía de esa carpeta.

| Prueba | Resultado |
|---|---|
| Abrir LFind otra vez con LFind abierto | 1 proceso; la ventana sigue viva |
| Crear acceso directo, con un detector de consolas validado antes con un control | `.lnk` creado; **0 consolas** |
| Enviar 6 archivos a la Papelera, 1 en uso | «Enviado a la Papelera 5 elementos; 1 con error. Está en uso por otra aplicación…»; 5 en la Papelera, el bloqueado a la vista |
| Restaurar con la carpeta original borrada | carpeta recreada; archivo restaurado con su contenido |
| Restaurar un elemento bloqueado dentro de la Papelera | «Un elemento está en uso…»; no se da por restaurado. Al soltarlo, el segundo intento funciona |
| Copiar 4 GB al otro panel y cancelar | «Operación cancelada»; destino vacío, sin temporales; origen intacto |
| Activar y desactivar las cuatro opciones de integración | todas activas; al desactivar, el Registro queda **idéntico** al de antes, también `Software\LFind` |
| Metadatos del instalador | nombre, versión e icono correctos; editor «nova» |
| Desinstalar | programa, ejecutable, clave de desinstalación y acceso del menú Inicio retirados; datos de usuario conservados; 0 procesos |
| Preferencias e índice reales | intactos, comprobados por huella y fecha |

### Con la API real de Windows, fuera de la app

Un programa de prueba llama al mismo código que la app (`ops::trash_items` y `trash`):

| Prueba | Resultado |
|---|---|
| 6 archivos con 1 bloqueado, antes de corregir | 0 completados, 6 errores; 5 ya en la Papelera |
| Lo mismo, después | 5 completados; 1 error con su motivo |
| Carpeta con un archivo bloqueado dentro | no se mueve; «Contiene un archivo en uso: dentro.txt…»; contenido intacto |
| Restaurar: bloqueado, sin carpeta, duplicados y normal | todos los fallos devuelven error; ninguno da éxito falso |

Sigue vigente lo comprobado en la Update 15 ([UPDATE15_PROGRESS.md](docs/history/UPDATE15_PROGRESS.md)): navegación y actualización, selección, doble panel, vistas en los seis temas, zoom y persistencia, eliminación de la Papelera con 6 GB, cancelación y archivos bloqueados.

### Simuladas

- La rueda y los clics son eventos de entrada del navegador sobre la ventana real, no un ratón físico.
- La lógica de lotes de la Papelera también se prueba con un envío inyectado (prueba de Rust).

## 7. Consumo medido

Una sola ventana de 1500×900 con doble panel, 4 pestañas y una carpeta de 3 000 elementos. Son 7 procesos: LFind y WebView2.

| Medida | Resultado |
|---|---|
| CPU en reposo durante 30 s | 0,08 s de CPU: **0,02 %** del procesador |
| Memoria privada / conjunto de trabajo | 277 MB / 481 MB |
| Handles / hilos | 3 686 / 243 |
| Tras 30 navegaciones seguidas | +17 MB privada, +11 handles, −4 hilos |

Los 17 MB de más son las cachés de iconos y listados de las carpetas visitadas, no un crecimiento continuo. En la Update 15, el conjunto de trabajo en reposo, con fondo y una sola pestaña, fue de 473 a 483 MB.

## 8. Instalador y actualizador

- **Instalador NSIS:** se compila, instala, abre y desinstala sin dejar el programa ni accesos. Nombre, icono e identidad LFind correctos. La versión local es 0.1.0, y la real la pone la publicación a partir del número de ejecución.
- **Firma:** las actualizaciones se firman con la clave del actualizador (minisign) y se verifican siempre. El instalador no tiene firma Authenticode.
- **Editor «nova»:** sale del identificador `app.nova.explorer`. El instalador lo usa para recordar la carpeta de instalación (`HKCU\Software\nova\LFind`). Cambiarlo haría que una actualización no la encontrara, así que solo debe hacerse con un plan de migración.
- **Notas de versión:** salen de la primera sección del `CHANGELOG.md`. Las Updates 14 y 15 se publicaron con las notas de la 13. La próxima publicación mostrará las de la 15 y estas correcciones.
- **Migración de preferencias:** las preferencias por zonas de la Update 14 se convierten sin pérdida (comprobado en la Update 15).

## 9. Limitaciones conocidas

- Mover entre unidades y cancelar deja una copia parcial en el destino. El original no se toca.
- Los diálogos de abrir y guardar, `Win+E` y el Explorador abierto siguen siendo de Windows. No hay forma soportada de cambiarlo.
- El resto, sin cambios: ver `PROJECT_STATUS.md`, «Limitaciones y pendientes».

## 10. Pruebas pendientes

| Prueba | Por qué no se hizo |
|---|---|
| **Enviar a la Papelera en una unidad sin Papelera** (red, algunas USB) | No hay una unidad así para pruebas, y las demás unidades de este equipo pueden tener datos personales. Windows decide ahí si avisa o borra; conviene comprobarlo antes de prometer que «siempre es recuperable». |
| «Vaciar» sobre la Papelera real | Contiene archivos personales. Usa el mismo camino que eliminar una selección, que sí está probado. |
| Archivos de OneDrive solo en la nube: copiar, mover, Papelera | Sin una cuenta de pruebas |
| Instalación limpia en otro equipo y actualización real desde una versión publicada | Requiere otro equipo o publicar |
| Mover entre unidades y cancelar | Las otras unidades pueden tener datos personales |
| Arrastre a otras aplicaciones y ratón físico | Requiere una persona |

## 11. Estado de Git

Rama `main`, igual que `origin/main` en `74a220e` (Update 15). Sin commit:

```
 M CHANGELOG.md
 M PROJECT_CONTEXT.md
 M PROJECT_STATUS.md
 M crates/nova-core/src/ops.rs
 M crates/nova-core/src/recycle.rs
 M crates/nova-core/src/shell.rs
 M crates/nova-core/src/winshell.rs
 M src-tauri/src/commands.rs
 M src-tauri/src/shellapps_cmd.rs
?? FINAL_AUDIT_REPORT.md
```

Sin archivos temporales ni claves en el árbol. Para subirlo, añade los archivos por nombre, no con `git add .`.

Limpieza opcional de los restos de las pruebas de Rust en este equipo (solo las carpetas con nombres de prueba y la clave vacía):

```powershell
Get-ChildItem $env:TEMP -Directory | Where-Object Name -Match '^lfind-(report|dup|bg|walk|watch|cloud)-' | Remove-Item -Recurse -Force
if (-not (Get-ChildItem 'HKCU:\Software\LFindTests') -and (Get-Item 'HKCU:\Software\LFindTests').ValueCount -eq 0) { Remove-Item 'HKCU:\Software\LFindTests' }
```
