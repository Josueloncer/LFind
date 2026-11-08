# LFind · PROJECT_ARCHITECTURE

Cómo se relacionan los módulos. Auditoría **Fase 0 — 2026-11-08**. Antes de cambiar algo, comprobar contra el código (regla 4 de [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md)).

## 1. Vista general

```mermaid
flowchart TB
  subgraph WebView2["WebView2 · React + TypeScript"]
    UI["components/*"]
    H["hooks/*"]
    S["stores/* (zustand)"]
    L["lib/* (lógica pura y colas)"]
    W["sort.worker"]
    FS["lib/fs · FsProvider"]
    UI --> H --> FS
    UI --> S --> FS
    UI --> L
    H --> L
    L --> FS
    L -.-> W
  end
  subgraph Tauri["src-tauri · capa fina"]
    CMD["51 comandos + protocolo lfview"]
    ST["AppState · ThumbState · IconState · IndexState"]
    CMD --> ST
  end
  subgraph Core["crates/nova-core · sin Tauri"]
    OPS["ops · undo · safety · names"]
    DIR["dir · places · volumes · props · pathbar"]
    MEDIA["thumbs · icons · preview"]
    SHELL["shell · archive · sendto · newmenu · clipboard"]
    IDX["index/*"]
    PREF["prefs"]
  end
  FS -- "invoke / Channel (JSON)" --> CMD
  CMD --> OPS & DIR & MEDIA & SHELL & IDX & PREF
  IDX --> DB[("SQLite WAL<br/>lfind-index.db")]
  MEDIA --> CACHE[("Caché en disco<br/>thumbs / icons")]
  PREF --> PJ[("prefs.json")]
  OPS --> DISK[("Sistema de archivos")]
  DIR --> DISK
  SHELL --> WIN["Windows: Shell, tar.exe,<br/>reg.exe, PowerShell"]
```

## 2. Arranque

```mermaid
sequenceDiagram
  participant R as Rust (setup, hilo principal)
  participant W as WebView
  participant A as App.tsx
  R->>R: AppState, ThumbService
  R->>R: index_cmd::init · abre SQLite (síncrono)
  R->>R: IconService · efecto Mica
  R-->>R: hilo: mostrar ventana a los 6 s si sigue oculta
  R-->>R: hilo: indexador tras 3 s, revisión cada 10 min
  R->>W: carga index.html (ventana oculta)
  W->>W: fondo en línea + boot.js (tema y Mica cacheados)
  W->>A: main.tsx monta App
  A->>R: get_prefs + get_places + get_platform_info
  A-->>R: temporizador: mostrar ventana a 1,2 s
  R-->>A: datos
  A->>A: aplica tema, nav.init(primera ubicación)
  A->>R: core:window show
  A->>R: open_dir, get_drive_usage, index_status
  A-->>R: en reposo: list_send_to, list_new_templates
```

Puntos clave: `index_cmd::init` se ejecuta **dentro de `setup`** (ver R-08 en `PROJECT_STATUS.md`). Un fallo al abrir el índice no impide arrancar: la búsqueda queda desactivada con su mensaje.

## 3. Construcción de la interfaz

```mermaid
flowchart TB
  App --> GlyphDefs & TitleBar & Sidebar & Pane & ContextMenu & Dialogs & Overlays
  Pane --> Toolbar & StatusBar & PreviewPane
  Pane -->|"ámbito carpeta"| FileView
  Pane -->|"ámbito global"| SearchView
  Dialogs --> DeleteDialog & PropertiesDialog & IndexDialog & ConflictDialog
  FileView --> Item --> Visual
  Visual --> Thumb --> EntryIcon --> EntryGlyph
  SearchView --> EntryIcon
```

| Store | Responsabilidad |
|---|---|
| `nav` | Pestañas, historial atrás/adelante |
| `prefs` | Preferencias; guardado diferido 400 ms; `flush` al cerrar |
| `app` | Ubicaciones, unidades, Mica, uso de discos |
| `ui` | Toasts, menú abierto, diálogo actual |
| `ops` | Cola de operaciones, ocultación optimista, deshacer |
| `clipboard` | Portapapeles interno |
| `shellData` | «Enviar a» y plantillas, precargados en reposo |
| `search` | Ámbito, texto, filtros, resultados (delegado en `SearchController`) |
| `indexStatus` | Sondeo de estado: 1,5 s indexando, 10 s en reposo |

