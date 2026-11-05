mod commands;
mod icons_cmd;
mod state;
mod thumbs_cmd;

use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::window::{Effect, EffectsBuilder};
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::default())
        .setup(|app| {
            // Miniaturas: caché en la carpeta de caché local de la app (no en Roaming).
            let thumbs_dir = app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir().join("nova-explorer")).join("thumbs");
            app.manage(thumbs_cmd::ThumbState(std::sync::Arc::new(nova_core::thumbs::ThumbService::with_defaults(thumbs_dir))));
            // Iconos del sistema: su caché en disco (solo iconos propios de archivo) va junto a la de miniaturas.
            let icons_dir = app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir().join("nova-explorer")).join("icons");
            app.manage(icons_cmd::IconState(std::sync::Arc::new(nova_core::icons::IconService::with_defaults(icons_dir))));
            // Mica solo existe en Windows 11. Si el sistema lo rechaza, la interfaz
            // usa superficies opacas (ver `get_platform_info`).
            if let Some(win) = app.get_webview_window("main") {
                let ok = win
                    .set_effects(EffectsBuilder::new().effect(Effect::Mica).build())
                    .is_ok();
                app.state::<state::AppState>().vibrancy.store(ok, Ordering::Relaxed);

                // La ventana nace oculta (tauri.conf.json) y la interfaz la muestra cuando
                // ya tiene contenido. Red de seguridad: si por cualquier motivo la interfaz
                // no lo hace (error de JS, etc.), se muestra igualmente pasados 6 s.
                let fallback = win.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(6));
                    if !fallback.is_visible().unwrap_or(true) {
                        let _ = fallback.show();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_dir,
            commands::cancel_dir,
            commands::cancel_job,
            commands::start_op,
            commands::resolve_conflict,
            commands::rename_path,
            commands::create_folder,
            commands::create_file,
            commands::peek_undo,
            commands::undo_last,
            commands::open_path,
            commands::reveal_path,
            commands::create_shortcut,
            commands::get_properties,
            commands::start_stats,
            commands::get_drive_usage,
            thumbs_cmd::get_thumbnail,
            thumbs_cmd::set_thumb_epoch,
            thumbs_cmd::clear_thumb_cache,
            thumbs_cmd::get_text_preview,
            icons_cmd::get_icons,
            icons_cmd::clear_icon_cache,
            commands::get_places,
            commands::get_platform_info,
            commands::get_prefs,
            commands::set_prefs,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar LFind");
}
