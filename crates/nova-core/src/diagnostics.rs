//! Diagnóstico de fallos: registro local persistente y frontera de pánicos.
//!
//! Estrategia de pánicos de LFind:
//!  * **Por defecto, fallar rápido.** Un pánico fuera de una frontera se registra y termina el proceso (`abort`): el estado
//!    podría ser incoherente y continuar a medias sería peor (un hilo muerto con cerrojos envenenados aparentaría funcionar).
//!  * **Fronteras explícitas (`isolate`)** para código que procesa CONTENIDO de archivos ajenos: un archivo malformado que haga
//!    entrar en pánico a un decodificador debe producir un error de ESE archivo, no cerrar la aplicación.
//!
//! Por eso la compilación usa `panic = "unwind"` (sin él `catch_unwind` no hace nada) y el gancho aborta todo lo que no esté
//! dentro de una frontera: fuera de ellas el comportamiento es el mismo que con `panic = "abort"`, pero queda registrado.
//!
//! Lo que `isolate` NO puede contener: falta de memoria, desbordamiento de pila y fallos de código nativo (por ejemplo un
//! manejador de miniaturas de terceros cargado en el proceso). Para eso la única protección real es un proceso aparte.

use serde_json::{json, Value};
use std::cell::Cell;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

// Sin desenrollado, `isolate` sería una falsa protección: se prefiere que no compile.
const _: () = assert!(cfg!(panic = "unwind"), "LFind necesita panic = \"unwind\": ver el comentario de diagnostics.rs");

/// Tamaño a partir del cual el registro se rota (se conserva un único archivo anterior).
pub const MAX_LOG_BYTES: u64 = 512 * 1024;
/// Longitud máxima de cada campo: lo que llega de la interfaz no es de fiar y el registro debe seguir siendo pequeño.
pub const MAX_FIELD_CHARS: usize = 8 * 1024;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Fija dónde se escribe el registro. Solo tiene efecto la primera vez.
pub fn set_log_path(path: PathBuf) {
    let _ = LOG_PATH.set(path);
}

/// Recorta por caracteres (nunca parte uno por la mitad).
pub fn clip(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// Añade una línea JSON al registro `path`, rotándolo si ha crecido demasiado.
pub fn append(path: &Path, entry: &Value) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if fs::metadata(path).map_or(false, |m| m.len() >= MAX_LOG_BYTES) {
        let _ = fs::rename(path, path.with_extension("log.1"));
    }
    let mut line = entry.to_string();
    line.push('\n');
    OpenOptions::new().create(true).append(true).open(path)?.write_all(line.as_bytes())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Registra un error de la interfaz (`ErrorBoundary`). Sin ruta configurada no hace nada.
pub fn record_ui_error(scope: &str, message: &str, stack: Option<&str>) -> io::Result<()> {
    let Some(path) = LOG_PATH.get() else { return Ok(()) };
    append(
        path,
        &json!({
            "ts": now_ms(),
            "kind": "ui",
            "scope": clip(scope, 32),
            "message": clip(message, MAX_FIELD_CHARS),
            "stack": stack.map(|s| clip(s, MAX_FIELD_CHARS)),
        }),
    )
}

fn payload_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "pánico sin mensaje".into())
}

/// Instala el gancho global: registra cada pánico y aborta si no está dentro de `isolate`. Se llama una vez, al arrancar la aplicación.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let contained = DEPTH.with(|d| d.get()) > 0;
        if let Some(path) = LOG_PATH.get() {
            // Si escribir falla no hay a quién avisar: lo importante es no provocar un segundo pánico.
            let _ = append(
                path,
                &json!({
                    "ts": now_ms(),
                    "kind": "panic",
                    "contained": contained,
                    "thread": std::thread::current().name().unwrap_or("?"),
                    "location": info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())),
                    "message": clip(&payload_text(info.payload()), MAX_FIELD_CHARS),
                }),
            );
        }
        previous(info);
        if !contained {
            std::process::abort();
        }
    }));
}

/// El código aislado entró en pánico. El motivo técnico ya está en el registro; este valor no se muestra al usuario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contained {
    pub message: String,
}

struct Scope;

impl Scope {
    fn enter() -> Scope {
        DEPTH.with(|d| d.set(d.get() + 1));
        Scope
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get() - 1));
    }
}

/// Ejecuta `f` convirtiendo un pánico en `Err`. Usar SOLO alrededor de código que procesa datos de archivos y no toca estado
/// compartido: lo que `f` hubiera dejado a medias se descarta con ella.
pub fn isolate<T>(f: impl FnOnce() -> T) -> Result<T, Contained> {
    let _scope = Scope::enter();
    catch_unwind(AssertUnwindSafe(f)).map_err(|p| Contained { message: payload_text(&*p) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nova-diag-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d.join("logs").join("x.log")
    }

    #[test]
    fn isolate_passes_results_through_and_contains_panics() {
        assert_eq!(isolate(|| 7), Ok(7));
        assert_eq!(isolate(|| -> u8 { panic!("archivo malformado") }), Err(Contained { message: "archivo malformado".into() }));
        assert_eq!(isolate(|| -> u8 { panic!("n = {}", 3) }).unwrap_err().message, "n = 3");
    }

    #[test]
    fn isolate_scope_is_balanced_after_a_panic() {
        let _ = isolate(|| isolate(|| -> u8 { panic!("dentro") }));
        assert_eq!(DEPTH.with(|d| d.get()), 0, "tras contener un pánico, el hilo vuelve a estar fuera de la frontera");
    }

    #[test]
    fn clip_respects_char_boundaries() {
        assert_eq!(clip("ñññ", 2), "ññ…");
        assert_eq!(clip("ab", 2), "ab");
    }

    #[test]
    fn append_creates_folders_writes_lines_and_rotates() {
        let p = tmp("rotate");
        append(&p, &json!({"a": "línea\ncon salto"})).unwrap();
        append(&p, &json!({"a": 2})).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert_eq!(text.lines().count(), 2, "un salto dentro de un campo no parte la entrada");

        fs::write(&p, vec![b'x'; MAX_LOG_BYTES as usize]).unwrap();
        append(&p, &json!({"a": 3})).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap().lines().count(), 1);
        assert_eq!(fs::metadata(p.with_extension("log.1")).unwrap().len(), MAX_LOG_BYTES);
        let _ = fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }
}
