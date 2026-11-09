//! Terminal real sobre la pseudoconsola de Windows (ConPTY). Cada sesión es un proceso con su propia consola; sus hijos van en un Job Object
//! que los termina con la sesión, también si LFind se cierra de golpe: no quedan procesos huérfanos.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Shell {
    Powershell,
    Cmd,
}

fn system32() -> PathBuf {
    PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into())).join("System32")
}

impl Shell {
    pub fn command_line(self) -> String {
        match self {
            // El indicador avisa de la carpeta actual con una secuencia invisible (OSC 9;9, la misma de Windows Terminal). LFind la ignora si la sincronización está desactivada.
            Shell::Cmd => format!("\"{}\" /k prompt $E]9;9;$P$E\\$P$G", system32().join("cmd.exe").display()),
            Shell::Powershell => {
                const REPORT: &str = "$global:__lfindPrompt=$function:prompt;function global:prompt{$e=[char]27;$b=[char]7;[Console]::Out.Write(\"$e]9;9;$($executionContext.SessionState.Path.CurrentFileSystemLocation.ProviderPath)$b\");& $global:__lfindPrompt}";
                let utf16: Vec<u8> = REPORT.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
                format!("\"{}\" -NoLogo -NoExit -EncodedCommand {}", system32().join("WindowsPowerShell\\v1.0\\powershell.exe").display(), crate::b64::encode(&utf16))
            }
        }
    }
}

/// Carpeta de inicio: la pedida si existe de verdad; si no (ruta virtual, unidad retirada), la del usuario.
pub fn start_dir(requested: Option<&str>) -> PathBuf {
    requested.map(PathBuf::from).filter(|p| p.is_dir()).or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("C:\\"))
}

pub type OnOutput = Box<dyn FnMut(Vec<u8>) + Send>;
pub type OnExit = Box<dyn FnOnce(u32) + Send>;

