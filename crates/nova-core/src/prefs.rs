//! Preferencias persistentes en JSON, con escritura atómica y recuperación ante archivo corrupto.

use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
    Cyberpunk,
    Hacker,
    Minimal,
    /// Sigue al sistema. También recoge cualquier valor desconocido (p. ej. un tema de una versión
    /// futura): así un `prefs.json` más nuevo no hace fallar la carga ni pierde el resto de ajustes.
    #[default]
    #[serde(other)]
    System,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    #[default]
    Details,
    List,
    Tiles,
    Content,
    #[serde(alias = "grid")]
    Icons,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortKey {
    #[default]
    Name,
    Modified,
    Size,
    Kind,
    Created,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum GroupBy {
    #[default]
    None,
    Type,
    Modified,
    Created,
    Size,
    Name,
}

/// Columnas opcionales de la vista de detalles (el nombre siempre se muestra).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Column {
    Modified,
    Created,
    Type,
    Size,
}

pub const MAX_RECENTS: usize = 15;
pub const MAX_FOLDER_VIEWS: usize = 300;
pub const MAX_FAVORITES: usize = 100;
pub const CURRENT_VERSION: u32 = 2;

/// Vista recordada para una carpeta concreta.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    pub path: String,
    pub view_mode: ViewMode,
    pub icon_size: u16,
    pub sort_key: SortKey,
    pub sort_dir: SortDir,
    pub group_by: GroupBy,
    /// `true` cuando el usuario pidió explícitamente recordar la vista (modo e icono) de esta carpeta.
    /// Las entradas anteriores a la actualización 03 no lo tienen (`false`): solo conservan el orden/agrupación
    /// y el modo/tamaño se hereda de la vista global.
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Favorite {
    pub path: String,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub theme: Theme,
    pub view_mode: ViewMode,
    pub show_hidden: bool,
    pub sort_key: SortKey,
    pub sort_dir: SortDir,
    pub sidebar_collapsed: bool,
    pub favorites: Vec<Favorite>,
    pub icon_size: u16,
    pub group_by: GroupBy,
    pub columns: Vec<Column>,
    pub recents: Vec<String>,
    pub folder_views: Vec<FolderView>,
    /// Mostrar miniaturas reales de imágenes, PDF, etc. (se pueden desactivar en discos lentos).
    pub thumbnails: bool,
    /// Mostrar los iconos del sistema (los del Explorador de Windows) en lugar de los glifos propios de LFind.
    pub system_icons: bool,
    /// Panel de vista previa a la derecha de la lista.
    pub preview_pane: bool,
    /// Anchura del panel de vista previa, en píxeles.
    pub preview_width: u16,
    /// Comprobar en silencio si hay una versión nueva al iniciar.
    pub auto_update_check: bool,
    /// Última comprobación automática (milisegundos epoch), para no repetirla en cada arranque.
    pub last_update_check: i64,
    /// Versión del formato; permite migrar archivos antiguos. Si falta, es 0.
    #[serde(default = "zero")]
    pub version: u32,
}

fn zero() -> u32 {
    0
}

impl Prefs {
    /// Limita tamaños y valores: lo que llega de la interfaz no es de fiar.
    pub fn sanitize(&mut self) {
        self.icon_size = self.icon_size.clamp(16, 512);
        self.preview_width = self.preview_width.clamp(220, 900);
        self.recents.truncate(MAX_RECENTS);
        self.favorites.truncate(MAX_FAVORITES);
        let excess = self.folder_views.len().saturating_sub(MAX_FOLDER_VIEWS);
        self.folder_views.drain(..excess); // se descartan las más antiguas
        for f in &mut self.folder_views {
            f.icon_size = f.icon_size.clamp(16, 512);
        }
        let mut seen = vec![];
        self.columns.retain(|c| !seen.contains(c) && { seen.push(*c); true });
    }

