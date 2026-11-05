# LFind

> Antes «NOVA Explorer». El nombre visible cambió en la actualización 03; el identificador interno (`app.nova.explorer`), la carpeta de preferencias, las claves `nova.*` del almacenamiento local y los nombres de crates (`nova-core`) se conservan a propósito para no perder la configuración de nadie.

Explorador de archivos para Windows: Tauri 2 + Rust + React + TypeScript.
Estado actual: **Fase 1** (ventana premium + lectura real de carpetas). Ver `PROJECT_STATUS.md`.

## Requisitos (Windows 10/11 x64)

| Herramienta | Instalación |
|---|---|
| Microsoft C++ Build Tools (carga *Desarrollo de escritorio con C++*) | `winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` |
| WebView2 Runtime | Incluido en Windows 11 y en Windows 10 actualizado |
| Rust (≥ 1.77.2, toolchain `stable-msvc`) | `winget install Rustlang.Rustup` y luego `rustup default stable-msvc` |
| Node.js LTS (≥ 20) | `winget install OpenJS.NodeJS.LTS` |

Cierra y reabre PowerShell tras instalar. Comprueba: `node -v; npm -v; rustc -V; cargo -V`.

## Ejecutar en desarrollo

```powershell
cd C:\Dev\nova-explorer
npm install
npm run tauri dev
```

La **primera compilación de Rust tarda varios minutos** (descarga y compila Tauri). Las siguientes son rápidas.

## Pruebas

```powershell
npm test            # frontend: rutas, navegación, ordenación (vitest)
npm run typecheck   # TypeScript estricto
npm run test:core   # núcleo Rust (nova-core): lectura de directorios, ubicaciones, preferencias
```

`npm run test:core` en Windows ejecuta además las ramas específicas de Windows
(atributos de ocultos y `GetLogicalDrives`), que no se han podido compilar fuera de Windows.

## Generar el instalador

```powershell
npm run tauri build
```

Salida: `src-tauri\target\release\bundle\nsis\` (instalador `.exe`).

## Interfaz sin Tauri (solo diseño)

`npm run dev` abre la interfaz en el navegador (http://localhost:1420). **No hay acceso a disco**:
se muestra un aviso, no archivos inventados. Sirve para revisar estilos, tema y atajos.

## Atajos

| Atajo | Acción |
|---|---|
| `Alt+←` / `Alt+→` / `Alt+↑` o `Backspace` | Atrás / Adelante / Subir |
| `Ctrl+T` / `Ctrl+W` | Nueva pestaña / Cerrar pestaña (clic central también cierra) |
| `Ctrl+L` | Escribir una ruta |
| `Ctrl+F` | Filtrar la carpeta actual |
| `Ctrl+B` | Mostrar/ocultar panel lateral |
| `F5` | Actualizar |
| Flechas, `Inicio`, `Fin`, `Ctrl+A`, `Intro`, `Esc` | Selección y apertura de carpetas |

## Estructura

```
crates/nova-core/     Lógica de archivos en Rust puro (sin Tauri): dir, places, prefs, search*, thumbs*
src-tauri/            Capa Tauri: comandos, estado, permisos, configuración de ventana
src/lib/fs/           Contrato FsProvider (Tauri real / navegador sin disco)
src/stores/           Estado: pestañas+historial, preferencias, ubicaciones
src/hooks/            useDirectory: carga incremental y cancelable
src/components/       TitleBar, Sidebar, Toolbar, FileView (virtualizada), Pane…
src/styles/           tokens.css (temas) + app.css
scripts/make-icons.py Regenera los iconos (requiere Pillow)
```
\* módulos reservados, aún sin implementación.

## Si algo falla al compilar

Copia el error completo de PowerShell y pásaselo a Claude junto con `PROJECT_STATUS.md`.