#[cfg(windows)]
pub use imp::Session;

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{sync_channel, SyncSender};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;

    type H = *mut c_void;

    #[repr(C)]
    struct StartupInfoEx {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        cx: u32,
        cy: u32,
        cols: u32,
        rows: u32,
        fill: u32,
        flags: u32,
        show: u16,
        r2: u16,
        r3: *mut u8,
        stdin: H,
        stdout: H,
        stderr: H,
        attrs: *mut c_void,
    }
    #[repr(C)]
    struct ProcessInfo {
        process: H,
        thread: H,
        pid: u32,
        tid: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct BasicLimit {
        a: i64,
        b: i64,
        flags: u32,
        c: usize,
        d: usize,
        e: u32,
        f: usize,
        g: u32,
        h: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io: [u64; 6],
        process: usize,
        job: usize,
        peak_p: usize,
        peak_j: usize,
    }

    #[link(name = "kernel32")]
    #[allow(clashing_extern_declarations)]
    extern "system" {
        fn CreatePipe(r: *mut H, w: *mut H, sa: *mut c_void, size: u32) -> i32;
        fn CreatePseudoConsole(size: u32, input: H, output: H, flags: u32, out: *mut H) -> i32;
        fn ResizePseudoConsole(h: H, size: u32) -> i32;
        fn ClosePseudoConsole(h: H);
        fn InitializeProcThreadAttributeList(l: *mut c_void, n: u32, f: u32, size: *mut usize) -> i32;
        fn UpdateProcThreadAttribute(l: *mut c_void, f: u32, attr: usize, v: *mut c_void, cb: usize, prev: *mut c_void, ret: *mut usize) -> i32;
        fn DeleteProcThreadAttributeList(l: *mut c_void);
        fn CreateProcessW(app: *const u16, cmd: *mut u16, pa: *mut c_void, ta: *mut c_void, inherit: i32, flags: u32, env: *mut c_void, cwd: *const u16, si: *mut StartupInfoEx, pi: *mut ProcessInfo) -> i32;
        fn ResumeThread(h: H) -> u32;
        fn ReadFile(h: H, buf: *mut u8, n: u32, read: *mut u32, ov: *mut c_void) -> i32;
        fn WriteFile(h: H, buf: *const u8, n: u32, wrote: *mut u32, ov: *mut c_void) -> i32;
        fn CloseHandle(h: H) -> i32;
        fn CreateJobObjectW(sa: *mut c_void, name: *const u16) -> H;
        fn SetInformationJobObject(job: H, class: u32, info: *mut c_void, cb: u32) -> i32;
        fn AssignProcessToJobObject(job: H, p: H) -> i32;
        fn TerminateJobObject(job: H, code: u32) -> i32;
        fn QueryInformationJobObject(job: H, class: u32, info: *mut c_void, cb: u32, ret: *mut u32) -> i32;
        fn GetExitCodeProcess(p: H, code: *mut u32) -> i32;
        fn WaitForSingleObject(h: H, ms: u32) -> u32;
        fn TerminateProcess(p: H, code: u32) -> i32;
    }

    const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x20016;
    const EXTENDED_STARTUPINFO_PRESENT: u32 = 0x0008_0000;
    const CREATE_SUSPENDED: u32 = 0x4;
    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x400;
    const STARTF_USESTDHANDLES: u32 = 0x100;
    const KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const JOB_EXTENDED_LIMITS: u32 = 9;
    const JOB_PROCESS_LIST: u32 = 3;
    const INFINITE: u32 = 0xFFFF_FFFF;
    const READ_BUF: usize = 16 * 1024;
    /// Lo que la interfaz puede tener sin pintar antes de que se deje de leer: un comando que escupe sin parar no llena la memoria.
    const MAX_UNACKED: u64 = 2 << 20;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn coord(cols: u16, rows: u16) -> u32 {
        (cols.max(2) as u32) | ((rows.max(1) as u32) << 16)
    }

    #[derive(Clone, Copy)]
    struct Handle(H);
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}

    pub struct Session {
        console: Handle,
        job: Handle,
        process: Handle,
        writer: Option<SyncSender<Vec<u8>>>,
        reader: Option<JoinHandle<()>>,
        waiter: Option<JoinHandle<()>>,
        unacked: Arc<AtomicU64>,
        closing: Arc<AtomicBool>,
        closed: bool,
    }

    unsafe impl Send for Session {}
    unsafe impl Sync for Session {}

    impl Session {
        pub fn start(shell: Shell, cwd: &Path, cols: u16, rows: u16, on_output: OnOutput, on_exit: OnExit) -> Result<Session, String> {
            Self::spawn(&shell.command_line(), cwd, cols, rows, on_output, on_exit)
        }

        pub fn spawn(command: &str, cwd: &Path, cols: u16, rows: u16, mut on_output: OnOutput, on_exit: OnExit) -> Result<Session, String> {
            unsafe {
                let (mut in_r, mut in_w, mut out_r, mut out_w): (H, H, H, H) = (null_mut(), null_mut(), null_mut(), null_mut());
                if CreatePipe(&mut in_r, &mut in_w, null_mut(), 0) == 0 {
                    return Err("No se pudo crear la tubería de entrada.".into());
                }
                if CreatePipe(&mut out_r, &mut out_w, null_mut(), 0) == 0 {
                    CloseHandle(in_r);
                    CloseHandle(in_w);
                    return Err("No se pudo crear la tubería de salida.".into());
                }
                let mut console: H = null_mut();
                let hr = CreatePseudoConsole(coord(cols, rows), in_r, out_w, 0, &mut console);
                // La pseudoconsola se queda con su copia.
                CloseHandle(in_r);
                CloseHandle(out_w);
                if hr < 0 {
                    CloseHandle(in_w);
                    CloseHandle(out_r);
                    return Err(format!("Esta versión de Windows no admite la pseudoconsola ({hr:#x})."));
                }
                let fail = |msg: String, console: H, in_w: H, out_r: H| {
                    ClosePseudoConsole(console);
                    CloseHandle(in_w);
                    CloseHandle(out_r);
                    msg
                };
                let mut size = 0usize;
                InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size);
                let mut list = vec![0u8; size];
                let attrs = list.as_mut_ptr() as *mut c_void;
                if InitializeProcThreadAttributeList(attrs, 1, 0, &mut size) == 0 || UpdateProcThreadAttribute(attrs, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, console, size_of::<H>(), null_mut(), null_mut()) == 0 {
                    return Err(fail("No se pudo preparar la consola.".into(), console, in_w, out_r));
                }
                let mut si: StartupInfoEx = zeroed();
                si.cb = size_of::<StartupInfoEx>() as u32;
                // Con los tres nulos, el proceso no hereda los canales de LFind y usa los de la pseudoconsola.
                si.flags = STARTF_USESTDHANDLES;
                si.attrs = attrs;
                let mut pi: ProcessInfo = zeroed();
                let mut cmd = wide(command);
                let dir = wide(&cwd.to_string_lossy());
                let created = CreateProcessW(null(), cmd.as_mut_ptr(), null_mut(), null_mut(), 0, EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT, null_mut(), dir.as_ptr(), &mut si, &mut pi);
                DeleteProcThreadAttributeList(attrs);
                if created == 0 {
                    return Err(fail(format!("No se pudo iniciar la terminal: {}", std::io::Error::last_os_error()), console, in_w, out_r));
                }
                let job = CreateJobObjectW(null_mut(), null());
                let mut limit = ExtendedLimit::default();
                limit.basic.flags = KILL_ON_JOB_CLOSE;
                if job.is_null() || SetInformationJobObject(job, JOB_EXTENDED_LIMITS, &mut limit as *mut _ as *mut c_void, size_of::<ExtendedLimit>() as u32) == 0 || AssignProcessToJobObject(job, pi.process) == 0 {
                    TerminateProcess(pi.process, 1);
                    CloseHandle(pi.process);
                    CloseHandle(pi.thread);
                    if !job.is_null() {
                        CloseHandle(job);
                    }
                    return Err(fail("No se pudo limitar la vida de los procesos de la terminal.".into(), console, in_w, out_r));
                }
                ResumeThread(pi.thread);
                CloseHandle(pi.thread);

                let unacked = Arc::new(AtomicU64::new(0));
                let closing = Arc::new(AtomicBool::new(false));
                let (input, output, process, pending, leaving) = (Handle(in_w), Handle(out_r), Handle(pi.process), unacked.clone(), closing.clone());
                let (tx, rx) = sync_channel::<Vec<u8>>(256);
                let _ = std::thread::Builder::new().name("lfind-term-write".into()).spawn(move || {
                    let input = input;
                    for data in rx {
                        let mut at = 0usize;
                        while at < data.len() {
                            let mut wrote = 0u32;
                            if WriteFile(input.0, data[at..].as_ptr(), (data.len() - at) as u32, &mut wrote, null_mut()) == 0 {
                                break;
                            }
                            at += wrote as usize;
                        }
                    }
                    CloseHandle(input.0);
                });
                let reader = std::thread::Builder::new().name("lfind-term-read".into()).spawn(move || {
                    let output = output;
                    let mut buf = vec![0u8; READ_BUF];
                    loop {
                        // Al cerrar hay que seguir leyendo: la pseudoconsola no se cierra mientras le quede salida por entregar.
                        while pending.load(Ordering::Relaxed) > MAX_UNACKED && !leaving.load(Ordering::Relaxed) {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        let mut got = 0u32;
                        if ReadFile(output.0, buf.as_mut_ptr(), READ_BUF as u32, &mut got, null_mut()) == 0 || got == 0 {
                            break;
                        }
                        pending.fetch_add(got as u64, Ordering::Relaxed);
                        on_output(buf[..got as usize].to_vec());
                    }
                    CloseHandle(output.0);
                }).map_err(|e| e.to_string())?;
                // El final del shell no cierra la tubería de salida (la mantiene la pseudoconsola): hay que esperar al propio proceso.
                let waiter = std::thread::Builder::new().name("lfind-term-wait".into()).spawn(move || {
                    let process = process;
                    WaitForSingleObject(process.0, INFINITE);
                    std::thread::sleep(Duration::from_millis(60)); // lo último que escribió llega antes que el aviso
                    let mut code = 0u32;
                    GetExitCodeProcess(process.0, &mut code);
                    on_exit(code);
                }).map_err(|e| e.to_string())?;
                Ok(Session { console: Handle(console), job: Handle(job), process: Handle(pi.process), writer: Some(tx), reader: Some(reader), waiter: Some(waiter), unacked, closing, closed: false })
            }
        }

        pub fn write(&self, data: Vec<u8>) -> Result<(), String> {
            self.writer.as_ref().ok_or("La terminal está cerrada.")?.send(data).map_err(|_| "La terminal está cerrada.".to_string())
        }

        pub fn resize(&self, cols: u16, rows: u16) {
            unsafe { ResizePseudoConsole(self.console.0, coord(cols, rows)) };
        }

        /// La interfaz ya pintó `bytes`.
        pub fn ack(&self, bytes: u64) {
            let mut cur = self.unacked.load(Ordering::Relaxed);
            while let Err(now) = self.unacked.compare_exchange_weak(cur, cur.saturating_sub(bytes), Ordering::Relaxed, Ordering::Relaxed) {
                cur = now;
            }
        }

        pub fn process_ids(&self) -> Vec<u32> {
            #[repr(C)]
            struct List {
                assigned: u32,
                listed: u32,
                ids: [usize; 256],
            }
            let mut l: List = unsafe { zeroed() };
            let ok = unsafe { QueryInformationJobObject(self.job.0, JOB_PROCESS_LIST, &mut l as *mut _ as *mut c_void, size_of::<List>() as u32, null_mut()) } != 0;
            if ok { l.ids[..(l.listed as usize).min(256)].iter().map(|p| *p as u32).collect() } else { vec![] }
        }

        /// ¿Hay algo ejecutándose además del propio shell?
        pub fn busy(&self) -> bool {
            self.process_ids().len() > 1
        }

        pub fn close(&mut self) {
            if std::mem::replace(&mut self.closed, true) {
                return;
            }
            self.writer = None;
            self.closing.store(true, Ordering::Relaxed);
            unsafe {
                TerminateJobObject(self.job.0, 0);
                ClosePseudoConsole(self.console.0);
            }
            for t in [self.reader.take(), self.waiter.take()].into_iter().flatten() {
                let _ = t.join();
            }
            unsafe {
                CloseHandle(self.process.0);
                CloseHandle(self.job.0);
            }
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            self.close();
        }
    }
}

