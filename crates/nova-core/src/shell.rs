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
    use std::os::windows::process::CommandExt;
    let script = "$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:NOVA_LNK);$s.TargetPath=$env:NOVA_TARGET;$s.WorkingDirectory=$env:NOVA_WD;$s.Save()";
    let wd = if target.is_dir() { target } else { target.parent().unwrap_or(target) };
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", script])
        .env("NOVA_LNK", link_path)
        .env("NOVA_TARGET", target)
        .env("NOVA_WD", wd)
        // Sin esto, desde una app de ventanas se abre una consola visible un instante (`-WindowStyle Hidden` llega tarde).
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
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

/// Terminal en su propia ventana, en `dir`, con el shell elegido.
///  * Windows Terminal (`wt.exe -d`) si está instalado.
///  * Si no, una consola nueva creada con `CreateProcessW` directamente. `std::process::Command` siempre pasa la entrada estándar
///    del padre, y en una aplicación de ventanas como LFind esa entrada es nula: la consola aparecía, pero no se podía escribir.
///  * `admin`: con el aviso de UAC de Windows. Si el usuario lo rechaza, se informa sin más.
#[cfg(windows)]
pub fn open_terminal(dir: &Path, shell: crate::terminal::Shell, admin: bool) -> io::Result<()> {
    use crate::terminal::{powershell_profile_plan, ProfilePlan, Shell};
    let plan = if shell == Shell::Powershell { powershell_profile_plan() } else { ProfilePlan::default() };
    if admin {
        return run_elevated(&shell.exe(), &shell.console_args(&plan, Some(dir)));
    }
    let args = shell.console_args(&plan, None);
    if let Ok(mut c) = windows_terminal() {
        use std::os::windows::process::CommandExt;
        c.arg("-d").arg(dir).arg(shell.exe()).raw_arg(&args);
        if c.spawn().is_ok() {
            return Ok(());
        }
    }
    new_console(&format!("\"{}\" {args}", shell.exe().display()), dir)
}

/// Windows Terminal instalado (alias de ejecución de la Tienda incluido).
#[cfg(windows)]
fn windows_terminal() -> io::Result<std::process::Command> {
    let alias = std::env::var_os("LOCALAPPDATA").map(|l| std::path::PathBuf::from(l).join("Microsoft\\WindowsApps\\wt.exe"));
    match alias.filter(|p| p.exists()) {
        Some(p) => Ok(std::process::Command::new(p)),
        None => Err(io::Error::new(io::ErrorKind::NotFound, "Windows Terminal no está instalado.")),
    }
}

/// Proceso de consola en una ventana propia, sin heredar nada de LFind: Windows le da su consola con teclado y pantalla propios.
#[cfg(windows)]
fn new_console(command_line: &str, dir: &Path) -> io::Result<()> {
    use crate::sys::win;
    #[repr(C)]
    struct StartupInfo {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        dims: [u32; 7],
        flags: u32,
        show: u16,
        r2: u16,
        r3: *mut u8,
        stdin: isize,
        stdout: isize,
        stderr: isize,
    }
    #[repr(C)]
    struct ProcessInfo {
        process: isize,
        thread: isize,
        pid: u32,
        tid: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateProcessW(app: *const u16, cmd: *mut u16, pa: *mut u8, ta: *mut u8, inherit: i32, flags: u32, env: *mut u8, cwd: *const u16, si: *mut StartupInfo, pi: *mut ProcessInfo) -> i32;
        fn CloseHandle(h: isize) -> i32;
    }
    const CREATE_NEW_CONSOLE: u32 = 0x10;
    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x400;
    let mut cmd: Vec<u16> = command_line.encode_utf16().chain(Some(0)).collect();
    let cwd = win::wide(dir.as_os_str());
    unsafe {
        let mut si: StartupInfo = std::mem::zeroed();
        si.cb = std::mem::size_of::<StartupInfo>() as u32;
        let mut pi: ProcessInfo = std::mem::zeroed();
        // Sin STARTF_USESTDHANDLES y sin heredar handles: la consola nueva aporta la entrada y la salida.
        if CreateProcessW(std::ptr::null(), cmd.as_mut_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), 0, CREATE_NEW_CONSOLE | CREATE_UNICODE_ENVIRONMENT, std::ptr::null_mut(), cwd.as_ptr(), &mut si, &mut pi) == 0 {
            return Err(io::Error::last_os_error());
        }
        CloseHandle(pi.thread);
        CloseHandle(pi.process);
    }
    Ok(())
}

/// Con el aviso de UAC. La carpeta va en los argumentos: Windows no respeta la carpeta de trabajo al elevar.
#[cfg(windows)]
fn run_elevated(exe: &Path, args: &str) -> io::Result<()> {
    use crate::sys::win;
    let (verb, file, params) = (win::wide("runas".as_ref()), win::wide(exe.as_os_str()), win::wide(args.as_ref()));
    unsafe {
        let com = win::CoInitializeEx(std::ptr::null_mut(), 2) >= 0;
        let r = win::ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), params.as_ptr(), std::ptr::null(), 1);
        if com {
            win::CoUninitialize();
        }
        match r {
            r if r > 32 => Ok(()),
            // SE_ERR_ACCESSDENIED: el usuario dijo que no en el aviso de UAC (o una directiva lo impide).
            5 => Err(io::Error::new(io::ErrorKind::PermissionDenied, "No se concedió el permiso de administrador. La terminal no se ha abierto.")),
            c => Err(io::Error::new(io::ErrorKind::Other, format!("Windows no pudo abrir la terminal como administrador (código {c})."))),
        }
    }
}

#[cfg(not(windows))]
pub fn open_terminal(dir: &Path, _shell: crate::terminal::Shell, _admin: bool) -> io::Result<()> {
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
