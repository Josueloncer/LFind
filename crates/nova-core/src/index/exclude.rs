//! Reglas de exclusión del indexado (configurables por el usuario).

use super::classify::path_key;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExcludeRules {
    /// Carpetas completas (ruta absoluta): ni ellas ni sus descendientes se indexan.
    pub paths: Vec<String>,
    /// Nombres de carpeta excluidos en cualquier nivel (sin distinguir mayúsculas).
    pub names: Vec<String>,
    /// Nombres de carpeta excluidos solo en la raíz de la unidad (p. ej. `Windows`).
    pub root_dirs: Vec<String>,
    /// Extensiones excluidas (sin punto).
    pub exts: Vec<String>,
}

impl ExcludeRules {
    pub fn new(paths: Vec<String>, names: Vec<String>, root_dirs: Vec<String>, exts: Vec<String>) -> Self {
        let lc = |v: Vec<String>| v.into_iter().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect::<Vec<_>>();
        ExcludeRules {
            paths: paths.iter().map(|p| path_key(p.trim())).filter(|p| !p.is_empty()).collect(),
            names: lc(names),
            root_dirs: lc(root_dirs),
            exts: lc(exts).into_iter().map(|e| e.trim_start_matches('.').to_string()).collect(),
        }
    }

    /// Valores por defecto: carpetas del sistema, papelera y recuperación, que no aportan al buscar archivos propios.
    pub fn windows_defaults() -> Self {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        ExcludeRules {
            paths: vec![],
            names: s(&["$recycle.bin", "system volume information", "$winreagent", "node_modules"]),
            root_dirs: s(&["windows", "programdata", "recovery", "$windows.~bt", "$windows.~ws", "windows.old", "config.msi", "msocache"]),
            exts: vec![],
        }
    }

    /// ¿Se omite esta carpeta (y todo lo que contiene)? `depth` = 1 para hijos directos de la raíz.
    pub fn dir_excluded(&self, key: &str, name_lc: &str, depth: u32) -> bool {
        if self.names.iter().any(|n| n == name_lc) {
            return true;
        }
        if depth == 1 && self.root_dirs.iter().any(|n| n == name_lc) {
            return true;
        }
        self.paths.iter().any(|p| key == p || (key.len() > p.len() && key.starts_with(p.as_str()) && key.as_bytes()[p.len()] == b'/'))
    }

    pub fn ext_excluded(&self, ext: &str) -> bool {
        !ext.is_empty() && self.exts.iter().any(|e| e == ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules() {
        let r = ExcludeRules::new(vec!["D:\\Privado\\".into()], vec!["Node_Modules".into()], vec!["Windows".into()], vec![".TMP".into()]);
        assert!(r.dir_excluded("c:/windows", "windows", 1));
        assert!(!r.dir_excluded("c:/users/windows", "windows", 2), "root_dirs solo aplica en la raíz");
        assert!(r.dir_excluded("c:/p/node_modules", "node_modules", 2));
        assert!(r.dir_excluded("d:/privado", "privado", 1));
        assert!(r.dir_excluded("d:/privado/sub", "sub", 2));
        assert!(!r.dir_excluded("d:/privado2", "privado2", 1), "prefijo de texto no es prefijo de carpeta");
        assert!(r.ext_excluded("tmp") && !r.ext_excluded("txt") && !r.ext_excluded(""));
    }
}
