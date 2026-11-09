//! Comandos del analizador de almacenamiento (Update 10). La lógica vive en `nova_core::analyzer`; las consultas van a hilos de
//! trabajo para no retener nunca la interfaz. Los informes están en su propia carpeta, separados del índice de búsqueda.

use nova_core::analyzer::report::{AnalyzeEvent, Analyzer, DirHit, DirsQuery, ExtStat, FileHit, FilesQuery, Issue, Page, ReportInfo, TreeNode};
use nova_core::analyzer::walk::CatStat;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

pub struct AnalyzerState(pub Arc<Analyzer>);

pub fn init(app: &AppHandle) -> AnalyzerState {
    let dir = app.path().app_local_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("nova-explorer")).join("analyzer");
    AnalyzerState(Arc::new(Analyzer::new(dir)))
}

async fn blocking<R: Send + 'static>(f: impl FnOnce() -> Result<R, String> + Send + 'static) -> Result<R, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn analyzer_start(root: String, on_event: Channel<AnalyzeEvent>, state: State<'_, AnalyzerState>) -> Result<(), String> {
    let a = state.0.clone();
    // Si la interfaz ya no escucha, el análisis se cancela: nada sigue trabajando para nadie.
    let weak = Arc::downgrade(&a);
    blocking(move || {
        a.start(&root, Box::new(move |e| {
            if on_event.send(e).is_err() {
                if let Some(a) = weak.upgrade() {
                    a.cancel();
                }
            }
        }))
    })
    .await
}

/// Actualiza el informe releyendo solo lo que cambió.
#[tauri::command]
pub async fn analyzer_update(root: String, on_event: Channel<AnalyzeEvent>, state: State<'_, AnalyzerState>) -> Result<(), String> {
    let a = state.0.clone();
    blocking(move || a.update(&root, None, Box::new(move |e| { let _ = on_event.send(e); }))).await
}

/// Vigila una ubicación con informe y lo actualiza en segundo plano cuando cesa la actividad.
#[tauri::command]
pub async fn analyzer_watch(root: String, on_event: Channel<AnalyzeEvent>, state: State<'_, AnalyzerState>) -> Result<(), String> {
    let a = state.0.clone();
    blocking(move || a.watch(&root, Arc::new(move |e| { let _ = on_event.send(e); }))).await
}

#[tauri::command]
pub fn analyzer_unwatch(state: State<'_, AnalyzerState>) {
    state.0.unwatch();
}

#[tauri::command]
pub fn analyzer_pause(state: State<'_, AnalyzerState>) {
    state.0.pause();
}

#[tauri::command]
pub fn analyzer_resume(state: State<'_, AnalyzerState>) {
    state.0.resume();
}

#[tauri::command]
pub fn analyzer_cancel(state: State<'_, AnalyzerState>) {
    state.0.cancel();
}

#[tauri::command]
pub async fn analyzer_reports(state: State<'_, AnalyzerState>) -> Result<Vec<ReportInfo>, String> {
    let a = state.0.clone();
    blocking(move || Ok(a.reports())).await
}

#[tauri::command]
pub async fn analyzer_report(root: String, state: State<'_, AnalyzerState>) -> Result<Option<ReportInfo>, String> {
    let a = state.0.clone();
    blocking(move || Ok(a.report(&root))).await
}

#[tauri::command]
pub async fn analyzer_forget(root: String, state: State<'_, AnalyzerState>) -> Result<(), String> {
    let a = state.0.clone();
    blocking(move || a.forget(&root)).await
}

#[tauri::command]
pub async fn analyzer_node(root: String, at: String, state: State<'_, AnalyzerState>) -> Result<Option<DirHit>, String> {
    let a = state.0.clone();
    blocking(move || a.node(&root, &at)).await
}

#[tauri::command]
pub async fn analyzer_dirs(root: String, query: DirsQuery, state: State<'_, AnalyzerState>) -> Result<Page<DirHit>, String> {
    let a = state.0.clone();
    blocking(move || a.dirs(&root, &query)).await
}

#[tauri::command]
pub async fn analyzer_files(root: String, query: FilesQuery, state: State<'_, AnalyzerState>) -> Result<Page<FileHit>, String> {
    let a = state.0.clone();
    blocking(move || a.files(&root, &query)).await
}

#[tauri::command]
pub async fn analyzer_categories(root: String, dir: i64, avail: Option<String>, state: State<'_, AnalyzerState>) -> Result<Vec<CatStat>, String> {
    let a = state.0.clone();
    blocking(move || a.categories_for(&root, dir, avail.as_deref())).await
}

#[tauri::command]
pub async fn analyzer_availability(root: String, dir: i64, state: State<'_, AnalyzerState>) -> Result<Vec<nova_core::analyzer::report::AvailStat>, String> {
    let a = state.0.clone();
    blocking(move || a.availability(&root, dir)).await
}

#[tauri::command]
pub async fn analyzer_exts(root: String, dir: i64, offset: u32, limit: u32, avail: Option<String>, state: State<'_, AnalyzerState>) -> Result<Page<ExtStat>, String> {
    let a = state.0.clone();
    blocking(move || a.exts_for(&root, dir, offset, limit, avail.as_deref())).await
}

#[tauri::command]
pub async fn analyzer_treemap(root: String, dir: i64, state: State<'_, AnalyzerState>) -> Result<Vec<TreeNode>, String> {
    let a = state.0.clone();
    blocking(move || a.treemap(&root, dir)).await
}

#[tauri::command]
pub async fn analyzer_issues(root: String, state: State<'_, AnalyzerState>) -> Result<Vec<Issue>, String> {
    let a = state.0.clone();
    blocking(move || a.issues(&root)).await
}
