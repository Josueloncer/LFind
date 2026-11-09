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
pub const CURRENT_VERSION: u32 = 3;
const MAX_APPEARANCE_JSON: usize = 128 * 1024;
const MAX_LAYOUT_ITEMS: usize = 128;
const MAX_LAYOUT_ID: usize = 1024;

/// Carpeta del historial. El orden de la lista es de más a menos reciente.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecentFolder {
    pub path: String,
    /// Último acceso, en milisegundos epoch. 0 si procede de una versión que no lo guardaba.
    pub at: i64,
}

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

/// Distribución personalizada de la interfaz. Listas de identificadores estables; vacías, la distribución original.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Layout {
    pub sections: Vec<String>,
    pub places: Vec<String>,
    pub drives: Vec<String>,
    pub toolbar: Vec<String>,
    pub hidden_tools: Vec<String>,
}

impl Layout {
    fn sanitize(&mut self) {
        for list in [&mut self.sections, &mut self.places, &mut self.drives, &mut self.toolbar, &mut self.hidden_tools] {
            list.truncate(MAX_LAYOUT_ITEMS);
            let mut seen = vec![];
            list.retain(|s| s.len() <= MAX_LAYOUT_ID && !seen.contains(s) && {
                seen.push(s.clone());
                true
            });
        }
    }
}

/// Una distribución dañada no invalida el archivo: se pierde solo ella, nunca las ubicaciones fijadas ni el resto.
fn lenient_layout<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Layout, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
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
    /// Formato anterior del historial: se migra a `recent_folders` y queda vacío.
    pub recents: Vec<String>,
    pub recent_folders: Vec<RecentFolder>,
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
    /// Personalización visual. La valida la interfaz, campo por campo; un valor dañado no invalida el resto.
    #[serde(deserialize_with = "any_json")]
    pub appearance: serde_json::Value,
    #[serde(deserialize_with = "any_json")]
    pub custom_themes: serde_json::Value,
    /// Vista de doble panel.
    pub dual_pane: bool,
    /// Posición del divisor, en milésimas de la anchura.
    pub dual_split: u16,
    #[serde(default = "terminal_height")]
    pub terminal_height: u16,
    #[serde(default = "terminal_shell")]
    pub terminal_shell: String,
    #[serde(default)]
    pub terminal_sync: bool,
    #[serde(default = "sidebar_side")]
    pub sidebar_side: String,
    #[serde(default = "preview_tab")]
    pub preview_tab: String,
    /// Actualización de los informes del analizador: `auto`, `manual` u `off`.
    #[serde(default = "manual_refresh")]
    pub analyzer_refresh: String,
    /// Distribución de barras y secciones. Campo añadido sin cambiar la versión: un archivo sin él usa la distribución original.
    #[serde(deserialize_with = "lenient_layout")]
    pub layout: Layout,
    /// Versión del formato; permite migrar archivos antiguos. Si falta, es 0.
    #[serde(default = "zero")]
    pub version: u32,
}

fn any_json<'de, D: serde::Deserializer<'de>>(d: D) -> Result<serde_json::Value, D::Error> {
    serde_json::Value::deserialize(d)
}

fn terminal_height() -> u16 {
    280
}

fn terminal_shell() -> String {
    "powershell".into()
}

fn sidebar_side() -> String {
    "left".into()
}

fn preview_tab() -> String {
    "preview".into()
}

fn manual_refresh() -> String {
    "manual".into()
}

fn zero() -> u32 {
    0
}

fn newest_first(k: SortKey) -> bool {
    matches!(k, SortKey::Modified | SortKey::Created | SortKey::Size)
}

