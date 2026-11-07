//! «Enviar a»: lista la carpeta `%APPDATA%\Microsoft\Windows\SendTo` y lanza el destino elegido. Sin IContextMenu ni COM.
//! Los accesos `.lnk` se resuelven UNA vez (en segundo plano, al arrancar) con un único PowerShell; abrir el menú no resuelve nada.
//! Se invoca por IDENTIFICADOR (nombre del archivo en SendTo); jamás se ejecuta una ruta que llegue de la interfaz.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Programa + argumentos fijos del acceso directo; los archivos se añaden al final.
    App { exe: PathBuf, args: String },
    /// Carpeta: la interfaz inicia una copia hacia ella.
    Folder(PathBuf),
    /// «Carpeta comprimida»: la interfaz comprime con `archive`.
    Zip,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    pub label: String,
    pub kind: &'static str, // "app" | "folder" | "zip"
}

pub fn sendto_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| Path::new(&a).join("Microsoft").join("Windows").join("SendTo"))
}

/// Texto visible: el nombre sin extensión ni el sufijo « (crear acceso directo)» que Windows añade a algunos destinos.
pub fn label_of(file: &str) -> String {
    let stem = file.rfind('.').map_or(file, |i| &file[..i]);
    stem.replace(" (crear acceso directo)", "").trim().to_string()
}

/// Salida del script de resolución: una línea por acceso, `archivo<TAB>destino<TAB>argumentos`.
pub fn parse_resolved(out: &str) -> Vec<(String, String, String)> {
    out.lines()
        .filter_map(|l| {
            let mut p = l.trim_end_matches('\r').splitn(3, '\t');
            let f = p.next()?.trim();
            let t = p.next()?.trim();
            (!f.is_empty() && !t.is_empty()).then(|| (f.to_string(), t.to_string(), p.next().unwrap_or("").trim().to_string()))
        })
        .collect()
}

/// Clasifica los archivos de SendTo ya resueltos. `files`: nombres presentes; `resolved`: salida de `parse_resolved`;
/// `is_dir`: ¿es carpeta este destino? Los tipos que requieren COM (`.MAPIMail`, `.DeskLink`, `.mydocs`) se omiten.
pub fn classify(files: &[String], resolved: &[(String, String, String)], is_dir: &dyn Fn(&Path) -> bool) -> (Vec<Item>, HashMap<String, Target>) {
    let mut items = Vec::new();
    let mut map = HashMap::new();
    for f in files {
        let ext = f.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let target = match ext.as_str() {
            "zfsendtotarget" => Some(Target::Zip),
            "lnk" => resolved.iter().find(|r| &r.0 == f).map(|(_, t, a)| {
                let p = PathBuf::from(t);
                if is_dir(&p) { Target::Folder(p) } else { Target::App { exe: p, args: a.clone() } }
            }),
            _ => None,
        };
        if let Some(t) = target {
            let kind = match t { Target::App { .. } => "app", Target::Folder(_) => "folder", Target::Zip => "zip" };
            items.push(Item { id: f.clone(), label: label_of(f), kind });
            map.insert(f.clone(), t);
        }
    }
    items.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    (items, map)
}

/// Comillas al estilo de `CommandLineToArgvW` para una ruta.
pub fn quote_arg(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}

/// Línea de argumentos final: los fijos del acceso directo + cada ruta entre comillas.
pub fn command_line(args: &str, paths: &[String]) -> String {
    let mut s = args.trim().to_string();
    for p in paths {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(&quote_arg(p));
    }
    s
}

/// Lee SendTo y resuelve los `.lnk`. Se llama una vez, fuera del hilo de la interfaz.
pub fn list() -> io::Result<(Vec<Item>, HashMap<String, Target>)> {
    let dir = match sendto_dir() {
        Some(d) if d.is_dir() => d,
        _ => return Ok((vec![], HashMap::new())),
    };
    let files: Vec<String> = std::fs::read_dir(&dir)?.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).filter(|n| !n.eq_ignore_ascii_case("desktop.ini")).collect();
    let resolved = if files.iter().any(|f| f.to_ascii_lowercase().ends_with(".lnk")) { resolve_links(&dir)? } else { vec![] };
    Ok(classify(&files, &resolved, &|p| p.is_dir()))
}

#[cfg(windows)]
fn resolve_links(dir: &Path) -> io::Result<Vec<(String, String, String)>> {
    use std::os::windows::process::CommandExt;
    let script = "$sh=New-Object -ComObject WScript.Shell;Get-ChildItem -LiteralPath $env:NOVA_SENDTO -Filter *.lnk|ForEach-Object{$l=$sh.CreateShortcut($_.FullName);$_.Name+\"`t\"+$l.TargetPath+\"`t\"+$l.Arguments}";
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", script])
        .env("NOVA_SENDTO", dir)
        .creation_flags(0x0800_0000)
        .output()?;
    Ok(parse_resolved(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(not(windows))]
fn resolve_links(_dir: &Path) -> io::Result<Vec<(String, String, String)>> {
    Ok(vec![])
}

/// Lanza un destino de tipo programa con los archivos como argumentos.
pub fn launch(exe: &Path, args: &str, paths: &[String]) -> io::Result<()> {
    let line = command_line(args, paths);
    if line.len() > 30_000 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Demasiados elementos para enviar a esta aplicación de una vez."));
    }
    let mut c = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.raw_arg(&line);
    }
    #[cfg(not(windows))]
    c.args(line.split_whitespace());
    if let Some(d) = exe.parent().filter(|d| d.is_dir()) {
        c.current_dir(d);
    }
    c.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etiquetas() {
        assert_eq!(label_of("Escritorio (crear acceso directo).lnk"), "Escritorio");
        assert_eq!(label_of("Documentos.mydocs"), "Documentos");
        assert_eq!(label_of("Carpeta comprimida.ZFSendToTarget"), "Carpeta comprimida");
    }

    #[test]
    fn clasifica_y_ordena() {
        let out = "Notepad++.lnk\tC:\\np\\n.exe\t-multiInst\r\nDocumentos.lnk\tC:\\Users\\a\\Documents\t\r\nroto.lnk\t\t\r\n";
        let r = parse_resolved(out);
        assert_eq!(r.len(), 2);
        let files: Vec<String> = ["Notepad++.lnk", "Documentos.lnk", "roto.lnk", "x.ZFSendToTarget", "Correo.MAPIMail", "otro.txt"].map(String::from).to_vec();
        let (items, map) = classify(&files, &r, &|p| p.to_string_lossy().ends_with("Documents"));
        let l: Vec<(&str, &str)> = items.iter().map(|i| (i.label.as_str(), i.kind)).collect();
        assert_eq!(l, [("Documentos", "folder"), ("Notepad++", "app"), ("x", "zip")]);
        assert_eq!(map["Notepad++.lnk"], Target::App { exe: PathBuf::from("C:\\np\\n.exe"), args: "-multiInst".into() });
        assert!(!map.contains_key("roto.lnk"));
    }

    #[test]
    fn linea_de_comandos() {
        assert_eq!(command_line("-a", &["C:\\x y\\f.txt".into(), "C:\\q\"w".into()]), "-a \"C:\\x y\\f.txt\" \"C:\\q\\\"w\"");
        assert_eq!(command_line("", &["a".into()]), "\"a\"");
    }

    #[test]
    fn rechaza_demasiados_elementos() {
        let paths: Vec<String> = (0..3000).map(|i| format!("C:\\carpeta\\archivo{i}.txt")).collect();
        assert!(launch(Path::new("x"), "", &paths).is_err());
    }
}
