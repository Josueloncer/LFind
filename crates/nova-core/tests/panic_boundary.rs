//! El gancho de pánicos actúa sobre todo el proceso y aborta: se comprueba en un proceso hijo (esta misma prueba, relanzada)
//! para no interferir con el resto y poder observar cómo termina.

use nova_core::diagnostics::{install_panic_hook, isolate, set_log_path};
use std::path::PathBuf;
use std::process::Command;

const ENV_MODE: &str = "NOVA_PANIC_CHILD";
const ENV_LOG: &str = "NOVA_PANIC_LOG";

fn run_child(mode: &str) -> (std::process::Output, String) {
    let log: PathBuf = std::env::temp_dir().join(format!("nova-panic-{}-{}", mode, std::process::id())).join("e.log");
    let _ = std::fs::remove_dir_all(log.parent().unwrap());
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_scenario", "--nocapture", "--test-threads=1"])
        .env(ENV_MODE, mode)
        .env(ENV_LOG, &log)
        .output()
        .unwrap();
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_dir_all(log.parent().unwrap());
    (out, text)
}

/// No hace nada salvo cuando la lanza `run_child`.
#[test]
fn child_scenario() {
    let Ok(mode) = std::env::var(ENV_MODE) else { return };
    set_log_path(PathBuf::from(std::env::var(ENV_LOG).unwrap()));
    install_panic_hook();
    match mode.as_str() {
        "contained" => {
            let r = isolate(|| -> u8 { panic!("decodificador roto") });
            assert!(r.is_err());
            println!("SIGUE-VIVO");
        }
        "fatal" => {
            let h = std::thread::Builder::new().name("hilo-critico".into()).spawn(|| panic!("fallo critico")).unwrap();
            let _ = h.join();
            println!("SIGUE-VIVO"); // no debe llegar aquí
        }
        other => unreachable!("{other}"),
    }
}

#[test]
fn contained_panic_is_logged_and_the_process_survives() {
    let (out, log) = run_child("contained");
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("SIGUE-VIVO"));
    assert!(log.contains("\"contained\":true") && log.contains("decodificador roto"), "{log}");
}

#[test]
fn panic_outside_a_boundary_is_logged_and_terminates_the_process() {
    let (out, log) = run_child("fatal");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("SIGUE-VIVO"), "el proceso no debe continuar");
    assert!(!out.status.success());
    assert_ne!(out.status.code(), Some(101), "101 sería el fallo normal de una prueba, no un abort");
    assert!(log.contains("\"contained\":false") && log.contains("fallo critico") && log.contains("hilo-critico"), "{log}");
}