**Atajos:** repartidos en tres lugares. `App.tsx` (navegación, pestañas, Ctrl+F, Ctrl+Mayús+F, Ctrl+B, Alt+P), `Pane.tsx` (operaciones sobre la selección, F5) y `FileView.tsx` (flechas, Ctrl+A, escritura rápida, menú por teclado). `SearchView` tiene los suyos.

## 4. Comunicación Rust ↔ TypeScript

| Patrón | Cuándo | Ejemplos |
|---|---|---|
| `invoke` petición/respuesta | Resultado único | `get_prefs`, `rename_path`, `index_search`, `get_thumbnail` |
| `invoke` + `Channel<T>` | Progreso o flujo | `open_dir`, `start_op`, `start_stats` |
| Id + cancelación | Trabajos largos | `cancel_dir`, `cancel_job`, `resolve_conflict` |
| Sondeo | Estado del índice | `index_status` |

No se usa el sistema de eventos globales de Tauri. Verificado en la Update 07: los 51 comandos registrados coinciden con los 51 usados en `lib/fs/tauri.ts`, único archivo que llama a `invoke`. El contenido de archivos para el visor no viaja por IPC, sino por el protocolo `lfview` (§8 bis).

**Tipos:** Rust serializa con `camelCase`; `lib/fs/types.ts` es el espejo manual. No hay generación automática: un cambio exige tocar ambos lados.

**Errores:** los comandos devuelven `String` u objetos (`OpError`, `ThumbFail`); `errMsg()` extrae el texto y se muestra con `toast`. Los mensajes ya vienen en español desde Rust.

**Permisos:** `capabilities/default.json` solo concede operaciones de ventana. Los comandos propios los valida Rust. CSP: `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost`. La Update 07 añade solo `http://lfview.localhost` a `img-src`, `media-src` y `connect-src`, y cierra `object-src`, `frame-src`, `base-uri` y `form-action`. Sin `asset:` ni `blob:`.

## 5. Lectura de una carpeta

```mermaid
sequenceDiagram
  participant P as Pane / useDirectory
  participant F as TauriProvider
  participant C as open_dir
  participant D as dir::read_dir_batched
  P->>F: openDir(path, onEvent)
  F->>C: invoke con Channel
  C-->>F: id (inmediato)
  C->>D: spawn_blocking
  loop Lotes (64, luego 1500)
    D-->>P: batch · primero inmediato, resto cada 60 ms
  end
  D-->>P: done
  P->>P: useSortedEntries: síncrono ≤ 3000, Worker si más
  P->>P: filtro · ocultos · layout · virtualización
  Note over P,C: Cambiar de carpeta: live=false + cancel_dir
```

## 6. Operaciones de archivo

```mermaid
flowchart LR
  A["useFileActions / DnD / menú"] --> Q["useOps.start"]
  Q -->|"máx. 2 a la vez"| SO["start_op"]
  SO --> RO["run_op"]
  RO -->|parse_path| SF["safety"]
  RO --> C["copy_items / move_items"]
  RO --> D["delete_items"]
  RO --> T["trash_items · lotes de 64"]
  C -->|"temporal + rename"| FS[(Disco)]
  C -.->|"Conflict"| DLG["ConflictDialog"]
  DLG -.->|"resolve_conflict"| C
  SO -->|"undo"| U["Pila en AppState"]
  SO -->|"Done"| Q
  Q --> V["version++ → recarga de carpeta"]
```

## 7. Búsqueda global e indexación

### Indexado