    fn migrate(&mut self) {
        if self.version < 2 {
            // Antes "list" era la vista de detalles y "grid" la de iconos.
            if self.view_mode == ViewMode::List {
                self.view_mode = ViewMode::Details;
            }
            self.version = CURRENT_VERSION;
        }
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            view_mode: ViewMode::Details,
            show_hidden: false,
            sort_key: SortKey::Name,
            sort_dir: SortDir::Asc,
            sidebar_collapsed: false,
            favorites: vec![],
            icon_size: 64,
            group_by: GroupBy::None,
            columns: vec![Column::Modified, Column::Type, Column::Size],
            recents: vec![],
            folder_views: vec![],
            thumbnails: true,
            system_icons: true,
            preview_pane: false,
            preview_width: 320,
            auto_update_check: true,
            last_update_check: 0,
            version: CURRENT_VERSION,
        }
    }
}

/// Carga las preferencias. Si no existen → valores por defecto.
/// Si el JSON está corrupto, se conserva como `.bak` y se devuelven los valores por defecto.
pub fn load(path: &Path) -> Prefs {
    match fs::read_to_string(path) {
        Ok(s) => match serde_json::from_str::<Prefs>(&s) {
            Ok(mut p) => {
                p.migrate();
                p.sanitize();
                p
            }
            Err(_) => {
                let _ = fs::rename(path, path.with_extension("json.bak"));
                Prefs::default()
            }
        },
        Err(_) => Prefs::default(),
    }
}

