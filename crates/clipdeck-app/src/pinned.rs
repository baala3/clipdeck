//! Commands behind the Settings Window's Pinned section, where pinned items
//! are written, edited, and removed (Clipy's "Edit Snippets" equivalent).

use crate::{clip_menu, AppState};
use clip_engine::ClipContent;
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct PinnedItem {
    /// Chronological index in the Pinned list, which the other commands take.
    index: usize,
    text: String,
}

/// Newest first, matching the menus.
#[tauri::command]
pub async fn list_pinned(state: State<'_, AppState>) -> Result<Vec<PinnedItem>, String> {
    let engine = state.engine.lock().unwrap();
    Ok(engine
        .pinned()
        .into_iter()
        .enumerate()
        .rev()
        .filter_map(|(index, clip)| match clip.content {
            ClipContent::Text(text) => Some(PinnedItem { index, text }),
            ClipContent::Image(_) => None,
        })
        .collect())
}

#[tauri::command]
pub async fn add_pinned(app: AppHandle, state: State<'_, AppState>, text: String) -> Result<(), String> {
    let added = state.engine.lock().unwrap().pin_text(text);
    clip_menu::refresh_tray_menu(&app);
    added.then_some(()).ok_or_else(|| "A pinned item can't be blank.".into())
}

#[tauri::command]
pub async fn edit_pinned(
    app: AppHandle,
    state: State<'_, AppState>,
    index: usize,
    text: String,
) -> Result<(), String> {
    let edited = state.engine.lock().unwrap().update_pinned(index, text);
    clip_menu::refresh_tray_menu(&app);
    edited
        .then_some(())
        .ok_or_else(|| "A pinned item can't be blank.".into())
}

#[tauri::command]
pub async fn remove_pinned(app: AppHandle, state: State<'_, AppState>, index: usize) -> Result<(), String> {
    state.engine.lock().unwrap().unpin(index);
    clip_menu::refresh_tray_menu(&app);
    Ok(())
}