```mermaid
flowchart TB
  T["Hilo ticker: 3 s, luego cada 10 min"] --> S["Indexer::start"]
  S --> H["Hilo lfind-index · prioridad de fondo"]
  H --> R{"Por cada raíz:<br/>¿conectada y pendiente,<br/>interrumpida o caducada?"}
  R -- no --> R
  R -- sí --> B["begin_scan: gen + 1"]
  B --> W["scan_root: recorrido DFS<br/>sin seguir enlaces"]
  W --> U["upsert por lotes de 2000<br/>una transacción por lote"]
  U --> W
  W -->|"terminó"| F["finish_scan: borra gen ≠ actual,<br/>guarda totales, complete"]
  W -->|"cancelado / error"| I["interrupt_scan"]
```

Un escaneo interrumpido **vuelve a empezar de cero** con otra generación: las filas anteriores se conservan hasta el barrido final, de modo que la búsqueda sigue funcionando.

### Consulta

```mermaid
sequenceDiagram
  participant U as Toolbar / filtros
  participant S as useSearch
  participant C as SearchController
  participant R as index_search
  participant I as Indexer
  participant Q as SQLite
  U->>S: setText / patchFilters
  S->>C: setQuery(buildQuery)
  C-->>C: debounce 120 ms
  C->>R: consulta (limit 200)
  R->>R: cancela la bandera anterior
  R->>I: search (spawn_blocking)
  I->>Q: roots() por la conexión ESCRITORA
  I->>Q: query() por la conexión lectora
  Q-->>C: página
  C->>S: snapshot · SearchView virtualiza
  Note over C: Respuestas de consultas viejas se descartan por secuencia
```

### Esquema (`user_version = 1`)

```mermaid
erDiagram
  roots ||--o{ entries : contiene
  roots ||--o{ issues : registra
  roots {
    int id PK
    text path UK
    text state
    int gen
    int online
    int files
    int dirs
    int skipped
    int last_complete
  }
  entries {
    int id PK
    int root_id
    text key UK
    text path
    text name
    text name_lc
    text ext
    int kind
    int size
    int modified
    int created
    int accessed
    int depth
    int gen
  }
  issues {
    int id PK
    int root_id
    text path
    text message
  }
  meta {
    text key PK
    text value
  }
```

`entries_fts`: FTS5 `trigram`, contenido externo sobre `name_lc`, mantenido con 3 disparadores. Índices: `(root_id, gen)`, `(ext, size)`, `(modified)`. La columna `key` es la ruta en minúsculas con `/`; su orden permite resolver un subárbol por rango.

Conexiones: **escritora** (`Mutex<Connection>`, 32 MB de caché, `synchronous=NORMAL`) y **lectora** (solo lectura, 64 MB de caché). Un esquema de otra versión o una base corrupta al abrir se recrea.

## 8. Miniaturas e iconos

```mermaid
flowchart LR
  T["Thumb / PreviewPane"] --> US["useThumb"]
  US --> TS["thumbService TS<br/>4 en vuelo · LRU 400 · época"]
  TS --> GT["get_thumbnail"]
  GT --> SV["ThumbService Rust"]
  SV --> DC{"Caché en disco"}
  DC -- acierto --> R["data: URL"]
  DC -- fallo --> DD["Deduplicación + límite de 3"]
  DD --> P1["Raster"] --> P2["SVG"] --> P3["Shell de Windows<br/>hilo con 8 s de límite"]
  P3 --> R
  E["EntryIcon"] --> IS["iconService TS · lotes 256"] --> GI["get_icons"] --> IR["IconService · por tipo o por archivo"]
```

Al montarse un `FileView` (uno por carpeta, `key={tab.path}`) se abre una **época** nueva: lo pendiente de la carpeta anterior se descarta en ambos lados.

## 8 bis. Visor inteligente

```mermaid
flowchart LR
  SEL["Selección: FileView o SearchView"] --> PP["PreviewPane<br/>panel y vista ampliada"]
  VS["stores/viewer<br/>modo y navegación"] --> PP
  PP --> US["useViewerSource<br/>espera 90 ms"]
  US -->|"viewer_open"| VO["ViewerService.open"]
  VO --> R{"Inspecciona<br/>el archivo"}
  R -->|"nativo, PDF, vídeo"| T["Permiso por archivo"]
  R -->|"TIFF, PSD, EPS, RAW"| I["isolate + turno único"]
  R -->|"HEIC y otros"| TH["ThumbService"]
  I --> C[("caché viewer-v1")]
  TH --> C
  C --> T
  T -->|"token"| PP
  PP --> IV["ImageViewer"] & PV["PdfViewer lazy"] & MV["MediaViewer"]
  IV & MV & PV -->|"http://lfview.localhost/token"| S["serve: re-valida, rangos, nosniff"]
  PV -.-> W["Worker de PDF.js"]
```