/// Fuera de Windows no hay pseudoconsola: la orden falla con un mensaje claro.
#[cfg(not(windows))]
pub struct Session;

#[cfg(not(windows))]
impl Session {
    pub fn start(_s: Shell, _c: &Path, _a: u16, _b: u16, _o: OnOutput, _e: OnExit) -> Result<Session, String> {
        Err("La terminal integrada solo está disponible en Windows.".into())
    }
    pub fn write(&self, _d: Vec<u8>) -> Result<(), String> {
        Ok(())
    }
    pub fn resize(&self, _a: u16, _b: u16) {}
    pub fn ack(&self, _b: u64) {}
    pub fn busy(&self) -> bool {
        false
    }
    pub fn close(&mut self) {}
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    struct T {
        s: Session,
        text: Arc<Mutex<String>>,
        exit: Receiver<u32>,
    }

    fn open(command: Option<&str>, dir: &Path) -> T {
        let text = Arc::new(Mutex::new(String::new()));
        let t = text.clone();
        let (tx, exit) = channel();
        let out: OnOutput = Box::new(move |d| t.lock().unwrap().push_str(&String::from_utf8_lossy(&d)));
        let end: OnExit = Box::new(move |c| {
            let _ = tx.send(c);
        });
        let cmd = Shell::Cmd.command_line();
        let s = Session::spawn(command.unwrap_or(&cmd), dir, 80, 25, out, end).unwrap();
        T { s, text, exit }
    }

