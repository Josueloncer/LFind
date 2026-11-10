//! Integración con Windows. Solo usa `HKEY_CURRENT_USER`, sin permisos de administrador y sin tocar `explorer.exe`, el escritorio ni el shell.
//!
//! Windows no ofrece un modo oficial de sustituir todas las ventanas del Explorador. Lo que sí admite es el verbo predeterminado de las
//! carpetas para el usuario actual: lo respetan las aplicaciones que abren carpetas con `ShellExecute`.
//! No lo respetan `Win+E`, `explorer.exe` lanzado directamente ni los diálogos de abrir y guardar: eso sigue siendo del Explorador.

use serde::{Deserialize, Serialize};

pub const VERB_MENU: &str = "LFind";
pub const VERB_OPEN: &str = "LFindOpen";
pub const RUN_VALUE: &str = "LFind";

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Feature {
    Preferred,
    ContextFolders,
    ContextDrives,
    Startup,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub preferred: bool,
    pub context_folders: bool,
    pub context_drives: bool,
    pub startup: bool,
    /// Algo apunta a otra instalación de LFind.
    pub stale: bool,
    pub exe: String,
}

/// Dónde escribe. Las pruebas usan una rama propia para no tocar la configuración real.
#[derive(Clone, Debug)]
pub struct Integration {
    pub classes: String,
    pub run: String,
    pub backup: String,
    pub exe: String,
}

impl Integration {
    pub fn for_user(exe: String) -> Self {
        Integration { classes: "Software\\Classes".into(), run: "Software\\Microsoft\\Windows\\CurrentVersion\\Run".into(), backup: "Software\\LFind\\Integration".into(), exe }
    }
}