- **Permisos:** `Tickets` emite un token por archivo (tope de 64). `serve` vuelve a comprobar tamaño y fecha: si el archivo cambió responde 410. `viewer_close` retira todos.
- **Por qué protocolo propio:** `assetProtocol` exigiría un scope global para un explorador. Con tokens, la webview solo lee lo que el backend autorizó.
- **Rangos:** el protocolo entrega el cuerpo en memoria, así que cada respuesta se limita a 8 MiB. PDF.js usa `PDFDataRangeTransport` para no pedir nunca el archivo completo.
- **Recursos:** cambiar de archivo cancela lo pendiente en backend y frontend. Cada PDF tiene su Worker, que se termina a la fuerza 150 ms después de cerrarlo. Sin visor abierto no queda nada en marcha.
- **Navegación:** `Pane` registra en `useViewer` un contexto que recorre `orderRef` (el orden real de `FileView`, con agrupación) o los resultados de búsqueda, saltando carpetas.

## 9. Errores y permisos

| Situación | Tratamiento |
|---|---|
| Ruta inválida por IPC | `safety::parse_path` rechaza `..`, NUL y relativas |
| Operación sobre el sistema | `check_destructive` bloquea raíces, perfil, `Windows`, `Program Files`, papelera |
| Error en un elemento | Se acumula, sigue con el resto; se informa al final |
| Carpeta sin permiso al indexar | Se registra en `issues`; el indexado continúa |
| Archivo de miniatura dañado | Marcador negativo (5 min fallo, 7 días sin proveedor) |
| Manejador de Windows colgado | Hilo abandonado tras el límite |
| Índice que no abre | App funcional, búsqueda desactivada |
| `prefs.json` corrupto | `.bak` + valores por defecto |
| Error de JS en render | `ErrorBoundary`: `view`/`panel` en `Pane`, `app` en `main.tsx`. Pantalla de recuperación, reintento limitado, detalle en `lfind-errors.log` |
| Fallo al mostrar un archivo en el visor | `ErrorBoundary` `viewer` dentro del panel; la barra del visor sigue disponible |
| Pánico dentro de `diagnostics::isolate` (miniaturas y conversiones del visor) | Fallo de ese archivo; se registra y la app sigue |
| Pánico fuera de una frontera | Se registra y **cierra la app** a propósito (estado no fiable) |
| Falta de memoria, desbordamiento de pila, fallo de código nativo | Cierra la app; no se puede contener dentro del proceso |

## 10. Procesos en segundo plano

| Hilo / tarea | Duración | Prioridad / límite |
|---|---|---|
| `lfind-index` | Por pasada | Fondo (`THREAD_MODE_BACKGROUND_BEGIN`), pausa de 2 ms por lote |
| Ticker del índice | Toda la sesión | Duerme 10 min |
| Pool `spawn_blocking` | Por petición | El de Tauri/Tokio |
| Miniaturas | Por petición | 3 generándose a la vez |
| `nova-shell-thumb`, `lfind-icons` | Por extracción | Con tiempo límite; se abandonan si se cuelgan |
| Worker de orden | Por orden | 15 s; después orden síncrono |
| Sondeo `index_status` | Toda la sesión | 1,5 s / 10 s |

## 11. Compilación para Windows

`npm run tauri build` → `npm run build` (`tsc` + Vite, destino `chrome105`, a `dist/`) → compilación Rust en release (`opt-level=3`, LTO, `codegen-units=1`, `strip`, con desenrollado de pánicos) → NSIS en `src-tauri/target/release/bundle/nsis/`. SQLite se compila dentro del binario, por lo que requiere MSVC. Detalle: [DEVELOPMENT_GUIDE.md](DEVELOPMENT_GUIDE.md).
