//! Integración con el shell del sistema: abrir con la aplicación predeterminada,
//! mostrar en el Explorador de Windows y crear accesos directos.

use std::io;
use std::path::Path;

/// Abre un archivo con su aplicación predeterminada (o una carpeta en el Explorador de Windows).
#[cfg(windows)]
pub fn open_path(p: &Path) -> io::Result<()> {
    use crate::sys::win;
    let file = win::wide(p.as_os_str());
    let verb = win::wide("open".as_ref());
    let dir = p.parent().map(|d| win::wide(d.as_os_str()));
    unsafe {
        // ShellExecute espera COM inicializado en el hilo que lo llama.
        let com = win::CoInitializeEx(std::ptr::null_mut(), 2) >= 0;
        let r = win::ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()), 1);
        if com {
            win::CoUninitialize();
        }
        if r > 32 {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, match r {
                31 => "No hay ninguna aplicación asociada a este tipo de archivo.".to_string(),
                5 => "Acceso denegado al abrir el archivo.".to_string(),
                2 | 3 => "No se encontró el archivo.".to_string(),
                c => format!("Windows no pudo abrir el archivo (código {c})."),
            }))
        }
    }
}

#[cfg(not(windows))]
pub fn open_path(p: &Path) -> io::Result<()> {
    std::process::Command::new("xdg-open").arg(p).spawn().map(|_| ())
}

/// Abre el Explorador de Windows con el elemento seleccionado.
#[cfg(windows)]
pub fn reveal(p: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    // explorer.exe devuelve código 1 aunque funcione: solo se lanza, no se comprueba el estado.
    std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", p.display())).spawn().map(|_| ())
}

#[cfg(not(windows))]
pub fn reveal(p: &Path) -> io::Result<()> {
    open_path(p.parent().unwrap_or(p))
}

/// Crea un acceso directo a `target` en `link_path`. En Windows se genera un `.lnk` real
/// mediante el objeto COM WScript.Shell (los datos viajan en variables de entorno: sin riesgo de inyección).
#[cfg(windows)]
pub fn create_shortcut(target: &Path, link_path: &Path) -> io::Result<()> {
    let script = "$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:NOVA_LNK);$s.TargetPath=$env:NOVA_TARGET;$s.WorkingDirectory=$env:NOVA_WD;$s.Save()";
    let wd = if target.is_dir() { target } else { target.parent().unwrap_or(target) };
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", script])
        .env("NOVA_LNK", link_path)
        .env("NOVA_TARGET", target)
        .env("NOVA_WD", wd)
        .output()?;
    if out.status.success() && link_path.exists() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, format!("No se pudo crear el acceso directo: {}", String::from_utf8_lossy(&out.stderr).trim())))
    }
}

#[cfg(not(windows))]
pub fn create_shortcut(target: &Path, link_path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link_path)
}

/// «Abrir con…»: el cuadro de selección de aplicación de Windows.
#[cfg(windows)]
pub fn open_with(p: &Path) -> io::Result<()> {
    crate::shellapps::choose_app(p).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}

#[cfg(not(windows))]
pub fn open_with(p: &Path) -> io::Result<()> {
    open_path(p)
}

/// Terminal en `dir`: Windows Terminal (`wt.exe -d`) y, si no está instalado, una consola de PowerShell.
#[cfg(windows)]
pub fn open_terminal(dir: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    if std::process::Command::new("wt.exe").arg("-d").arg(dir).spawn().is_ok() {
        return Ok(());
    }
    std::process::Command::new("powershell.exe").arg("-NoExit").current_dir(dir).creation_flags(0x0000_0010).spawn().map(|_| ()) // CREATE_NEW_CONSOLE
}

#[cfg(not(windows))]
pub fn open_terminal(dir: &Path) -> io::Result<()> {
    std::process::Command::new("x-terminal-emulator").current_dir(dir).spawn().map(|_| ())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn shortcut_is_created() {
        let d = std::env::temp_dir().join(format!("nova-sc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("t.txt"), b"x").unwrap();
        create_shortcut(&d.join("t.txt"), &d.join("t.lnk")).unwrap();
        assert!(d.join("t.lnk").exists());
        let _ = std::fs::remove_dir_all(d);
    }
}
