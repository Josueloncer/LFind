# LFind

Explorador de archivos para Windows, rápido y personalizable. Tauri 2, Rust, React y TypeScript.

> Antes «NOVA Explorer». El identificador interno (`app.nova.explorer`) y la carpeta de datos conservan ese nombre para no perder los datos de quien ya lo tenía instalado.

## Qué incluye

- Pestañas, cinco modos de vista, orden, agrupación y vista de **doble panel**.
- **Búsqueda global** en todas las unidades con un índice local.
- **Visor inteligente** y miniaturas.
- **Analizador de almacenamiento** con mapa de carpetas y actualización incremental.
- **Personalización:** reorganizar barras, fondos, colores, tipografía y temas propios.
- Integración con OneDrive sin descargar archivos, y actualizaciones automáticas firmadas.

## Requisitos (Windows 10/11 x64)

| Herramienta | Instalación |
|---|---|
| Microsoft C++ Build Tools (*Desarrollo de escritorio con C++*) | `winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` |
| WebView2 Runtime | Incluido en Windows 11 y en Windows 10 actualizado |
| Rust (≥ 1.77.2, `stable-msvc`) | `winget install Rustlang.Rustup` y luego `rustup default stable-msvc` |
| Node.js LTS (≥ 20) | `winget install OpenJS.NodeJS.LTS` |

## Desarrollo

```powershell
npm install
npm run tauri dev
```

`npm run dev` abre solo la interfaz en el navegador, sin acceso a disco.

## Pruebas

```powershell
npm run typecheck   # TypeScript, incluidas las pruebas
npm test            # Vitest
npm run test:core   # núcleo Rust
```

## Instalador y publicación

`npm run tauri build` genera el instalador en `src-tauri\target\release\bundle\nsis\`.

Cada push a `main` con cambios de código publica una versión firmada (`.github/workflows/release.yml`). La primera sección de `CHANGELOG.md` son las novedades que ven los usuarios. Ver [DEVELOPMENT_GUIDE.md](DEVELOPMENT_GUIDE.md).

## Atajos

| Atajo | Acción |
|---|---|
| `Alt+←` / `Alt+→` / `Alt+↑` | Atrás / Adelante / Subir |
| `Ctrl+T` / `Ctrl+W` | Nueva pestaña / Cerrar pestaña |
| `Ctrl+L` | Escribir una ruta |
| `Ctrl+F` / `Ctrl+Mayús+F` | Filtrar la carpeta / Buscar en todo |
| `Ctrl+B` / `Alt+P` | Panel lateral / Vista previa |
| `Ctrl+Mayús+D` | Doble panel |
| `F6` / `Ctrl+Mayús+X` | Cambiar de panel / Intercambiar paneles |
| `Ctrl+Mayús+C` / `Ctrl+Mayús+M` | Copiar / Mover al otro panel |
| `Alt` + arrastrar | Mover una herramienta |
| `F5` | Actualizar |

## Documentación

| Documento | Contenido |
|---|---|
| [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md) | Contexto técnico vigente y reglas de desarrollo |
| [PROJECT_STATUS.md](PROJECT_STATUS.md) | Estado actual, pruebas y pendientes |
| [PROJECT_ARCHITECTURE.md](PROJECT_ARCHITECTURE.md) | Módulos y flujos |
| [DEVELOPMENT_GUIDE.md](DEVELOPMENT_GUIDE.md) | Cómo desarrollar y publicar |
| [WINDOWS_VERIFICATION.md](WINDOWS_VERIFICATION.md) | Comprobaciones en Windows |
| [docs/history/](docs/history/) | Informes históricos de cada actualización |
