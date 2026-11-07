//! Barra de dirección: normaliza y valida lo que el usuario escribe o pega. NO navega (eso lo sigue haciendo la
//! pestaña + `open_dir`); solo contesta «¿esta ruta existe, es una carpeta y se puede abrir?» sin bloquear.
//!
//! `normalize` es PURA (sin disco ni entorno real: las variables llegan por un cierre) y se prueba en cualquier sistema.

use crate::safety;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// Qué se ha escrito, ya validado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// Carpeta existente y legible.
    Dir(String),
    /// Era un archivo: se ofrece su carpeta contenedora.
    File { dir: String, file: String },
}

/// Por qué no se pudo resolver (el texto ya es el mensaje para el usuario).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// `empty` | `invalid` | `notFound` | `denied` | `unreachable` | `timeout`
    pub kind: &'static str,
    pub message: String,
}

fn problem(kind: &'static str, message: String) -> Problem {
    Problem { kind, message }
}

/// Tiempo máximo para comprobar una ruta. Una unidad de red caída puede tardar decenas de segundos en responder.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(6);

fn expand_env(s: &str, get: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) if j > 0 && after[..j].chars().all(|c| c.is_alphanumeric() || c == '_' || c == '(' || c == ')') => match get(&after[..j]) {
                Some(v) => {
                    out.push_str(&v);
                    rest = &after[j + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            },
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Deja la ruta lista para el sistema: quita comillas y espacios, expande `%VARIABLES%`, unifica las barras, completa
/// `C:` → `C:\`, quita la barra final y entiende rutas de red (`\\servidor\recurso`, `\\?\UNC\…`). Solo rutas absolutas.
pub fn normalize(input: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<String, Problem> {
    let mut s = input.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_string();
    if s.is_empty() {
        return Err(problem("empty", "Escribe una ruta.".into()));
    }
    if s.contains('\0') || s.chars().any(|c| c.is_control()) {
        return Err(problem("invalid", "La ruta contiene caracteres no válidos.".into()));
    }
    if s.contains('%') {
        s = expand_env(&s, env);
    }
    // Prefijos «verbatim» de Windows.
    if let Some(r) = s.strip_prefix(r"\\?\UNC\").or_else(|| s.strip_prefix(r"//?/UNC/")) {
        s = format!(r"\\{r}");
    } else if let Some(r) = s.strip_prefix(r"\\?\").or_else(|| s.strip_prefix("//?/")) {
        s = r.to_string();
    }
    let windows = is_drive(&s) || s.starts_with(r"\\") || s.starts_with("//") || s.contains('\\');
    if windows {
        let unc = s.starts_with(r"\\") || s.starts_with("//");
        let body = s.replace('/', r"\");
        let parts: Vec<&str> = body.split('\\').filter(|p| !p.is_empty()).collect();
        if unc {
            if parts.len() < 2 {
                return Err(problem("invalid", r"Falta el recurso compartido: escribe \\servidor\recurso.".into()));
            }
            return Ok(format!(r"\\{}", parts.join(r"\")));
        }
        let Some(first) = parts.first().filter(|p| is_drive(p) && p.len() == 2) else {
            return Err(problem("invalid", r"Escribe una ruta completa, por ejemplo C:\Users\Usuario\Downloads.".into()));
        };
        let letter = first.to_ascii_uppercase();
        return Ok(if parts.len() == 1 { format!(r"{letter}\") } else { format!(r"{letter}\{}", parts[1..].join(r"\")) });
    }
    if !s.starts_with('/') {
        return Err(problem("invalid", r"Escribe una ruta completa, por ejemplo C:\Users\Usuario\Downloads.".into()));
    }
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    Ok(format!("/{}", parts.join("/")))
}

fn io_problem(p: &str, e: &std::io::Error) -> Problem {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => problem("notFound", format!("No se encuentra «{p}». Comprueba que la ruta es correcta.")),
        PermissionDenied => problem("denied", format!("Acceso denegado a «{p}».")),
        _ => problem("unreachable", format!("No se pudo acceder a «{p}»: {e}")),
    }
}

/// Comprobación síncrona (puede tardar con rutas de red): existe, y si es carpeta se puede abrir.
pub fn check(path: &str) -> Result<Resolved, Problem> {
    let p = safety::parse_path(path).map_err(|m| problem("invalid", m))?;
    let m = std::fs::metadata(&p).map_err(|e| io_problem(path, &e))?;
    if m.is_dir() {
        std::fs::read_dir(&p).map_err(|e| io_problem(path, &e))?; // solo abre el listado: comprueba el permiso sin leerlo
        return Ok(Resolved::Dir(path.to_string()));
    }
    let dir = Path::new(path).parent().map(|d| d.display().to_string()).filter(|d| !d.is_empty()).ok_or_else(|| problem("invalid", format!("«{path}» no es una carpeta.")))?;
    let file = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Resolved::File { dir, file })
}

/// `normalize` + `check` con tiempo límite: la comprobación corre en un hilo aparte y, si la ruta no responde a tiempo
/// (típico de una unidad de red caída), se abandona sin bloquear a quien llama.
pub fn resolve(input: &str, env: &dyn Fn(&str) -> Option<String>, timeout: Duration) -> Result<Resolved, Problem> {
    let path = normalize(input, env)?;
    let (tx, rx) = mpsc::channel();
    let p = path.clone();
    std::thread::spawn(move || {
        let _ = tx.send(check(&p));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => r,
        Err(_) => Err(problem("timeout", format!("«{path}» no respondió a tiempo. Si es una ruta de red, comprueba la conexión."))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(k: &str) -> Option<String> {
        match k {
            "USERPROFILE" => Some(r"C:\Users\Ana".into()),
            "SystemRoot" => Some(r"C:\Windows".into()),
            _ => None,
        }
    }
    fn n(s: &str) -> Result<String, String> {
        normalize(s, &env).map_err(|p| p.kind.to_string())
    }

    #[test]
    fn rutas_de_windows() {
        assert_eq!(n(r"C:\Users\Usuario\Downloads").unwrap(), r"C:\Users\Usuario\Downloads");
        assert_eq!(n(r"  c:/Users//Usuario/Downloads/  ").unwrap(), r"C:\Users\Usuario\Downloads");
        assert_eq!(n(r#""D:\Mis documentos\""#).unwrap(), r"D:\Mis documentos");
        assert_eq!(n("c:").unwrap(), r"C:\");
        assert_eq!(n(r"c:\").unwrap(), r"C:\");
        assert_eq!(n(r"\\?\C:\Windows").unwrap(), r"C:\Windows");
    }
    #[test]
    fn variables_de_entorno() {
        assert_eq!(n(r"%USERPROFILE%\Downloads").unwrap(), r"C:\Users\Ana\Downloads");
        assert_eq!(n(r"%SystemRoot%").unwrap(), r"C:\Windows");
        assert_eq!(n(r"C:\50%\x").unwrap(), r"C:\50%\x", "un % suelto no es una variable");
        assert_eq!(n(r"%NOEXISTE%\x").unwrap_err(), "invalid", "variable sin definir: queda como texto y no es una ruta absoluta");
    }
    #[test]
    fn rutas_de_red() {
        assert_eq!(n(r"\\servidor\recurso\carpeta\").unwrap(), r"\\servidor\recurso\carpeta");
        assert_eq!(n("//servidor/recurso").unwrap(), r"\\servidor\recurso");
        assert_eq!(n(r"\\?\UNC\srv\share\x").unwrap(), r"\\srv\share\x");
        assert_eq!(n(r"\\servidor").unwrap_err(), "invalid", "falta el recurso compartido");
    }
    #[test]
    fn rechaza_lo_que_no_es_absoluto_o_es_raro() {
        for bad in ["", "   ", "Downloads", r"Users\Ana", r"C:Users", "..", "x\0y"] {
            assert!(normalize(bad, &env).is_err(), "{bad:?}");
        }
        assert_eq!(n("").unwrap_err(), "empty");
    }
    #[test]
    fn rutas_posix() {
        assert_eq!(n("/usr//lib/").unwrap(), "/usr/lib");
        assert_eq!(n("/").unwrap(), "/");
    }

    #[cfg(unix)]
    mod disco {
        use super::*;
        fn tmp(n: &str) -> std::path::PathBuf {
            let d = std::env::temp_dir().join(format!("nova-pb-{n}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            d
        }
        fn r(s: &str) -> Result<Resolved, Problem> {
            resolve(s, &env, Duration::from_secs(2))
        }
        #[test]
        fn carpeta_archivo_y_errores() {
            let d = tmp("a");
            std::fs::create_dir_all(d.join("sub/anidada")).unwrap();
            std::fs::write(d.join("f.txt"), b"x").unwrap();
            let s = d.display().to_string();
            assert_eq!(r(&format!("{s}/sub/anidada/")).unwrap(), Resolved::Dir(format!("{s}/sub/anidada")));
            assert_eq!(r(&format!("{s}/f.txt")).unwrap(), Resolved::File { dir: s.clone(), file: "f.txt".into() });
            let e = r(&format!("{s}/no-existe")).unwrap_err();
            assert_eq!(e.kind, "notFound");
            assert!(e.message.contains("no-existe"), "el mensaje nombra la ruta: {}", e.message);
            assert_eq!(r("relativa/x").unwrap_err().kind, "invalid");
            let _ = std::fs::remove_dir_all(d);
        }
        #[test]
        fn sin_permiso_de_lectura() {
            use std::os::unix::fs::PermissionsExt;
            let d = tmp("b");
            let p = d.join("cerrada");
            std::fs::create_dir_all(&p).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
            let denied = std::fs::read_dir(&p).is_err(); // como root el permiso no se aplica
            let res = r(&p.display().to_string());
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            if denied {
                assert_eq!(res.unwrap_err().kind, "denied");
            }
            let _ = std::fs::remove_dir_all(d);
        }
        #[test]
        fn el_tiempo_limite_no_bloquea() {
            // `resolve` con 0 ms vuelve enseguida con «timeout» o con el resultado, nunca se queda esperando.
            let t = std::time::Instant::now();
            let _ = resolve("/", &env, Duration::from_millis(0));
            assert!(t.elapsed() < Duration::from_millis(500));
        }
    }
}