/// Ruta que trae la línea de órdenes, ya limpia. Windows entrega `"C:\"` como `C:"` por la barra antes de la comilla.
pub fn path_from_args(args: &[String]) -> Option<String> {
    args.iter().skip(1).find(|a| !a.starts_with("--")).map(|a| {
        let t = a.trim().trim_matches('"').to_string();
        if t.len() == 2 && t.ends_with(':') { format!("{t}\\") } else { t }
    }).filter(|t| !t.is_empty())
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    type HKey = isize;
    const HKCU: HKey = 0x8000_0001u32 as i32 as isize;
    const KEY_READ: u32 = 0x20019;
    const KEY_WRITE: u32 = 0x20006;
    const REG_SZ: u32 = 1;
    const ERROR_FILE_NOT_FOUND: i32 = 2;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegCreateKeyExW(k: HKey, sub: *const u16, r: u32, class: *const u16, opts: u32, sam: u32, sa: *mut c_void, out: *mut HKey, disp: *mut u32) -> i32;
        fn RegOpenKeyExW(k: HKey, sub: *const u16, opts: u32, sam: u32, out: *mut HKey) -> i32;
        fn RegSetValueExW(k: HKey, name: *const u16, r: u32, kind: u32, data: *const u8, cb: u32) -> i32;
        fn RegQueryValueExW(k: HKey, name: *const u16, r: *mut u32, kind: *mut u32, data: *mut u8, cb: *mut u32) -> i32;
        fn RegDeleteValueW(k: HKey, name: *const u16) -> i32;
        fn RegDeleteTreeW(k: HKey, sub: *const u16) -> i32;
        fn RegDeleteKeyW(k: HKey, sub: *const u16) -> i32;
        fn RegQueryInfoKeyW(k: HKey, class: *mut u16, cch_class: *mut u32, r: *mut u32, subkeys: *mut u32, max_sub: *mut u32, max_class: *mut u32, values: *mut u32, max_name: *mut u32, max_data: *mut u32, sd: *mut u32, written: *mut c_void) -> i32;
        fn RegCloseKey(k: HKey) -> i32;
    }
    #[link(name = "shell32")]
    extern "system" {
        fn SHChangeNotify(event: i32, flags: u32, a: *const c_void, b: *const c_void);
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn name_ptr(name: &str, buf: &mut Vec<u16>) -> *const u16 {
        if name.is_empty() {
            null()
        } else {
            *buf = wide(name);
            buf.as_ptr()
        }
    }

    pub fn read(key: &str, name: &str) -> Option<String> {
        unsafe {
            let mut h: HKey = 0;
            if RegOpenKeyExW(HKCU, wide(key).as_ptr(), 0, KEY_READ, &mut h) != 0 {
                return None;
            }
            let mut holder = vec![];
            let n = name_ptr(name, &mut holder);
            let mut buf = vec![0u16; 2048];
            let (mut kind, mut cb) = (0u32, (buf.len() * 2) as u32);
            let r = RegQueryValueExW(h, n, null_mut(), &mut kind, buf.as_mut_ptr() as *mut u8, &mut cb);
            RegCloseKey(h);
            if r != 0 || kind != REG_SZ {
                return None;
            }
            let len = (cb as usize / 2).min(buf.len());
            let end = buf[..len].iter().position(|c| *c == 0).unwrap_or(len);
            Some(String::from_utf16_lossy(&buf[..end]))
        }
    }

    pub fn write(key: &str, name: &str, value: &str) -> Result<(), String> {
        unsafe {
            let mut h: HKey = 0;
            let r = RegCreateKeyExW(HKCU, wide(key).as_ptr(), 0, null(), 0, KEY_WRITE, null_mut(), &mut h, null_mut());
            if r != 0 {
                return Err(format!("No se pudo escribir en el Registro ({r})."));
            }
            let mut holder = vec![];
            let n = name_ptr(name, &mut holder);
            let data = wide(value);
            let r = RegSetValueExW(h, n, 0, REG_SZ, data.as_ptr() as *const u8, (data.len() * 2) as u32);
            RegCloseKey(h);
            if r == 0 { Ok(()) } else { Err(format!("No se pudo escribir en el Registro ({r}).")) }
        }
    }

    pub fn remove_value(key: &str, name: &str) -> Result<(), String> {
        unsafe {
            let mut h: HKey = 0;
            let r = RegOpenKeyExW(HKCU, wide(key).as_ptr(), 0, KEY_WRITE, &mut h);
            if r == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            if r != 0 {
                return Err(format!("No se pudo modificar el Registro ({r})."));
            }
            let mut holder = vec![];
            let n = name_ptr(name, &mut holder);
            let r = RegDeleteValueW(h, n);
            RegCloseKey(h);
            if r == 0 || r == ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(format!("No se pudo modificar el Registro ({r}).")) }
        }
    }

    pub fn remove_tree(key: &str) -> Result<(), String> {
        let r = unsafe { RegDeleteTreeW(HKCU, wide(key).as_ptr()) };
        if r == 0 || r == ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(format!("No se pudo modificar el Registro ({r}).")) }
    }

    /// Borra la clave solo si no tiene valores ni subclaves: nunca se lleva nada que no sea de LFind.
    pub fn remove_if_empty(key: &str) {
        unsafe {
            let mut h: HKey = 0;
            if RegOpenKeyExW(HKCU, wide(key).as_ptr(), 0, KEY_READ, &mut h) != 0 {
                return;
            }
            let (mut subkeys, mut values) = (1u32, 1u32);
            let r = RegQueryInfoKeyW(h, null_mut(), null_mut(), null_mut(), &mut subkeys, null_mut(), null_mut(), &mut values, null_mut(), null_mut(), null_mut(), null_mut());
            RegCloseKey(h);
            if r == 0 && subkeys == 0 && values == 0 {
                RegDeleteKeyW(HKCU, wide(key).as_ptr());
            }
        }
    }

    pub fn exists(key: &str) -> bool {
        unsafe {
            let mut h: HKey = 0;
            let ok = RegOpenKeyExW(HKCU, wide(key).as_ptr(), 0, KEY_READ, &mut h) == 0;
            if ok {
                RegCloseKey(h);
            }
            ok
        }
    }

    pub fn notify() {
        const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, 0, null(), null()) };
    }
}

#[cfg(windows)]
impl Integration {
    fn open_command(&self, arg: &str) -> String {
        format!("\"{}\" \"{arg}\"", self.exe)
    }

    fn set_verb(&self, key: &str, label: &str, arg: &str) -> Result<(), String> {
        imp::write(key, "MUIVerb", label)?;
        imp::write(key, "Icon", &format!("{},0", self.exe))?;
        imp::write(&format!("{key}\\command"), "", &self.open_command(arg))
    }

