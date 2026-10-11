//! Commands behind the Settings Window's Pinned section, where pinned items
//! are written, edited, and removed (Clipy's "Edit Snippets" equivalent), and
//! behind its editor for a History Clip.

use crate::{clip_menu, AppState};
use base64::prelude::{Engine as _, BASE64_STANDARD};
use clip_engine::menu::image_title;
use clip_engine::ClipContent;
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct PinnedItem {
    /// Chronological index in the Pinned list, which the other commands take.
    index: usize,
    /// A text item's text. An image item has `image` instead.
    text: Option<String>,
    image: Option<PinnedImage>,
}

#[derive(Serialize)]
pub struct PinnedImage {
    /// What the menus call it, e.g. "Image 640 \u{d7} 480".
    title: String,
    /// A `data:` URL of the thumbnail, or `None` if its file is missing.
    thumbnail: Option<String>,
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
        .map(|(index, clip)| match clip.content {
            ClipContent::Text(text) => PinnedItem {
                index,
                text: Some(text),
                image: None,
            },
            ClipContent::Image(image) => PinnedItem {
                index,
                text: None,
                image: Some(PinnedImage {
                    title: image_title(&image),
                    thumbnail: engine.image_thumbnail_png(&image).map(|png| {
                        format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png))
                    }),
                }),
            },
        })
        .collect())
}

#[tauri::command]
pub async fn add_pinned(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> Result<(), String> {
    let added = state.engine.lock().unwrap().pin_text(text);
    clip_menu::refresh_tray_menu(&app);
    added
        .then_some(())
        .ok_or_else(|| "A pinned item can't be blank.".into())
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
        .ok_or_else(|| "A pinned item needs some text, and a pinned image can't be edited.".into())
}

#[tauri::command]
pub async fn remove_pinned(
    app: AppHandle,
    state: State<'_, AppState>,
    index: usize,
) -> Result<(), String> {
    state.engine.lock().unwrap().unpin(index);
    clip_menu::refresh_tray_menu(&app);
    Ok(())
}

/// Rewrites the History Clip that holds `original`. It's found by its text,
/// not by an index, because History moves on while the editor is open.
#[tauri::command]
pub async fn edit_history_clip(
    app: AppHandle,
    state: State<'_, AppState>,
    original: String,
    text: String,
) -> Result<(), String> {
    let edited = {
        let mut engine = state.engine.lock().unwrap();
        let original = ClipContent::Text(original);
        let index = engine
            .history()
            .iter()
            .position(|clip| clip.content == original)
            .ok_or("That Clip is no longer in History.")?;
        engine.update_history(index, text)
    };
    clip_menu::refresh_tray_menu(&app);
    edited
        .then_some(())
        .ok_or_else(|| "A Clip can't be blank.".into())
}
