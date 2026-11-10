# Update 16: accesos directos y rendimiento al entrar en carpetas

Sin commit, push ni publicación. Sin dependencias nuevas. Dos partes:

1. «Abrir ubicación del archivo» en accesos directos.
2. [Rendimiento al entrar en carpetas](#parte-2-rendimiento-al-entrar-en-carpetas).
3. [Terminales CMD y PowerShell](#parte-3-terminales-cmd-y-powershell).

# Parte 1: «Abrir ubicación del archivo»

Solo usa APIs de Windows: `ole32`, `shell32` y `msi`.

## Qué hace

Al hacer clic derecho sobre un acceso directo (`.lnk`), el menú ofrece «Abrir ubicación del archivo», justo después de «Abrir con…». LFind abre en el mismo panel la carpeta del destino y deja el destino seleccionado y a la vista. No abre otra ventana ni el Explorador.

| Destino | Resultado |
|---|---|
| Documento o programa | Abre su carpeta y lo selecciona |
| Carpeta | Abre la carpeta que la contiene y la selecciona, como el Explorador |
| Raíz de una unidad | Abre la unidad |
| Acceso directo de Windows Installer (Office y otros) | Se pregunta la ruta a Windows Installer |
| Destino borrado | «El destino de este acceso directo ya no existe: …» |
| Elemento especial («Panel de control», «Este equipo») | «Este acceso directo apunta a «Panel de control», un elemento especial de Windows…» |
| Destino oculto, con los ocultos sin mostrar | Abre la carpeta y avisa de que el destino no se ve |
| Acceso directo dañado | «El acceso directo está dañado o no se puede leer.» |

La opción no aparece con varios elementos seleccionados, ni en carpetas, archivos `.url` u otros archivos.

## Cómo

- **`crates/nova-core/src/lnk.rs` (nuevo).**
  - Lee el destino con `IShellLinkW` + `IPersistFile` (`GetPath`, `GetArguments`), en un hilo con COM propio.
  - No usa `IShellLink::Resolve`, porque puede buscar por el disco y mostrar diálogos.
  - Para los accesos anunciados (`SLDF_HAS_DARWINID`) usa `MsiGetShortcutTargetW` + `MsiGetComponentPathW`.
  - Los elementos del shell sin ruta se nombran con `SHGetNameFromIDList`.
- **Comando `resolve_shortcut`** (`src-tauri/src/commands.rs`): valida la ruta y lee el destino fuera del hilo principal.
- **Menú** (`src/lib/contextMenu.ts`): `isShortcut` y la nueva acción `openTargetLocation`.
- **Acción** (`src/hooks/useFileActions.ts`): reutiliza la vía de «Abrir ubicación» de la búsqueda global: `navigate` a la carpeta y `useSearch.setReveal` del destino.
- **Selección** (`src/components/Pane.tsx`): compara el nombre sin distinguir mayúsculas, porque el `.lnk` puede guardar la ruta con otras mayúsculas, y avisa si el destino no está a la vista.

### Corrección encontrada al probar

En System32 (5 019 elementos), `notepad.exe` quedaba seleccionado pero fuera de la vista. **Causa:** por encima de 3 000 elementos la lista se ordena en un Worker, y mientras tanto `Pane` recibe la lista sin ordenar. La selección se hacía en ese momento y desplazaba la vista a la posición que el elemento tenía en la lista desordenada. Afectaba también a «Abrir ubicación» de la búsqueda global y a lo recién creado o renombrado en carpetas grandes.

**Corrección:** esos dos efectos esperan a que termine la ordenación (`sorting`) y a que la lista sea de la carpeta actual. Para lo segundo, `useDirectory` ahora indica a qué carpeta pertenece el estado (`DirState.path`).

## Archivos

| Zona | Archivos |
|---|---|
| Rust | `crates/nova-core/src/lnk.rs` (nuevo), `crates/nova-core/src/lib.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` |
| Interfaz | `src/lib/contextMenu.ts`, `src/hooks/useFileActions.ts`, `src/components/Pane.tsx`, `src/hooks/useDirectory.ts`, `src/components/ContextMenu.tsx`, `src/components/icons.ts`, `src/lib/fs/{types,tauri,browser}.ts` |
| Pruebas | `src/lib/contextMenu.test.ts`; pruebas en `lnk.rs` |
| Documentación | `CHANGELOG.md`, `PROJECT_CONTEXT.md`, este documento |

## Pruebas

### Automatizadas

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| `npm test` | 454/454 (nueva: la opción solo para un único `.lnk`, justo tras «Abrir con…») |
| Rust: `lnk` | 3/3, con accesos directos reales creados con el shell: documento con «ñ» y paréntesis, carpeta, EXE con argumentos, destino borrado, `.lnk` dañado |
| Rust con SQLite, `--locked` | 332 + 3 correctas |
| Rust sin SQLite, `--locked` | 275 + 3 correctas |
| `npm run build` y compilación de Tauri | correctos |

### Con la API real, solo lectura

Se leyeron los 233 accesos directos del menú Inicio de este equipo en 4,4 s, unos 19 ms por acceso:

- 213 se resolvieron.
- 17 apuntan a programas desinstalados, como versiones antiguas de Adobe. Un método independiente encuentra los mismos 17.
- 3 son elementos especiales: «Explorador de archivos», «Panel de control» y «Ejecutar».

### En la app real

Instalación aislada y carpeta desechable con 300 archivos de relleno:

| Prueba | Resultado |
|---|---|
| Menú en `.lnk`, `.txt` y `.url` | solo aparece en el `.lnk` |
| Documento `informe ñ (1).txt` | carpeta correcta; seleccionado y visible |
| Carpeta | abre la carpeta contenedora; carpeta seleccionada |
| Bloc de notas con argumentos `/A "C:\otra ruta\x.txt"` | abre `C:\Windows\System32` (5 019 elementos); `notepad.exe` seleccionado y visible. Antes de la corrección: seleccionado pero fuera de la vista |
| Ruta guardada en mayúsculas | selecciona `informe ñ (1).txt` |
| Destino borrado | aviso con la ruta; no navega |
| Destino oculto | abre la carpeta y avisa |
| «Panel de control» | aviso con el nombre del elemento |
| Teclado: Mayús+F10, flechas, Intro | la opción es la tercera; navega y selecciona |
| Doble panel | actúa en el panel donde se pidió |
| Ventanas del Explorador abiertas | ninguna nueva; un solo proceso de LFind |
| Preferencias e índice reales | intactos |

## Limitaciones

- **Windows Installer sin probar:** en este equipo no hay accesos directos anunciados (los de Office instalado con MSI), así que la vía de `msi.dll` no se ha probado con uno real. Si no da una ruta, el mensaje lo dice.
- **Sin resolución de destinos movidos:** si el destino se movió, LFind no lo busca, a diferencia de `Resolve` al abrir un acceso directo con Windows. Dice que ya no existe.
- **Solo `.lnk`:** los `.url` (enlaces web) y los accesos directos de aplicaciones de la Tienda que no apuntan a un archivo no tienen ubicación que abrir.

# Parte 2: rendimiento al entrar en carpetas

## Conclusión

**Entrar en una carpeta ya era rápido.** La primera fila aparece en unos 30–50 ms en este equipo, y en 150–220 ms con la CPU de la interfaz cuatro veces más lenta, como en un portátil modesto. La lectura del disco no llega a 6 ms ni con 5 000 elementos. Lo que tarda de verdad son las **miniaturas de fotos**: 2,4–2,9 s hasta llenar una pantalla de 132 en frío.

Se corrigieron tres fallos reales que afectaban a lo que se ve al entrar. La navegación en sí no se tocó, porque no tenía un cuello de botella medible, y dos optimizaciones posibles se descartaron con datos.

## Cómo se midió

Instalación aislada (`LFindNavTest`) con datos desechables:

| Carpeta | Contenido |
|---|---|
| `pequena` | 60 archivos de 30 tipos |
| `grande` | 5 000 archivos |
| `fotos` y `fotos4` | 300 JPEG de 4000×3000 (unos 800 KB) |
| `pesados` | 8 archivos de 300 MB |
| `subcarpetas` | 40 carpetas |
| `C:\Windows\System32` | carpeta real, solo lectura |

Se navega escribiendo la ruta, como el usuario, y se mide desde el Intro, fotograma a fotograma:

- primera fila de la carpeta nueva;
- lista completa, según la barra de estado;
- iconos, cuando dejan de aparecer nuevos;
- miniaturas;
- tareas largas del hilo principal y peor fotograma;
- llamadas IPC, desde las entradas de red a `ipc.localhost`, porque Tauri no deja envolver `invoke`.

Cada caso se mide con la CPU normal (×1) y frenada a ×4 (`Emulation.setCPUThrottlingRate`, que solo frena la interfaz). También se mide con el índice propio de la instalación de pruebas recorriendo C:.

## Diagnóstico

| Medida | Resultado |
|---|---|
| Lectura del disco (`bench_dir`, release) | 0,3–0,7 ms el primer lote; 5,7 ms las 5 000 entradas; 5,1 ms System32 |
| Antes de pedir la carpeta (escribiendo la ruta) | 10 ms con ×1 y 40 ms con ×4: `resolve_path` → `set_thumb_epoch` → `cancel_dir` → `open_dir` |
| Del primer lote a la primera fila | 25–35 ms con ×1 y unos 130 ms con ×4, casi todo estilos, distribución y pintado del navegador; el JavaScript propio son unos 45 ms de CPU por navegación |
| DOM | unos 780 nodos y 7 por fila: no es un problema de tamaño |
| Iconos del sistema en frío | `get_icons` tarda 180–250 ms; mientras, se ve el glifo; después, caché |
| Miniatura de una foto de 12 MP (Rust) | decodificar 24–27 ms + reducir ~19 ms + codificar 0,5–2 ms |
| Memoria al generar miniaturas en frío | Rust sube de 20 a un pico de 137 MB y vuelve a 54; WebView2 sube unos 40 MB |
| Índice indexando C: | la navegación no cambia: el indexador ya trabaja en modo de fondo de Windows (CPU y disco) |
| Navegación rápida (7 carpetas seguidas) | sin elementos de otra carpeta al final; las lecturas abandonadas se cancelan |

## Fallos corregidos

**1. Miniaturas que dejaban de aparecer tras recargar la página.**

- **Causa:** el backend solo acepta «épocas» de miniaturas que no retrocedan, y conserva la más alta. El frontend empezaba en 0 en cada carga. Tras «Recargar» en la pantalla de error, todas las peticiones se daban por obsoletas hasta haber cambiado de carpeta tantas veces como antes.
- **Medido antes:** 0 de 132 miniaturas en una carpeta ya cacheada, y el fallo seguía en toda la sesión.
- **Corrección:** el contador empieza en la hora actual en milisegundos (`thumbService`).
- **Después:** las 132 en 253 ms.

**2. Fotos giradas (las de móvil): miniatura el doble de rápida.**

- **Causa:** la orientación EXIF se aplicaba a la foto **completa** antes de reducirla, lo que suponía otra copia de 36 MB y su tiempo.
- **Corrección:** ahora se reduce y se gira la miniatura. El marco es cuadrado y el giro es exacto, así que el resultado es idéntico; lo comprueba la prueba de orientación EXIF existente.
- **Medido:** con una foto de 4000×3000 con orientación 6, de 87,6 a 44,4 ms por miniatura.

**3. Destino seleccionado pero fuera de la vista en carpetas de más de 3 000 elementos** (corregido en la parte 1). Se seleccionaba antes de que el Worker terminara de ordenar.

## Descartado, con datos

- **Reducir por bloques antes del filtro.** Ahorra 5–12 ms por foto, pero se aleja más de una referencia Lanczos3: 6,5 niveles de error frente a 2,0 con una foto real a 256 px. Empeora la calidad, así que se retiró.
- **Pedir primero la miniatura a la caché de Windows (`INCACHEONLY`).** Si la tiene, cuesta 11 ms frente a 42. Si no la tiene, cuesta 20 ms de más, y para tamaños grandes devuelve una menor que la pedida. En carpetas que nunca se abrieron en el Explorador sería un 50 % más lento.
- **Usar el proveedor de Windows para JPEG.** En frío es más lento: 72 ms frente a 42 ms, con una CPU parecida.

## Antes y después

Tiempos en ms desde el Intro, en la app real. «Antes» es la compilación de la parte 1; «después» incluye estas correcciones.

| Caso | Primera fila | Lista | Iconos | Miniaturas |
|---|---|---|---|---|
| Pequeña, 1.ª vez, ×1 | 32 → 40 | 32 → 40 | 342 → 353 | |
| Grande (5 000), 1.ª vez, ×1 | 36 → 38 | 53 → 51 | 79 → 81 | |
| Pesados, 1.ª vez, ×1 | 24 → 24 | 24 → 24 | 53 → 54 | |
| Fotos en frío, ×1 | 39 → 51 | 60 → 51 | 168 → 141 | 2 442 → 2 610 (132/132) |
| Fotos con caché, ×1 | 31 → 31 | 31 → 49 | | 31 → 49 |
| **Fotos tras recargar la página** | 31 → 45 | 44 → 45 | | **0/132 → 132/132 en 253** |
| Pequeña, ×4 | 162 → 147 | 162 → 147 | 230 → 224 | |
| Grande, ×4 | 168 → 180 | 168 → 180 | 235 → 252 | |
| Fotos en frío, ×4 | 200 → 210 | | | 0/132 → 132/132 en 2 917 (el «antes» arrastraba el fallo 1) |
| Grande con el índice trabajando, ×4 | 178 → 220 | | | |
| Navegación rápida, ×4 | 148 → 163 | | | sin restos de otras carpetas |

Las diferencias de primera fila y lista están dentro de la variación entre pasadas del mismo código (±30 ms con ×4): ese camino no se ha modificado. Las fotos de prueba no tienen orientación EXIF, así que la mejora 2 no aparece en esta tabla; está medida aparte.

## Archivos

| Archivo | Cambio |
|---|---|
| `src/lib/thumbService.ts` | época inicial a partir de la hora actual |
| `src/lib/thumbService.test.ts` | prueba: tras recargar no se reutilizan épocas antiguas |
| `crates/nova-core/src/thumbs.rs` | reducir antes de girar |

## Pruebas

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| `npm test` | 455/455 |
| Rust con SQLite, `--locked` | 332 + 3 correctas, incluidas las 26 de miniaturas (orientación EXIF) |
| Rust sin SQLite, `--locked` | 275 + 3 correctas |
| `npm run build` y compilación de Tauri | correctos |

Las medidas en la app real están arriba. Las preferencias y el índice reales quedaron intactos en todas las pasadas.

## Limitaciones

- **Portátil emulado.** El frenado ×4 solo afecta a la CPU de la interfaz: no reproduce una GPU integrada, el ahorro de energía, el antivirus ni un disco lento. No se ha probado en un portátil real.
- **Sin HDD, USB ni OneDrive reales.** Las carpetas de prueba están en un SSD y con la caché del sistema caliente, así que «primera entrada» significa primera vez para LFind, no lectura en frío del disco.
- **Las miniaturas siguen costando unos 43 ms por foto de 12 MP.** El suelo es decodificar el JPEG entero. Bajarlo de verdad exigiría decodificar a escala reducida con WIC; con la API del shell no se mejoró, y no se ha intentado directamente.
- **En System32 la primera fila no se mide bien** (sin prefijo propio, cuenta la carpeta anterior); allí vale la columna «Lista».


# Parte 3: terminales CMD y PowerShell

Las correcciones valen para cualquier equipo, no solo para uno concreto. No se cambia ninguna política de Windows, ni el perfil del usuario, ni ningún permiso.

## Causas

**1. «Abrir terminal aquí» abría una consola en la que no se podía escribir.** Ocurría cuando Windows Terminal no está instalado.

- **Causa:** se usaba `std::process::Command` con `CREATE_NEW_CONSOLE`. `Command` siempre pasa la entrada estándar del proceso padre (`STARTF_USESTDHANDLES`). En una aplicación de ventanas como LFind esa entrada es nula, así que la consola aparecía pero el shell no podía leer el teclado.
- **Comprobado** con una sonda de ventanas que lanza PowerShell de las dos formas:
  - con `Command`: `IsInputRedirected=True` y sin teclado;
  - con `CreateProcessW` sin heredar canales: `IsInputRedirected=False`, teclado correcto y carpeta de trabajo correcta.

**2. Error del perfil al abrir PowerShell.** En Windows, la política de ejecución por defecto es `Restricted`, así que cualquier equipo con un perfil de PowerShell muestra «No se puede cargar el archivo …profile.ps1 porque la ejecución de scripts está deshabilitada en este sistema». Se reprodujo de forma idéntica fijando `Restricted` solo para el proceso hijo (variable de entorno `PSExecutionPolicyPreference`); el sistema y el perfil no se tocaron.

**3. Terminal integrada.** Lo que se escribía mientras el shell arrancaba se perdía. Además, `term_open` era síncrono y se ejecutaba en el hilo de la ventana.

## Correcciones

- **Terminal externa** (`nova_core::shell::open_terminal`):
  - Si Windows Terminal está instalado, se usa, con el shell elegido.
  - Si no, se crea una consola nueva con `CreateProcessW`, sin `STARTF_USESTDHANDLES` y sin heredar handles.
  - Respeta el shell elegido en el panel de terminal: PowerShell o CMD. Antes abría siempre PowerShell.
- **Perfil de PowerShell** (`nova_core::terminal::powershell_profile_plan`):
  - Si no hay ningún perfil en disco, no se hace nada más.
  - Si lo hay, un PowerShell oculto (`-NoProfile -NonInteractive`) informa de la política efectiva, de si la fija una directiva de grupo y de qué perfiles no podrían cargarse. Tiene en cuenta `Restricted`, `AllSigned` sin firma y `RemoteSigned` con marca de Internet. El resultado se recuerda 5 minutos.
  - Si el perfil no puede cargarse, PowerShell se abre con `-NoProfile` y una explicación en amarillo, con lo que el usuario puede hacer: `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` o `Unblock-File`. Si la política viene de una directiva, se dice que solo un administrador puede cambiarla, sin sugerir nada.
  - Si el perfil sí puede cargarse, se carga como siempre.
  - LFind nunca pasa `-ExecutionPolicy` a la terminal del usuario.
- **«Abrir terminal como administrador»** (menú de una carpeta y del fondo):
  - Usa `ShellExecute` con el verbo `runas`, que muestra el aviso de UAC. Si el usuario lo rechaza, LFind dice que no se concedió el permiso.
  - La carpeta va en los argumentos (`cd /d` y `Set-Location -LiteralPath`), porque Windows ignora la carpeta de trabajo al elevar.
  - LFind sigue funcionando sin permisos de administrador.
- **Terminal integrada:**
  - `term_open` es asíncrono.
  - Lo escrito antes de que el shell esté listo se guarda y se entrega en orden.
  - El mismo plan del perfil se aplica también aquí.

## Archivos

| Archivo | Cambio |
|---|---|
| `crates/nova-core/src/terminal.rs` | `ProfilePlan`, `plan_from`, `profile_paths`, consulta de la política, `command_line(plan)`, `console_args` y pruebas |
| `crates/nova-core/src/shell.rs` | `open_terminal(dir, shell, admin)`, `new_console` (`CreateProcessW`) y `run_elevated` (UAC) |
| `src-tauri/src/system_cmd.rs`, `src-tauri/src/terminal_cmd.rs` | parámetros nuevos; `term_open` asíncrono |
| `src/lib/contextMenu.ts`, `src/hooks/useFileActions.ts`, `src/lib/fs/{types,tauri}.ts` | «Abrir terminal como administrador» y shell elegido |
| `src/components/TerminalPanel.tsx` | se guarda lo escrito durante el arranque |
| `src/lib/contextMenu.test.ts` | prueba del menú |

## Pruebas

| Prueba | Resultado |
|---|---|
| Sonda de ventanas: consola como antes / como ahora | entrada redirigida y sin teclado / teclado correcto, carpeta correcta |
| Simulación de `Restricted` solo en el proceso hijo, con el perfil real de este equipo (solo lectura) | antes: el mismo error del perfil; ahora: aviso y comandos ejecutados (`6*7` da `42`); perfil intacto, comprobado por contenido y fecha |
| Pruebas reales con ConPTY (CMD y PowerShell) | 12/12: comandos, entrada interactiva, código de salida, carpeta con espacios y comillas simples, cierre sin procesos huérfanos, PowerShell sin perfil usable |
| Decisión del perfil (`plan_from`) | sin perfil bloqueado se abre como siempre; con directiva no sugiere saltársela; rutas con comillas bien escapadas |
| Políticas de este equipo después de todo | sin cambios (`CurrentUser RemoteSigned`; MachinePolicy y UserPolicy `Undefined`) |
| `npm run typecheck` / `npm test` | sin errores / 456/456 |
| Rust con SQLite / sin SQLite | 335 + 3 / 278 + 3 |
| `npm run build` y compilación de Tauri | correctos |

## Pendiente y limitaciones

- **Sin probar en la app instalada con una persona escribiendo:** la consola externa y la terminal integrada se comprobaron con sondas y con pruebas de ConPTY, no tecleando a mano.
- **UAC sin probar:** el aviso aparece en el escritorio seguro de Windows, que no se puede automatizar. La aceptación y el rechazo están implementados, pero nadie los ha pulsado.
- **PowerShell 7 (`pwsh`)** no se ofrece: no está instalado en este equipo para probarlo. Se usa Windows PowerShell 5.1.
- **Operaciones de archivo que necesitan administrador** (carpetas protegidas del sistema): siguen dando «Acceso denegado» con su motivo. Elevarlas con UAC exigiría un proceso auxiliar elevado, un cambio de arquitectura que no se ha hecho.
- **Arranque de LFind:** se investigó y no se cambió nada, a petición del usuario. El problema se atribuyó a un portátil concreto. Lo observado:
  - la ventana se muestra cuando están las preferencias y los lugares;
  - el indexador arranca a los 3 s, con prioridad de fondo, y solo recorre lo que no está completo o ha caducado;
  - al arrancar, «Enviar a» lanza `powershell.exe` para leer sus accesos directos; podría leerlos con `nova_core::lnk` si algún día hace falta.