    fn folder_verb(&self) -> String {
        format!("{}\\Directory\\shell\\{VERB_MENU}", self.classes)
    }
    fn background_verb(&self) -> String {
        format!("{}\\Directory\\Background\\shell\\{VERB_MENU}", self.classes)
    }
    fn drive_verb(&self) -> String {
        format!("{}\\Drive\\shell\\{VERB_MENU}", self.classes)
    }
    fn shell_key(&self, kind: &str) -> String {
        format!("{}\\{kind}\\shell", self.classes)
    }

    fn command_is_ours(&self, verb_key: &str) -> Option<bool> {
        imp::read(&format!("{verb_key}\\command"), "").map(|c| c.starts_with(&format!("\"{}\"", self.exe)))
    }

    pub fn status(&self) -> Status {
        let open_key = format!("{}\\{VERB_OPEN}", self.shell_key("Directory"));
        let preferred = imp::read(&self.shell_key("Directory"), "").is_some_and(|v| v == VERB_OPEN);
        let run = imp::read(&self.run, RUN_VALUE);
        let mut stale = run.as_ref().is_some_and(|r| !r.starts_with(&format!("\"{}\"", self.exe)));
        for key in [self.folder_verb(), self.drive_verb(), open_key] {
            stale |= self.command_is_ours(&key) == Some(false);
        }
        Status { preferred, context_folders: imp::exists(&self.folder_verb()), context_drives: imp::exists(&self.drive_verb()), startup: run.is_some(), stale, exe: self.exe.clone() }
    }

    /// Guarda el valor anterior una sola vez: al volver a activar no se debe perder el original.
    fn remember_default(&self, kind: &str) -> Result<(), String> {
        let marker = format!("has_{kind}");
        if imp::read(&self.backup, &marker).is_some() {
            return Ok(());
        }
        if let Some(v) = imp::read(&self.shell_key(kind), "").filter(|v| v != VERB_OPEN) {
            imp::write(&self.backup, kind, &v)?;
        }
        imp::write(&self.backup, &marker, "1")
    }

    fn restore_default(&self, kind: &str) -> Result<(), String> {
        let shell = self.shell_key(kind);
        if imp::read(&shell, "").is_some_and(|v| v == VERB_OPEN) {
            match imp::read(&self.backup, kind) {
                Some(v) => imp::write(&shell, "", &v)?,
                None => imp::remove_value(&shell, "")?,
            }
        }
        imp::remove_tree(&format!("{shell}\\{VERB_OPEN}"))?;
        imp::remove_value(&self.backup, kind)?;
        imp::remove_value(&self.backup, &format!("has_{kind}"))
    }

    pub fn set(&self, feature: Feature, on: bool) -> Result<Status, String> {
        match (feature, on) {
            (Feature::Preferred, true) => {
                for kind in ["Directory", "Drive"] {
                    self.remember_default(kind)?;
                    self.set_verb(&format!("{}\\{VERB_OPEN}", self.shell_key(kind)), "Abrir en LFind", "%1")?;
                    imp::write(&self.shell_key(kind), "", VERB_OPEN)?;
                }
            }
            (Feature::Preferred, false) => {
                self.restore_default("Directory")?;
                self.restore_default("Drive")?;
                self.forget_backup()?;
            }
            (Feature::ContextFolders, true) => {
                self.set_verb(&self.folder_verb(), "Abrir en LFind", "%1")?;
                self.set_verb(&self.background_verb(), "Abrir en LFind", "%V")?;
            }
            (Feature::ContextFolders, false) => {
                imp::remove_tree(&self.folder_verb())?;
                imp::remove_tree(&self.background_verb())?;
            }
            (Feature::ContextDrives, true) => self.set_verb(&self.drive_verb(), "Abrir en LFind", "%1")?,
            (Feature::ContextDrives, false) => imp::remove_tree(&self.drive_verb())?,
            (Feature::Startup, true) => imp::write(&self.run, RUN_VALUE, &format!("\"{}\"", self.exe))?,
            (Feature::Startup, false) => imp::remove_value(&self.run, RUN_VALUE)?,
        }
        imp::notify();
        Ok(self.status())
    }