/// Guarda escribiendo a un temporal y renombrando, para no dejar un JSON a medias.
pub fn save(path: &Path, prefs: &Prefs) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut prefs = prefs.clone();
    prefs.sanitize();
    prefs.version = CURRENT_VERSION;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&prefs).map_err(io::Error::other)?)?;
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(n: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("nova-prefs-{}-{}", n, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn preferences_from_a_version_without_updater_keep_working() {
        let d = dir("preupdate");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("prefs.json");
        fs::write(&f, r#"{"theme":"hacker","iconSize":96,"previewPane":true,"version":2}"#).unwrap();
        let p = load(&f);
        assert_eq!((p.theme, p.icon_size, p.preview_pane), (Theme::Hacker, 96, true), "se conserva lo que había");
        assert!(p.auto_update_check, "la comprobación automática viene activada");
        assert_eq!(p.last_update_check, 0);
    }

    #[test]
    fn roundtrip() {
        let d = dir("rt");
        let f = d.join("prefs.json");
        let p = Prefs {
            theme: Theme::Dark,
            view_mode: ViewMode::Icons,
            favorites: vec![Favorite { path: "C:\\Proyectos".into(), label: "Proyectos".into() }],
            ..Prefs::default()
        };
        save(&f, &p).unwrap();
        assert_eq!(load(&f), p);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn missing_file_gives_defaults() {
        assert_eq!(load(&dir("none").join("x.json")), Prefs::default());
    }

    #[test]
    fn corrupt_file_is_backed_up() {
        let d = dir("bad");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("prefs.json");
        fs::write(&f, "{ esto no es json").unwrap();
        assert_eq!(load(&f), Prefs::default());
        assert!(d.join("prefs.json.bak").exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn unknown_fields_and_partial_files_are_tolerated() {
        let d = dir("partial");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("prefs.json");
        fs::write(&f, r#"{"theme":"light","campoFuturo":1}"#).unwrap();
        let p = load(&f);
        assert_eq!(p.theme, Theme::Light);
        assert_eq!(p.view_mode, ViewMode::Details);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn migrates_old_prefs_file() {
        // Formato de la Fase 1: "list" era la vista de detalles y "grid" la de iconos.
        let d = dir("migrate");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("prefs.json");
        fs::write(&f, r#"{"theme":"dark","viewMode":"list","favorites":[{"path":"C:/X","label":"X"}]}"#).unwrap();
        let p = load(&f);
        assert_eq!((p.theme, p.view_mode, p.version), (Theme::Dark, ViewMode::Details, 2));
        assert_eq!(p.favorites.len(), 1, "los favoritos se conservan");
        fs::write(&f, r#"{"viewMode":"grid"}"#).unwrap();
        assert_eq!(load(&f).view_mode, ViewMode::Icons);
        // Con la versión actual, "list" ya es la vista de lista compacta.
        fs::write(&f, r#"{"viewMode":"list","version":2}"#).unwrap();
        assert_eq!(load(&f).view_mode, ViewMode::List);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn old_files_get_the_new_preferences_with_safe_defaults() {
        let d = dir("newprefs");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("prefs.json");
        fs::write(&f, r#"{"theme":"dark","viewMode":"icons","version":2}"#).unwrap();
        let p = load(&f);
        assert!(p.thumbnails, "las miniaturas están activadas por defecto");
        assert!(p.system_icons, "los iconos del sistema están activados por defecto");
        assert!(!p.preview_pane);
        assert_eq!(p.view_mode, ViewMode::Icons);
        fs::write(&f, r#"{"thumbnails":false,"previewPane":true}"#).unwrap();
        let p = load(&f);
        assert!(!p.thumbnails && p.preview_pane);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn sanitize_caps_untrusted_data() {
        let mut p = Prefs { icon_size: 60000, ..Prefs::default() };
        p.recents = (0..100).map(|i| format!("C:\\{i}")).collect();
        p.folder_views = (0..500)
            .map(|i| FolderView { path: format!("C:\\{i}"), view_mode: ViewMode::Icons, icon_size: 1, sort_key: SortKey::Name, sort_dir: SortDir::Asc, group_by: GroupBy::None, pinned: false })
            .collect();
        p.columns = vec![Column::Size, Column::Size, Column::Type];
        p.sanitize();
        assert_eq!(p.icon_size, 512);
        assert_eq!(p.recents.len(), MAX_RECENTS);
        assert_eq!(p.folder_views.len(), MAX_FOLDER_VIEWS);
        assert_eq!(p.folder_views[0].path, "C:\\200", "se conservan las más recientes");
        assert_eq!(p.folder_views[0].icon_size, 16);
        assert_eq!(p.columns, vec![Column::Size, Column::Type]);
    }

    #[test]
    fn all_themes_round_trip_and_unknown_falls_back_to_system() {
        for (name, t) in [("dark", Theme::Dark), ("light", Theme::Light), ("cyberpunk", Theme::Cyberpunk), ("hacker", Theme::Hacker), ("minimal", Theme::Minimal), ("system", Theme::System)] {
            assert_eq!(serde_json::to_string(&t).unwrap(), format!("\"{name}\""));
            assert_eq!(serde_json::from_str::<Theme>(&format!("\"{name}\"")).unwrap(), t);
        }
        assert_eq!(serde_json::from_str::<Theme>("\"tema-del-futuro\"").unwrap(), Theme::System);
        let d = std::env::temp_dir().join(format!("nova-theme-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let f = d.join("prefs.json");
        fs::write(&f, r#"{"theme":"tema-del-futuro","viewMode":"icons","version":2}"#).unwrap();
        let p = load(&f);
        assert_eq!((p.theme, p.view_mode), (Theme::System, ViewMode::Icons), "un tema desconocido no descarta el resto de preferencias");
        let _ = fs::remove_dir_all(d);
    }
}

#[cfg(test)]
mod folder_view_tests {
    use super::*;

    #[test]
    fn folder_view_without_pinned_field_loads_as_not_pinned() {
        let j = r#"{"path":"C:\\A","viewMode":"icons","iconSize":96,"sortKey":"name","sortDir":"asc","groupBy":"none"}"#;
        let f: FolderView = serde_json::from_str(j).unwrap();
        assert!(!f.pinned);
        let back = serde_json::to_string(&FolderView { pinned: true, ..f }).unwrap();
        assert!(back.contains("\"pinned\":true"));
    }
}
