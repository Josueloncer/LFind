//! Archivos soltados desde otras aplicaciones (Explorador, WinRAR, el escritorio…).
//!
//! El destino OLE de la ventana es el de WebView2, que vive en el proceso del navegador: LFind no puede sustituirlo sin romper
//! el arrastre interno. La página recibe el `drop` HTML5 con `dataTransfer.files`, que no lleva rutas; la vía oficial de
//! WebView2 es reenviar esos objetos a la aplicación con `chrome.webview.postMessageWithAdditionalObjects`, que los entrega como
//! `ICoreWebView2File` con su ruta real. WinRAR, al soltar, extrae la selección a una carpeta temporal (`%TEMP%\Rar$DRa…`) y
//! esas son las rutas que llegan; el archivo comprimido nunca se toca.

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct ExternalDrop {
    /// Identifica el `drop` de la página que lo pidió (la carpeta de destino la decidió ella).
    pub token: String,
    pub paths: Vec<String>,
    /// Qué tipo de objeto llegó en cada posición («file», «handle» u «other»): para explicar un fallo sin adivinar.
    pub kinds: Vec<&'static str>,
}

/// Prefijo del mensaje de la página. Lo demás que llegue por `postMessage` no es asunto de este módulo.
pub const PREFIX: &str = "lfind-drop:";

#[cfg(windows)]
pub fn install(win: &tauri::WebviewWindow) {
    use tauri::{Emitter, Manager};
    let app = win.app_handle().clone();
    let _ = win.with_webview(move |wv| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2File, ICoreWebView2FileSystemHandle, ICoreWebView2WebMessageReceivedEventArgs2};
        use webview2_com::WebMessageReceivedEventHandler;
        use windows_core::{Interface, PWSTR};
        #[link(name = "ole32")]
        extern "system" {
            fn CoTaskMemFree(p: *mut core::ffi::c_void);
        }
        unsafe fn take(p: PWSTR) -> String {
            if p.is_null() {
                return String::new();
            }
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(p.0 as *mut _);
            s
        }
        let Ok(core) = wv.controller().CoreWebView2() else { return };
        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut text = PWSTR::null();
            if args.TryGetWebMessageAsString(&mut text).is_err() {
                return Ok(());
            }
            let text = take(text);
            let Some(token) = text.strip_prefix(PREFIX) else { return Ok(()) };
            let mut paths = Vec::new();
            let mut kinds = Vec::new();
            if let Ok(list) = args.cast::<ICoreWebView2WebMessageReceivedEventArgs2>().and_then(|a| a.AdditionalObjects()) {
                let mut n = 0u32;
                let _ = list.Count(&mut n);
                for i in 0..n {
                    let Ok(obj) = list.GetValueAtIndex(i) else { continue };
                    let mut p = PWSTR::null();
                    // Un archivo llega como `File`; una carpeta, como `FileSystemHandle`.
                    let ok = if let Ok(f) = obj.cast::<ICoreWebView2File>() {
                        kinds.push("file");
                        f.Path(&mut p).is_ok()
                    } else if let Ok(h) = obj.cast::<ICoreWebView2FileSystemHandle>() {
                        kinds.push("handle");
                        h.Path(&mut p).is_ok()
                    } else {
                        kinds.push("other");
                        false
                    };
                    if ok {
                        let s = take(p);
                        if !s.is_empty() {
                            paths.push(s);
                        }
                    }
                }
            }
            let _ = app.emit("external-drop", ExternalDrop { token: token.to_string(), paths, kinds });
            Ok(())
        }));
        let mut registration = 0i64;
        let _ = core.add_WebMessageReceived(&handler, &mut registration);

        // Al soltar algo que viene de WinRAR, WebView2 navega por su cuenta al archivo (`file:///…`) aunque la página haya
        // cancelado el `drop`, y LFind desaparecía de la ventana (medido). La interfaz nunca abre archivos locales como página.
        let guard = webview2_com::NavigationStartingEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                let mut uri = PWSTR::null();
                if args.Uri(&mut uri).is_ok() && take(uri).to_ascii_lowercase().starts_with("file:") {
                    let _ = args.SetCancel(true);
                }
            }
            Ok(())
        }));
        let mut nav_registration = 0i64;
        let _ = core.add_NavigationStarting(&guard, &mut nav_registration);
    });
}

#[cfg(not(windows))]
pub fn install(_win: &tauri::WebviewWindow) {}