impl Prefs {
    /// Limita tamaños y valores: lo que llega de la interfaz no es de fiar.
    pub fn sanitize(&mut self) {
        self.icon_size = self.icon_size.clamp(16, 512);
        self.preview_width = self.preview_width.clamp(220, 900);
        self.recents.truncate(MAX_RECENTS);
        self.recent_folders.truncate(MAX_RECENTS);
        self.favorites.truncate(MAX_FAVORITES);
        let mut pinned = vec![];
        self.favorites.retain(|f| {
            let k = crate::index::classify::path_key(&f.path);
            let new = !pinned.contains(&k);
            if new {
                pinned.push(k);
            }
            new
        });
        let excess = self.folder_views.len().saturating_sub(MAX_FOLDER_VIEWS);
        self.folder_views.drain(..excess); // se descartan las más antiguas
        for f in &mut self.folder_views {
            f.icon_size = f.icon_size.clamp(16, 512);
        }
        self.dual_split = self.dual_split.clamp(200, 800);
        self.terminal_height = self.terminal_height.clamp(120, 2000);
        if !matches!(self.terminal_shell.as_str(), "powershell" | "cmd") {
            self.terminal_shell = terminal_shell();
        }
        if !matches!(self.sidebar_side.as_str(), "left" | "right") {
            self.sidebar_side = sidebar_side();
        }
        if !matches!(self.preview_tab.as_str(), "preview" | "details") {
            self.preview_tab = preview_tab();
        }
        for v in [&mut self.appearance, &mut self.custom_themes] {
            if v.to_string().len() > MAX_APPEARANCE_JSON {
                *v = serde_json::Value::Null;
            }
        }
        self.layout.sanitize();
        if !matches!(self.analyzer_refresh.as_str(), "auto" | "manual" | "off") {
            self.analyzer_refresh = manual_refresh();
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
        }
        if self.version < 3 {
            self.recent_folders = std::mem::take(&mut self.recents).into_iter().map(|path| RecentFolder { path, at: 0 }).collect();
            // Hasta ahora elegir fecha o tamaño fijaba siempre el orden ascendente, por un error: se corrige lo que quedó guardado.
            if newest_first(self.sort_key) && self.sort_dir == SortDir::Asc {
                self.sort_dir = SortDir::Desc;
            }
            for f in &mut self.folder_views {
                if newest_first(f.sort_key) && f.sort_dir == SortDir::Asc {
                    f.sort_dir = SortDir::Desc;
                }
            }
        }
        self.version = CURRENT_VERSION;
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
            recent_folders: vec![],
            folder_views: vec![],
            thumbnails: true,
            system_icons: true,
            preview_pane: false,
            preview_width: 320,
            auto_update_check: true,
            last_update_check: 0,
            appearance: serde_json::Value::Null,
            custom_themes: serde_json::Value::Null,
            dual_pane: false,
            dual_split: 500,
            preview_tab: preview_tab(),
            sidebar_side: sidebar_side(),
            terminal_height: terminal_height(),
            terminal_shell: terminal_shell(),
            terminal_sync: false,
            analyzer_refresh: manual_refresh(),
            layout: Layout::default(),
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
    fn files_without_layout_keep_everything_and_use_the_original_layout() {
        let d = dir("layout-old");
        let f = d.join("prefs.json");
        fs::create_dir_all(&d).unwrap();
        fs::write(&f, r#"{"version":3,"theme":"hacker","favorites":[{"path":"C:\\A","label":"Mi A"}]}"#).unwrap();
        let p = load(&f);
        assert_eq!(p.layout, Layout::default());
        assert_eq!((p.theme, p.favorites.len(), p.favorites[0].label.as_str(), p.version), (Theme::Hacker, 1, "Mi A", CURRENT_VERSION));
    }

    #[test]
    fn new_fields_are_optional_and_tolerate_any_value() {
        let d = dir("appearance");
        let f = d.join("prefs.json");
        fs::create_dir_all(&d).unwrap();
        fs::write(&f, r#"{"version":3,"favorites":[{"path":"C:\\A","label":"Mi A"}]}"#).unwrap();
        let p = load(&f);
        assert_eq!((p.dual_pane, p.dual_split, p.analyzer_refresh.as_str(), p.appearance.is_null()), (false, 500, "manual", true));
        fs::write(&f, r#"{"version":3,"favorites":[{"path":"C:\\A","label":"Mi A"}],"appearance":7,"customThemes":"x","analyzerRefresh":"raro","dualSplit":9000}"#).unwrap();
        let q = load(&f);
        assert_eq!((q.favorites.len(), q.analyzer_refresh.as_str(), q.dual_split, q.appearance.as_i64()), (1, "manual", 800, Some(7)));
        let mut big = Prefs::default();
        big.appearance = serde_json::Value::String("x".repeat(MAX_APPEARANCE_JSON + 1));
        big.sanitize();
        assert!(big.appearance.is_null(), "tamaño limitado");
    }

    #[test]
    fn a_damaged_layout_never_loses_the_rest() {
        let d = dir("layout-bad");
        let f = d.join("prefs.json");
        fs::create_dir_all(&d).unwrap();
        for bad in ["3", "\"x\"", "[]", "null", r#"{"sections":7}"#, r#"{"toolbar":[1,2]}"#] {
            fs::write(&f, format!(r#"{{"version":3,"favorites":[{{"path":"C:\\A","label":"Mi A"}}],"layout":{bad}}}"#)).unwrap();
            let p = load(&f);
            assert_eq!(p.favorites.len(), 1, "layout dañado: {bad}");
            assert_eq!(p.layout, Layout::default());
            assert!(!f.with_extension("json.bak").exists(), "no se trata como archivo corrupto");
        }
    }

    #[test]
    fn layout_round_trip_and_limits() {
        let d = dir("layout-save");
        let f = d.join("prefs.json");
        let mut p = Prefs::default();
        p.layout.sections = vec!["computer".into(), "computer".into(), "locations".into()];
        p.layout.hidden_tools = vec!["new".into(), "x".repeat(MAX_LAYOUT_ID + 1)];
        p.layout.toolbar = (0..MAX_LAYOUT_ITEMS + 9).map(|i| i.to_string()).collect();
        save(&f, &p).unwrap();
        let q = load(&f);
        assert_eq!(q.layout.sections, ["computer", "locations"]);
        assert_eq!(q.layout.hidden_tools, ["new"]);
        assert_eq!(q.layout.toolbar.len(), MAX_LAYOUT_ITEMS);
        assert_eq!(q.version, CURRENT_VERSION);
        let raw = fs::read_to_string(&f).unwrap();
        assert!(raw.contains("hiddenTools"), "se guarda en camelCase, como lo lee la interfaz");
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
    fn version_2_files_migrate_history_favorites_and_wrong_sort_directions() {
        let d = dir("v2");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("prefs.json");
        fs::write(
            &f,
            r#"{"version":2,"sortKey":"size","sortDir":"asc","recents":["C:\\A","C:\\B"],
                "favorites":[{"path":"C:\\X","label":"Mi X"},{"path":"c:/x/","label":"dup"}],
                "folderViews":[{"path":"C:\\F","viewMode":"icons","iconSize":64,"sortKey":"modified","sortDir":"asc","groupBy":"none"},
                               {"path":"C:\\G","viewMode":"icons","iconSize":64,"sortKey":"name","sortDir":"asc","groupBy":"none"},
                               {"path":"C:\\H","viewMode":"icons","iconSize":64,"sortKey":"created","sortDir":"desc","groupBy":"none"}]}"#,
        )
        .unwrap();
        let p = load(&f);
        assert_eq!(p.recent_folders.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(), ["C:\\A", "C:\\B"], "se conserva el orden");
        assert!(p.recents.is_empty());
        assert_eq!(p.favorites, vec![Favorite { path: "C:\\X".into(), label: "Mi X".into() }], "sin perderlos y sin duplicados");
        assert_eq!(p.sort_dir, SortDir::Desc);
        assert_eq!(p.folder_views.iter().map(|v| v.sort_dir).collect::<Vec<_>>(), [SortDir::Desc, SortDir::Asc, SortDir::Desc], "el nombre sigue ascendente");
        // Una vez migrado, una elección explícita del usuario ya no se toca.
        let mut q = p.clone();
        q.sort_dir = SortDir::Asc;
        save(&f, &q).unwrap();
        assert_eq!(load(&f).sort_dir, SortDir::Asc);
        let _ = fs::remove_dir_all(d);
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
        assert_eq!((p.theme, p.view_mode, p.version), (Theme::Dark, ViewMode::Details, CURRENT_VERSION));
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