    fn wait(t: &T, needle: &str) -> bool {
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            let found = {
                let g = t.text.lock().unwrap();
                g.contains(needle)
            };
            if found || Instant::now() > end {
                return found;
            }
            std::thread::sleep(Duration::from_millis(30));
            t.s.ack(1 << 30);
        }
    }

    fn send(t: &T, s: &str) {
        t.s.write(s.as_bytes().to_vec()).unwrap();
    }

    fn alive(pid: u32) -> bool {
        let out = std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    }

    #[test]
    fn runs_commands_in_the_requested_folder() {
        let dir = std::env::temp_dir();
        let t = open(None, &dir);
        send(&t, "echo marca-lfind-%CD%\r\n");
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert!(wait(&t, &format!("marca-lfind-{}", dir.to_string_lossy().trim_end_matches('\\'))) || wait(&t, &name));
    }

    #[test]
    fn supports_interactive_input_and_reports_the_exit_code() {
        let t = open(None, &std::env::temp_dir());
        send(&t, "set /p NOMBRE=dime:\r\n");
        assert!(wait(&t, "dime:"));
        send(&t, "lfind-respuesta\r\n");
        send(&t, "echo recibido-%NOMBRE%\r\n");
        assert!(wait(&t, "recibido-lfind-respuesta"));
        send(&t, "exit 7\r\n");
        assert_eq!(t.exit.recv_timeout(Duration::from_secs(10)).unwrap(), 7);
    }

    #[test]
    fn reports_errors_of_the_shell() {
        let t = open(None, &std::env::temp_dir());
        send(&t, "esto-no-existe-lfind\r\n");
        assert!(wait(&t, "esto-no-existe-lfind"));
        send(&t, "echo codigo-%ERRORLEVEL%\r\n");
        assert!(wait(&t, "codigo-9009"));
    }

    #[test]
    fn resizing_reaches_the_console() {
        let t = open(None, &std::env::temp_dir());
        t.s.resize(132, 40);
        std::thread::sleep(Duration::from_millis(150));
        send(&t, "mode con\r\n");
        assert!(wait(&t, "132"));
    }

    #[test]
    fn both_shells_report_their_folder_invisibly() {
        let dir = std::env::temp_dir();
        let folder = dir.to_string_lossy().trim_end_matches('\\').to_string();
        for shell in [Shell::Cmd, Shell::Powershell] {
            let t = open(Some(&shell.command_line()), &dir);
            assert!(wait(&t, &format!("]9;9;{folder}")), "{shell:?}: {}", t.text.lock().unwrap());
        }
    }

    #[test]
    fn powershell_starts() {
        let t = open(Some(&Shell::Powershell.command_line()), &std::env::temp_dir());
        send(&t, "Write-Output (1234+4321)\r\n");
        assert!(wait(&t, "5555"));
    }

    #[test]
    fn closing_ends_the_shell_and_everything_it_started() {
        let mut t = open(None, &std::env::temp_dir());
        assert!(!t.s.busy());
        send(&t, "ping -n 120 127.0.0.1\r\n");
        let end = Instant::now() + Duration::from_secs(10);
        while !t.s.busy() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(t.s.busy(), "ping debe figurar como proceso hijo");
        let pids = t.s.process_ids();
        assert!(pids.len() >= 2);
        t.s.close();
        std::thread::sleep(Duration::from_millis(300));
        for p in pids {
            assert!(!alive(p), "el proceso {p} sigue vivo");
        }
        assert!(t.s.write(b"x".to_vec()).is_err());
    }

    #[test]
    fn unpainted_output_stops_the_reader_instead_of_filling_memory() {
        let t = open(None, &std::env::temp_dir());
        send(&t, "for /L %i in (1,1,200000) do @echo linea-de-relleno-para-llenar-el-buffer-%i\r\n");
        // Se llena hasta el tope y ahí se queda, por mucho que el comando siga escribiendo.
        let (mut seen, mut quiet) = (0usize, 0);
        let end = Instant::now() + Duration::from_secs(40);
        while quiet < 8 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(250));
            let now = t.text.lock().unwrap().len();
            quiet = if now == seen && now > (1 << 20) { quiet + 1 } else { 0 };
            seen = now;
        }
        assert_eq!(quiet, 8, "sin confirmación de la interfaz, deja de leer: {seen}");
        assert!(seen < (3 << 20), "{seen}");
    }

    #[test]
    fn a_missing_folder_falls_back_to_the_user_folder() {
        assert!(start_dir(Some("Z:\\no\\existe")).is_dir());
        assert_eq!(start_dir(Some("C:\\")), PathBuf::from("C:\\"));
        assert!(start_dir(None).is_dir());
    }
}
