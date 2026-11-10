mod analyzer_cmd;
mod backgrounds_cmd;
mod clipboard_cmd;
mod commands;
mod shellapps_cmd;
mod terminal_cmd;
mod windows_cmd;
mod duplicates_cmd;
mod inspector_cmd;
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
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| windows_cmd::on_second_instance(app, args)))
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
            app.manage(analyzer_cmd::init(app.handle()));
            app.manage(terminal_cmd::TerminalState::default());
            app.manage(windows_cmd::StartupPath::default());
            windows_cmd::remember_startup(app.handle());
            app.manage(duplicates_cmd::init(app.handle()));
            app.manage(backgrounds_cmd::init(app.handle()));
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
            commands::pause_job,
            commands::rename_path,
            commands::create_folder,
            commands::create_file,
            commands::peek_undo,
            commands::undo_last,
            commands::open_path,
            commands::reveal_path,
            commands::create_shortcut,
            commands::resolve_shortcut,
            commands::get_properties,
            commands::start_stats,
            commands::get_drive_usage,
            commands::list_fonts,
            inspector_cmd::inspect_path,
            terminal_cmd::term_open,
            terminal_cmd::term_write,
            terminal_cmd::term_resize,
            terminal_cmd::term_ack,
            terminal_cmd::term_busy,
            terminal_cmd::term_close,
            windows_cmd::take_startup_path,
            windows_cmd::win_status,
            windows_cmd::win_set,
            windows_cmd::win_restore,
            windows_cmd::win_repair,
            shellapps_cmd::open_with_list,
            shellapps_cmd::open_with_app,
            shellapps_cmd::start_native_drag,
            shellapps_cmd::recycle_list,
            shellapps_cmd::recycle_restore,
            shellapps_cmd::recycle_purge,
            shellapps_cmd::recycle_cancel,
            analyzer_cmd::analyzer_availability,
            duplicates_cmd::dup_start,
            duplicates_cmd::dup_pause,
            duplicates_cmd::dup_resume,
            duplicates_cmd::dup_pending,
            duplicates_cmd::dup_discard,
            duplicates_cmd::dup_suspend,
            duplicates_cmd::dup_cancel,
            duplicates_cmd::dup_default_exclusions,
            duplicates_cmd::dup_summary,
            duplicates_cmd::dup_groups,
            duplicates_cmd::dup_check_removal,
            duplicates_cmd::dup_prune,
            duplicates_cmd::dup_forget,
            duplicates_cmd::dup_compare,
            inspector_cmd::start_hash,
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
            backgrounds_cmd::background_import,
            backgrounds_cmd::background_load,
            backgrounds_cmd::background_retain,
            analyzer_cmd::analyzer_start,
            analyzer_cmd::analyzer_update,
            analyzer_cmd::analyzer_watch,
            analyzer_cmd::analyzer_unwatch,
            analyzer_cmd::analyzer_pause,
            analyzer_cmd::analyzer_resume,
            analyzer_cmd::analyzer_cancel,
            analyzer_cmd::analyzer_reports,
            analyzer_cmd::analyzer_report,
            analyzer_cmd::analyzer_forget,
            analyzer_cmd::analyzer_node,
            analyzer_cmd::analyzer_dirs,
            analyzer_cmd::analyzer_files,
            analyzer_cmd::analyzer_categories,
            analyzer_cmd::analyzer_exts,
            analyzer_cmd::analyzer_treemap,
            analyzer_cmd::analyzer_issues,
            commands::get_places,
            commands::get_cloud_places,
            commands::get_drives,
            commands::drives_mask,
            commands::check_paths,
            commands::cloud_only,
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
