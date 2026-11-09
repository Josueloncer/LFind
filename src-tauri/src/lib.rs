mod clipboard_cmd;
mod commands;
mod icons_cmd;
mod index_cmd;
mod state;
mod system_cmd;
mod thumbs_cmd;
mod update_cmd;
mod viewer_cmd;

use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::window::{Effect, EffectsBuilder};
use tauri::Manager;

pub fn run() {
    nova_core::diagnostics::install_panic_hook();
    tauri::Builder::default()
        .manage(state::AppState::default())
        .manage(update_cmd::UpdateState::default())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Contenido para el visor: solo archivos con permiso emitido por `viewer_open`.
        .register_asynchronous_uri_scheme_protocol(viewer_cmd::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn_blocking(move || responder.respond(viewer_cmd::handle(&app, &request)));
        })
        .setup(|app| {
            // Registro de errores: lo primero, para que cubra también el resto del arranque.
            if let Ok(dir) = app.path().app_log_dir() {
                nova_core::diagnostics::set_log_path(dir.join("lfind-errors.log"));
            }
            // Miniaturas: caché en la carpeta de caché local de la app (no en Roaming).
            let thumbs_dir = app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir().join("nova-explorer")).join("thumbs");
            let thumbs = std::sync::Arc::new(nova_core::thumbs::ThumbService::with_defaults(thumbs_dir));
            app.manage(thumbs_cmd::ThumbState(thumbs.clone()));
            let viewer_dir = app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir().join("nova-explorer")).join("viewer-v1");
            app.manage(viewer_cmd::ViewerState(std::sync::Arc::new(nova_core::viewer::ViewerService::new(viewer_dir, thumbs))));
            // Iconos del sistema: su caché en disco (solo iconos propios de archivo) va junto a la de miniaturas.
            let icons_dir = app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir().join("nova-explorer")).join("icons");
            // Índice de búsqueda global: se abre ahora y empieza a indexar en segundo plano poco después.
            let index_state = index_cmd::init(app.handle());
            app.manage(index_state);
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
            commands::resolve_path,
            thumbs_cmd::get_thumbnail,
            thumbs_cmd::set_thumb_epoch,
            thumbs_cmd::clear_thumb_cache,
            thumbs_cmd::get_text_preview,
            icons_cmd::get_icons,
            icons_cmd::clear_icon_cache,
            clipboard_cmd::clipboard_set_files,
            clipboard_cmd::clipboard_read_files,
            clipboard_cmd::clipboard_sequence,
            clipboard_cmd::clipboard_has_files,
            system_cmd::open_with,
            system_cmd::open_terminal,
            system_cmd::compress_paths,
            system_cmd::extract_archive,
            system_cmd::list_send_to,
            system_cmd::send_to,
            system_cmd::list_new_templates,
            system_cmd::create_from_template,
            index_cmd::index_search,
            index_cmd::index_status,
            index_cmd::index_get_settings,
            index_cmd::index_set_settings,
            index_cmd::index_available_drives,
            index_cmd::index_start,
            index_cmd::index_pause,
            index_cmd::index_resume,
            index_cmd::index_rebuild,
            commands::get_places,
            commands::get_platform_info,
            commands::get_prefs,
            commands::set_prefs,
            commands::log_ui_error,
            viewer_cmd::viewer_open,
            viewer_cmd::viewer_close,
            update_cmd::app_version,
            update_cmd::update_check,
            update_cmd::update_download,
            update_cmd::update_cancel,
            update_cmd::update_restart,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar LFind");
}