    /// Vuelve a escribir lo activo apuntando a este ejecutable. Corrige entradas de otra instalación o de una compilación de desarrollo.
    pub fn repair(&self) -> Result<Status, String> {
        let now = self.status();
        for (active, f) in [(now.preferred, Feature::Preferred), (now.context_folders, Feature::ContextFolders), (now.context_drives, Feature::ContextDrives), (now.startup, Feature::Startup)] {
            if active {
                self.set(f, true)?;
            }
        }
        Ok(self.status())
    }

    /// Deja Windows como estaba antes de LFind. Solo retira lo que LFind creó.
    pub fn restore(&self) -> Result<Status, String> {
        for f in [Feature::Preferred, Feature::ContextFolders, Feature::ContextDrives, Feature::Startup] {
            self.set(f, false)?;
        }
        self.forget_backup()?;
        Ok(self.status())
    }

    /// La copia del valor anterior solo existe mientras LFind es el explorador preferido; su carpeta, si queda vacía, tampoco se deja.
    fn forget_backup(&self) -> Result<(), String> {
        imp::remove_tree(&self.backup)?;
        if let Some((parent, _)) = self.backup.rsplit_once('\\') {
            imp::remove_if_empty(parent);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
impl Integration {
    pub fn status(&self) -> Status {
        Status { exe: self.exe.clone(), ..Default::default() }
    }
    pub fn set(&self, _f: Feature, _on: bool) -> Result<Status, String> {
        Err("Solo disponible en Windows.".into())
    }
    pub fn restore(&self) -> Result<Status, String> {
        Ok(self.status())
    }
    pub fn repair(&self) -> Result<Status, String> {
        Ok(self.status())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_path_of_the_command_line() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(path_from_args(&args(&["lfind.exe", "D:\\Fotos"])), Some("D:\\Fotos".to_string()));
        assert_eq!(path_from_args(&args(&["lfind.exe", "C:\""])), Some("C:\\".to_string()));
        assert_eq!(path_from_args(&args(&["lfind.exe", "--minimized", "E:\\"])), Some("E:\\".to_string()));
        assert_eq!(path_from_args(&args(&["lfind.exe"])), None);
        assert_eq!(path_from_args(&args(&["lfind.exe", "--minimized"])), None);
    }

    #[cfg(windows)]
    mod registry {
        use super::super::*;
        use std::sync::atomic::{AtomicU32, Ordering};

        static N: AtomicU32 = AtomicU32::new(0);

        struct T(Integration, String);
        impl Drop for T {
            fn drop(&mut self) {
                let _ = imp::remove_tree(&self.1);
            }
        }
        /// Rama aislada: estas pruebas no tocan las asociaciones reales.
        fn sandbox() -> T {
            let base = format!("Software\\LFindTests\\{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed));
            T(Integration { classes: format!("{base}\\Classes"), run: format!("{base}\\Run"), backup: format!("{base}\\Backup"), exe: "C:\\Programas\\LFind\\lfind.exe".into() }, base)
        }

        #[test]
        fn starts_with_nothing_enabled() {
            let t = sandbox();
            let s = t.0.status();
            assert!(!s.preferred && !s.context_folders && !s.context_drives && !s.startup && !s.stale);
        }

        #[test]
        fn context_menu_opens_the_clicked_folder() {
            let t = sandbox();
            assert!(t.0.set(Feature::ContextFolders, true).unwrap().context_folders);
            assert_eq!(imp::read(&format!("{}\\command", t.0.folder_verb()), "").unwrap(), "\"C:\\Programas\\LFind\\lfind.exe\" \"%1\"");
            assert_eq!(imp::read(&format!("{}\\command", t.0.background_verb()), "").unwrap(), "\"C:\\Programas\\LFind\\lfind.exe\" \"%V\"");
            assert!(!t.0.set(Feature::ContextFolders, false).unwrap().context_folders);
            assert!(!imp::exists(&t.0.background_verb()));
        }

        #[test]
        fn turning_preferred_off_leaves_no_empty_keys_but_keeps_foreign_values() {
            let t = sandbox();
            let i = Integration { backup: format!("{}\\Propia\\Integration", t.1), ..t.0.clone() };
            i.set(Feature::Preferred, true).unwrap();
            assert!(imp::exists(&i.backup));
            i.set(Feature::Preferred, false).unwrap();
            assert!(!imp::exists(&format!("{}\\Propia", t.1)), "ni la copia ni su carpeta vacía");
            imp::write(&format!("{}\\Propia", t.1), "ajeno", "1").unwrap();
            i.set(Feature::Preferred, true).unwrap();
            i.set(Feature::Preferred, false).unwrap();
            assert!(!imp::exists(&i.backup));
            assert_eq!(imp::read(&format!("{}\\Propia", t.1), "ajeno").as_deref(), Some("1"), "lo que no es de LFind se queda");
        }

        #[test]
        fn preferred_keeps_and_restores_the_previous_default() {
            let t = sandbox();
            imp::write(&t.0.shell_key("Directory"), "", "otroExplorador").unwrap();
            assert!(t.0.set(Feature::Preferred, true).unwrap().preferred);
            assert_eq!(imp::read(&t.0.shell_key("Directory"), "").unwrap(), VERB_OPEN);
            // Activarlo de nuevo no debe confundir el valor original con el propio.
            t.0.set(Feature::Preferred, true).unwrap();
            assert!(!t.0.set(Feature::Preferred, false).unwrap().preferred);
            assert_eq!(imp::read(&t.0.shell_key("Directory"), "").unwrap(), "otroExplorador");
            assert!(!imp::exists(&format!("{}\\{VERB_OPEN}", t.0.shell_key("Directory"))));
        }

        #[test]
        fn without_a_previous_default_the_value_disappears() {
            let t = sandbox();
            t.0.set(Feature::Preferred, true).unwrap();
            t.0.set(Feature::Preferred, false).unwrap();
            assert_eq!(imp::read(&t.0.shell_key("Directory"), ""), None);
            assert_eq!(imp::read(&t.0.shell_key("Drive"), ""), None);
        }

        #[test]
        fn restore_only_removes_what_lfind_created() {
            let t = sandbox();
            imp::write(&format!("{}\\Directory\\shell\\AjenoVerb\\command", t.0.classes), "", "ajeno").unwrap();
            imp::write(&t.0.run, "OtraApp", "otra.exe").unwrap();
            for f in [Feature::Preferred, Feature::ContextFolders, Feature::ContextDrives, Feature::Startup] {
                t.0.set(f, true).unwrap();
            }
            let on = t.0.status();
            assert!(on.preferred && on.context_folders && on.context_drives && on.startup);
            let off = t.0.restore().unwrap();
            assert!(!off.preferred && !off.context_folders && !off.context_drives && !off.startup);
            assert_eq!(imp::read(&format!("{}\\Directory\\shell\\AjenoVerb\\command", t.0.classes), "").unwrap(), "ajeno");
            assert_eq!(imp::read(&t.0.run, "OtraApp").unwrap(), "otra.exe");
            assert!(!imp::exists(&t.0.backup));
        }

        #[test]
        fn detects_entries_from_another_installation() {
            let t = sandbox();
            t.0.set(Feature::Startup, true).unwrap();
            let moved = Integration { exe: "D:\\Otra\\lfind.exe".into(), ..t.0.clone() };
            assert!(moved.status().stale);
            assert!(!t.0.status().stale);
        }

        #[test]
        fn repair_points_every_active_entry_to_this_executable() {
            let t = sandbox();
            let old = Integration { exe: "C:\\Dev\\target\\debug\\viejo.exe".into(), ..t.0.clone() };
            for f in [Feature::Preferred, Feature::ContextFolders, Feature::Startup] {
                old.set(f, true).unwrap();
            }
            assert!(t.0.status().stale);
            let s = t.0.repair().unwrap();
            assert!(!s.stale && s.preferred && s.context_folders && s.startup && !s.context_drives, "lo inactivo no se activa: {s:?}");
            assert_eq!(imp::read(&format!("{}\\{VERB_OPEN}\\command", t.0.shell_key("Directory")), "").unwrap(), "\"C:\\Programas\\LFind\\lfind.exe\" \"%1\"");
        }

        #[test]
        fn startup_is_written_quoted() {
            let t = sandbox();
            t.0.set(Feature::Startup, true).unwrap();
            assert_eq!(imp::read(&t.0.run, RUN_VALUE).unwrap(), "\"C:\\Programas\\LFind\\lfind.exe\"");
        }
    }
}
