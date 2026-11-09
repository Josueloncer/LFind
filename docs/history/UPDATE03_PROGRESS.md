# LFind · Actualización 03 · Registro de progreso

> Si la sesión se interrumpe: leer este archivo, ir a «Siguiente paso exacto» y continuar. No repetir etapas marcadas ✅.

## Decisiones del usuario (autorizadas)
1. Vistas: cada cambio de modo/tamaño actualiza la vista GLOBAL. La vista por carpeta solo se guarda con una acción explícita («Recordar vista para esta carpeta»). Carpetas sin configuración propia heredan la global.
2. Nombre visible NOVA → LFind. Se conservan `identifier` (`app.nova.explorer`), carpeta de preferencias y nombres internos (`nova-core`, claves `nova.*` de localStorage, etc.).
3. Temas: Cyberpunk (negro + cian/magenta), Hacker/Terminal (negro + verde fósforo, monoespaciada), Minimal (monocromo), Dark, Light, System.

## Entorno
- Sin acceso a npm ni crates.io → no se puede ejecutar tsc/vitest/vite/cargo del proyecto aquí. Toda verificación posible se hace con herramientas globales (tsc sin tipos de terceros, rustc sobre módulos sin dependencias, node, Playwright sobre CSS).
- Sin Windows: nada de `#[cfg(windows)]` ni `src-tauri` se puede compilar aquí.

## Etapas
- [x] 1. Sistema de tokens ✅
- [x] 2. Temas (6) ✅
- [x] 3. Persistencia de vistas (opción A) ✅
- [x] 4. Iconos nativos de Windows ✅ (Rust/FFI sin compilar en Windows)
- [x] 5. Renombrado a LFind ✅
- [x] 6. Verificación, CHANGELOG, instrucciones Windows, ZIP ✅

## Registro
(se rellena por etapa: qué se modificó · qué se verificó · siguiente paso)

### Etapa 1 · tokens ✅
- Nuevos: `src/styles/themes.css` (5 bloques; «system» se resuelve a dark/light). `tokens.css` reducido a escalas (tipografía, espaciado, tamaños, movimiento, capas) + derivados del acento.
- Modificados: `app.css` (sin colores/sombras/fuentes sueltos), `Glyphs.tsx` (colores por categoría y degradados desde tokens), `main.tsx` (importa themes.css).
- Verificado: script (mismas 62 variables en los 5 temas; ninguna variable usada sin definir; `app.css` sin `#hex` ni `rgba()` literales). Captura Playwright de una maqueta con las 5 paletas (revisadas a ojo).
- Nota: se mantienen valores literales de geometría (alto de filas, anchos de celdas, tamaños de icono) porque `viewGeometry.ts`/`FileView` los duplican en JS para la virtualización; los pasos finos de espaciado se llaman `--sp-f*`.

### Etapa 2 · temas ✅
- Nuevos: `src/lib/themes.ts`, `src/lib/themes.test.ts`. Modificados: `fs/types.ts` (Theme), `prefs.rs` (enum con `#[serde(other)]` → System, test), `App.tsx`, `Sidebar.tsx` (menú selector con 6 temas, icono Palette), `icons.ts`, `fs/tauri.ts` (tema nativo de la ventana vía `schemeOf`), `stores/prefs.ts` (normaliza tema), `public/boot.js`, `index.html`.
- Mica solo con dark/light; cyberpunk/hacker/minimal mantienen su fondo.

### Etapa 3 · vistas ✅ (lógica verificada; integración UI sin ejecutar)
- Regla A: modo y tamaño → vista GLOBAL; orden/agrupación → por carpeta (como antes); «Recordar esta vista solo para esta carpeta» (`pinned`) es explícito. Entradas antiguas sin `pinned` heredan modo/tamaño global y conservan su orden.
- Guardado inmediato (`flush`) al cerrar con el botón de la ventana y en `pagehide`/`beforeunload`.
- Modificados: `viewState.ts`, `stores/prefs.ts`, `Pane.tsx`, `Toolbar.tsx`, `menus.ts`, `TitleBar.tsx`, `App.tsx`, `prefs.rs` (campo `pinned` con `#[serde(default)]`), tests `layout.test.ts`, `menus.test.ts`.
- Verificado con arnés propio (`/tmp/.../harness.js`, sustituto de vitest): 56 pruebas TS de lógica OK. Cargo no ejecutable aquí.

### Etapa 4 · iconos nativos ✅ (parcial: no ejecutado en Windows)
- Ya existía en el directorio (de una pasada anterior de esta sesión, no registrada): `crates/nova-core/src/icons.rs` (servicio por lotes: clasificación tipo/archivo, claves, caché memoria+disco, hilos con tiempo límite, 10 pruebas) y su declaración en `lib.rs`. Se revisó y se validó con un stub de `image`: 10/10 OK.
- Nuevo: `icons_win.rs` (FFI: `SHGetFileInfoW` + `SHGetImageList` + `ImageList_GetIcon` + `GetIconInfo/GetDIBits`; NO ejecuta nada, NO usa `THUMBNAILONLY`). Comprobado de tipos con `rustc --cfg nova_winck`; **no ejecutado** (sin Windows).
- Nuevo: `src-tauri/src/icons_cmd.rs` (`get_icons`, `clear_icon_cache`), registrado en `lib.rs`. **Sin compilar** (sin crates.io).
- UI: `lib/iconKinds.ts`, `lib/iconService.ts` (lotes, dedupe por tipo, LRU, cancelación por carpeta), `hooks/useSystemIcon.ts`, `components/EntryIcon.tsx`; integrado en `FileView` (detalles/lista/mosaicos/contenido/iconos) y `Thumb` (el icono se ve primero; la miniatura lo sustituye). Pref `systemIcons` (Rust + TS + menú Ver «Usar iconos del sistema»). `clearThumbCache` también vacía iconos.
- Pruebas: `iconService.test.ts` (13, arnés propio OK).
- Decisión: DLL comparten el icono del tipo (como el Explorador); EXE/SCR/LNK/URL/ICO/CPL/appref-ms, raíces de unidad y carpetas con `desktop.ini` que declara icono usan el icono propio.

### Etapa 5 · renombrado ✅
- Visible: título HTML, `productName`, título de ventana, README, PROJECT_STATUS, aria-label, descripciones de Cargo, mensajes. Conservados: `identifier`, claves `nova.*`, `nova-core`, `nova-explorer` (paquete npm/crate), carpeta de caché de reserva, prefijo de logs `[nova]`, eventos `nova:*`.

### Etapa 6 · cierre ✅
- `CHANGELOG.md`, `WINDOWS_VERIFICATION.md` escritos. Arnés TS: 97 OK (68 originales + 29 nuevas); los 4 fallos y 2 errores de carga son idénticos en el código original (limitaciones del arnés).
- ZIP generado: `LFind-update-03.zip` (solo archivos nuevos y modificados).

## Siguiente paso exacto
En Windows: seguir `WINDOWS_VERIFICATION.md` (npm ci → typecheck → test → cargo test → cargo check src-tauri → tauri build → lista manual). Nada más pendiente de implementación.
