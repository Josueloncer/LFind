# UPDATE08_PROGRESS · Actualizaciones automáticas

Base: commit `98c1b10`. Sin commits, sin push, sin releases reales. Dependencia nueva: `tauri-plugin-updater` 2.14.1 (oficial).

## Cómo funciona

| Paso | Qué ocurre |
|---|---|
| Comprobar | `update_check` lee `latest.json` de la última GitHub Release. Solo ofrece versiones superiores a la instalada |
| Descargar | `update_download` con progreso real por `Channel`; se puede cancelar. El plugin verifica la firma al terminar |
| Listo | El paquete verificado queda en memoria: «Actualización lista. Reiniciar para aplicar» |
| Reiniciar | `update_restart` vuelve a verificar y lanza el instalador NSIS en modo pasivo; LFind se cierra y el instalador la abre de nuevo |

**Dónde está:** última fila de la barra lateral, «Buscar actualizaciones». Abre `UpdateDialog`. Un punto indica que hay una versión nueva.

**Comprobación automática:** 8 s después de arrancar, como mucho una vez cada 6 h, sin descargar nada. Solo avisa con un mensaje discreto y un punto. Un fallo es silencioso. Se desactiva con la casilla del diálogo (`autoUpdateCheck`).

**Datos del usuario:** las preferencias y el índice viven en `%APPDATA%` y `%LOCALAPPDATA%\app.nova.explorer`, fuera de la carpeta de instalación. El actualizador no los toca.

## Firma

- Claves generadas con `tauri signer generate`. La pública está en `tauri.conf.json`; la privada y su contraseña, en `C:\Users\josue\.tauri\`, **fuera del repositorio**.
- Doble verificación en cliente (al descargar y al instalar) y `requireSignedVersion`: la versión anunciada debe coincidir con la firmada.
- `scripts/release/lib.mjs` repite la verificación en el workflow con la clave pública de la app: si la clave de CI no correspondiera, no se publica nada.
- **Si se pierde la clave privada, las instalaciones existentes ya no podrán actualizarse.** Guarda una copia de seguridad cifrada.

## Versionado

`MAYOR.MENOR.<GITHUB_RUN_NUMBER>`, con `MAYOR.MENOR` de `tauri.conf.json` (hoy `0.1`), inyectado al compilar con `tauri build --config`.

- El número de ejecución crece siempre y no vive en ningún archivo: no hay commits automáticos ni ciclos.
- La primera publicación es `0.1.<n>`, superior a `0.1.0`.
- Salvaguarda: si la versión calculada no supera a la última publicada, el workflow falla.
- Un reintento de la misma ejecución da la misma versión: si ya estaba publicada, falla sin tocarla.
- Para un salto mayor, cambia `MAYOR.MENOR` a mano en `tauri.conf.json`. Nunca lo bajes.
- Si se borrara y recreara el workflow, el número se reiniciaría: la salvaguarda lo detendría. Sube entonces `MENOR`.

## Publicación (`.github/workflows/release.yml`)

Push a `main` (se ignoran cambios solo de `.md`, `docs/` y `.gitignore`) → comprueba secrets → tipos → frontend → Rust → versión → compila y firma → verifica firma y artefactos → release en **borrador** con 3 archivos → publica.

El manifiesto solo cuenta en `releases/latest` cuando el último paso publica la release. Un fallo antes deja, como mucho, un borrador invisible para los usuarios.

Permisos: `contents: read` global y `contents: write` solo en este trabajo. Acciones fijadas por SHA. Una publicación a la vez. El token de escritura solo está en los pasos que lo usan, una ejecución manual desde otra rama no publica, y los saltos de línea de los Secrets se eliminan antes de firmar.

**Sin ciclos:** el workflow solo se activa con pushes a `main` y nunca hace commits ni pushes; publicar una release no es un push a una rama.

**Comprobación previa a la primera publicación:** repositorio público y endpoint correcto; clave pública y privada coherentes (firma y verificación del instalador real); manifiesto con URL y firma correctas; pruebas y tipos en una copia con CRLF, como el checkout del runner; workflow validado contra su esquema. No se pueden comprobar desde aquí los nombres ni los valores de los Secrets en GitHub: el primer paso del workflow falla si faltan, y el de verificación si no corresponden a la clave pública.

## Primera versión

Las instalaciones actuales (Update 07 y anteriores) **no incluyen el actualizador**: hay que instalar a mano una vez la primera versión que lo trae. Se instala encima conservando datos. A partir de ahí, todo es automático.

## Pruebas

**Ejecutadas de verdad**, con una instalación aislada (`LFindUpdateTest`, identificador, ejecutable y datos propios), servidor local, clave de prueba y el plugin real:

| Escenario | Resultado |
|---|---|
| Sin novedades / versión inferior publicada | ✅ «Tienes la última versión» |
| Actualización disponible, con versiones, tamaño y notas (sin HTML) | ✅ |
| Firma de otra clave | ✅ Rechazada; no permite reiniciar |
| Paquete corrupto | ✅ Rechazado |
| Sin conexión, manifiesto 404 o inválido | ✅ Mensaje claro; LFind sigue funcionando |
| Descarga con 404 o cortada a mitad | ✅ Error y reintento correcto |
| Progreso real (0, 8, 11, 16… %) | ✅ |
| Cancelación | ✅ Vuelve a poder actualizar |
| Aviso automático | ✅ 1 consulta, 0 descargas |
| Reinicio dentro de las 6 h / desactivado | ✅ 0 consultas |
| **Actualización real 0.1.1 → 0.1.2 con instalador y reinicio** | ✅ |
| Preferencias tras actualizar | ✅ Tema, anchura y recientes iguales |
| Índice SQLite tras actualizar | ✅ Mismo archivo, ajustes intactos |

Rust 164/164 · Vitest 239/239 · tipos sin errores · clave real comprobada firmando y verificando un archivo · órdenes de versión y manifiesto · YAML del workflow válido.

**Requieren GitHub y no se han ejecutado:** el workflow completo, una release real, la descarga desde GitHub, HTTPS real, y la actualización de una instalación real.

**No probado:** sin conexión durante la instalación, y actualizar con el instalador cerrado por el antivirus.

## Recuperación ante errores

| Caso | Efecto |
|---|---|
| El workflow falla | Sin release; las instalaciones no ven nada |
| Queda un borrador | Invisible; el siguiente intento de esa versión lo sustituye |
| Se publicó una versión mala | Publica una corrección: tendrá número superior. No borres releases ya descargadas |
| Fallo en cliente | LFind sigue con su versión actual |
