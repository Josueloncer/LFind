# Update 14: barra lateral, Papelera, diálogos de Windows y desenfoque por zona

Sin commit, push ni publicación. La versión no se ha cambiado.

## 1. Qué cambió

**Barra lateral.** Su borde interior cambia el ancho: cursor `col-resize`, zona de agarre de 9 px y línea de acento al pasar por encima. Límites de 180 a 480 px, dejando siempre 420 px para el contenido. El ancho se guarda al soltar. Esc cancela, las flechas lo ajustan y el doble clic vuelve a 240 px. Mover de lado y cambiar el ancho son independientes.

**Papelera.** La eliminación definitiva va por tandas de 25, muestra el progreso y se puede cancelar. El resultado se comprueba volviendo a consultar la Papelera. Si algo no se puede borrar, se dice cuál y por qué. Solo se borra lo que llega en la lista: una lista vacía es un error, nunca «todo».

**Desenfoque por zona.** Barra lateral, contenido, panel secundario, y menús y diálogos tienen cada uno su interruptor, intensidad de desenfoque y transparencia. Quien ya tenía un fondo lo sigue viendo igual.

**Arreglo encontrado al probar:** tras mover archivos al Explorador, la lista de LFind seguía mostrándolos.

## 2. Archivos modificados

| Zona | Archivos |
|---|---|
| Rust, nuevo | `crates/nova-core/src/recycle.rs` |
| Rust | `crates/nova-core/src/lib.rs`, `crates/nova-core/src/shellapps.rs`, `src-tauri/src/shellapps_cmd.rs`, `src-tauri/src/lib.rs` |
| Interfaz | `src/App.tsx`, `src/components/Sidebar.tsx`, `src/components/RecycleView.tsx`, `src/components/AppearancePanel.tsx`, `src/components/Pane.tsx` |
| Lógica | `src/lib/appearance.ts`, `src/lib/recycle.ts`, `src/stores/backgrounds.ts`, `src/lib/fs/{types,tauri,browser}.ts` |
| Estilos | `src/styles/app.css` |
| Pruebas | `src/lib/zones.test.ts` (nuevo), `src/lib/appearance.test.ts`, `src/lib/recycle.test.ts`, `src/stores/backgrounds.test.ts` |
| Documentación | `PROJECT_CONTEXT.md`, este documento |

## 3. Causa raíz de los errores

**Papelera.** Se reprodujo con archivos desechables, usando la misma biblioteca (`trash` 5.2.9):

| Caso | Lo que devolvía `purge_all` | Lo que pasó de verdad |
|---|---|---|
| 1, 5 y 300 archivos, solo lectura, nombres especiales, carpeta de 4 000 archivos | éxito, de 0,3 a 1,2 s | borrados |
| Archivo en uso | **éxito** | **sigue en la Papelera** |
| Lote con uno en uso | **éxito** | los demás borrados, ese no |
| Carpeta con un archivo en uso | **éxito** | **vaciada a medias, y sigue listada** |

La biblioteca no comprueba si Windows abortó parte de la operación. LFind anunciaba «Eliminado definitivamente» y el elemento seguía ahí, que es lo que se percibe como que no termina de borrar. Además, toda la selección era una sola operación, sin progreso ni cancelación, con la ventana cubierta por un cuadro que no se podía cerrar: con una carpeta grande parecía colgada.

No se reprodujo ninguna espera infinita y el hilo principal no se bloqueaba: la operación ya corría fuera de él. Aun así, cada tanda se vigila desde otro hilo, de modo que la interfaz siempre recibe respuesta. Las tandas solo de archivos se dan por perdidas a los 60 s; las que incluyen carpetas no tienen límite, pero se pueden cancelar.

**Lista desactualizada tras mover al Explorador.** En un movimiento optimizado, el destino hace el trabajo y Windows informa «ningún efecto», así que LFind creía que no había pasado nada.

## 4. Pruebas realizadas y resultados

Automáticas:

| Comprobación | Resultado |
|---|---|
| `npm run typecheck` | sin errores |
| `npm test` | 440/440 (29 nuevas) |
| Rust con SQLite, `--locked` | 324 + 3 correctas (15 nuevas) |
| Rust con SQLite y carpeta temporal en formato corto, como en GitHub | 324 + 3 correctas |
| Rust sin SQLite, `--locked` | 267 + 3 correctas |
| `npm run build` y `tauri build` de producción | correctos |

En la app real, con una instalación aislada (preferencias e índice reales intactos) y archivos desechables:

| Prueba | Resultado |
|---|---|
| Ancho con el cursor real del sistema | de 240 a 351 px, guardado |
| Ancho: límites, Esc, lado derecho, reinicio, doble clic | 180 y 480 px; Esc lo conserva; se mide desde el borde correcto; persiste; vuelve a 240 |
| Eliminar varios | borrados en 1,3 s, con confirmación |
| Elementos ajenos de la Papelera, uno a uno | 0 desaparecidos en todas las pruebas |
| Archivo en uso | «Está en uso por otra aplicación»; sigue en la lista; se borra al liberarlo |
| Lote con uno en uso | 2 borrados, se nombra el tercero |
| Carpeta con un archivo en uso | «Contiene un archivo en uso: tapon.txt» |
| 300 elementos, cancelando | se detiene en 50 y deja 250, con el recuento exacto; interfaz respondiendo en 4 ms como máximo |
| Los 250 restantes | 2,6 s |
| «Vaciar», cancelando la confirmación | no borra nada |
| Zonas: migración de preferencias antiguas | mismos valores, sin capas de más |
| Zonas: independencia | cambiar una no altera ninguna variable de las demás |
| Zonas: doble panel | cada panel con sus valores |
| Zonas: persistencia y seis temas | correctas |
| Zonas: filtros | ningún filtro en vivo fuera de menús y diálogos |
| Deslizar el control 20 pasos | 269 ms, sin retirar la capa |
| Arrastre físico al Explorador, varios archivos | movidos, y la lista de LFind se actualiza |
| Ctrl al arrastrar al Explorador | copia |
| Arrastre físico entre paneles | movido |
| Arrastre físico a Edge | Edge abre el archivo; el original sigue en su sitio |

**Integración con Windows:**

| Prueba | Desactivada | Activada |
|---|---|---|
| Doble clic en una carpeta del escritorio (por mensajes) | Explorador | LFind, sin CMD, una instancia |
| Diálogo Abrir de .NET | funciona | funciona, no abre LFind |
| Diálogo Abrir de Edge | no probado | navega a una carpeta sin abrir LFind y devuelve el archivo |
| Diálogo Guardar de .NET | aparece, no abre LFind | aparece, no abre LFind |
| Restaurar | | Registro idéntico |

**Sin probar:**
- Arrastre físico al escritorio: no había ningún punto del escritorio visible.
- Completar un guardado: la herramienta de prueba no localiza el campo de nombre del cuadro Guardar. En Edge apareció antes un aviso del navegador que la prueba no acepta.
- «Vaciar» confirmado: habría borrado tu Papelera real. Usa el mismo código que eliminar una selección.
- Archivos solo en la nube dentro de la Papelera: no se han creado para no tocar tu OneDrive.
- Nada lo ha hecho una persona con un ratón: el cursor real se movió por programa.

## 5. Limitaciones reales de Windows

### Por qué LFind no puede sustituir el selector de archivos de otras aplicaciones

**No existe una interfaz oficial para hacerlo.** Microsoft documenta cómo una aplicación *usa* el cuadro y cómo *añadirle controles al suyo* (`IFileDialog`, `IFileDialogCustomize`). No hay ningún mecanismo para que un programa reemplace el de los demás.

**Cómo funciona de verdad.** En este equipo, `CLSID_FileOpenDialog` y `CLSID_FileSaveDialog` apuntan a `C:\Windows\System32\comdlg32.dll` como servidor *dentro del proceso*. El cuadro no es otro programa al que se pueda desviar: es código que se carga en cada aplicación.

La única vía técnica sería registrar una DLL propia bajo `HKCU\Software\Classes\CLSID` con esos identificadores. No se va a hacer, por estos motivos:

- **Es una técnica de ataque catalogada:** [secuestro de COM, MITRE T1546.015](https://attack.mitre.org/techniques/T1546/015/). Los antivirus y sistemas de detección la vigilan expresamente.
- **Inyectaría código en todas las aplicaciones:** una DLL de LFind dentro de cada programa, en 32 y 64 bits. Un fallo suyo tiraría abajo el programa ajeno.
- **Habría que reimplementar todas las interfaces del cuadro:** filtros, controles añadidos por cada aplicación, eventos, selección múltiple, y las rutas y permisos que espera cada una. Cualquier hueco rompe esa aplicación.
- **No cubriría todo:** [las aplicaciones elevadas no usan el registro del usuario](https://attackiq.com/2020/03/26/component-object-model-hijacking), y las antiguas usan `GetOpenFileName`, que es otra vía.
- **Los navegadores:** Chromium [ejecuta el cuadro en un proceso auxiliar](https://chromium.googlesource.com/chromium/src/+/HEAD/chrome/browser/win/chrome_select_file_dialog_factory.h) precisamente para aislarse de los módulos ajenos. En esta prueba, el cuadro de Edge apareció en un proceso distinto del de la ventana.

Otros exploradores de archivos han llegado a la misma conclusión: [Directory Opus indica que no reemplaza esos cuadros](https://github.com/Chaoses-Ib/DirectoryOpus/blob/main/Manual/basic_concepts/explorer_replacement.md), porque [no hay una buena forma de hacerlo sin causar problemas de compatibilidad](https://resource.dopus.com/t/can-opus-replace-the-file-open-save-dialogs-other-programs-use/36127).

La integración actual no toca estos cuadros: solo escribe las acciones de carpeta en `HKCU`.

### La alternativa real más cercana

1. **Disponible hoy, arrastrar desde LFind:** sueltas el archivo en la zona de subida de la web. Se ha comprobado que el arrastre nativo llega a Edge. No se ha probado sobre una zona de subida de un sitio concreto.
2. **Disponible hoy, «Copiar ruta de acceso» en LFind y pegarla** en el campo «Nombre» del cuadro Abrir. El campo acepta una ruta completa: la prueba escribió en él una carpeta y un archivo, en .NET y en Edge. No se ha probado con varias rutas a la vez.
3. **No implementado, «salto rápido»:** con un cuadro abierto, LFind ofrecería llevarlo a la carpeta que tienes abierta, escribiendo la ruta en ese campo desde fuera, sin inyectar nada. Es lo que hacen herramientas como [Listary](https://discussion.listary.com/t/quick-switch-directory-opus-libraries/3216), que tampoco funcionan con los cuadros no estándar. El cuadro seguiría siendo el de Windows.

### Otras limitaciones

- El desenfoque de menús y diálogos es en vivo (`backdrop-filter`), porque pasan sobre contenido que cambia. Solo existe mientras están abiertos y solo si se activa.
- Con un fondo y varias zonas distintas se guarda una copia de la imagen por cada desenfoque diferente. En la prueba, con todo el panel abierto y varias zonas distintas, WebView2 pasó de 481 a 565 MB.
- Un archivo en uso no se puede borrar hasta que su aplicación lo libere: lo decide Windows.

## 6. Cómo ejecutar y verificar

```powershell
npm run typecheck
npm test
cd crates/nova-core; cargo test --locked --features sqlite; cargo test --locked; cd ../..
npm run build
npx tauri build --config '{\"bundle\":{\"createUpdaterArtifacts\":false}}'
```

Comprobación manual:

1. Arrastra el borde de la barra lateral, reinicia LFind y comprueba que conserva el ancho. Muévela al otro lado con el asa superior: el ancho no cambia.
2. Envía a la Papelera un archivo abierto en una aplicación que lo bloquee. Elimínalo desde LFind: debe decir que está en uso. Cierra la aplicación y repite.
3. Pon un fondo en Personalización. En «Por zona», cambia una zona y comprueba que las demás no se mueven.
4. Con la integración activada y desactivada, abre un cuadro de subida en el navegador: sigue siendo el de Windows y funciona igual.
