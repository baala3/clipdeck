//! Commands behind the Settings Window's History and Pinned pages, which list
//! every Clip in their list and offer the same things on each: pinned items
//! are written, edited, and removed there (Clipy's "Edit Snippets"
//! equivalent), and History Clips edited, pinned, unpinned, and removed.

use crate::{clip_menu, AppState, Engine};
use base64::prelude::{Engine as _, BASE64_STANDARD};
use clip_engine::menu::image_title;
use clip_engine::{Clip, ClipContent};
use serde::Serialize;
use tauri::{AppHandle, State};

/// A Clip as either page lists it.
#[derive(Serialize)]
pub struct ListItem {
    /// Chronological index in its list, which the other commands take.
    index: usize,
    /// A text item's text. An image item has `image` instead.
    text: Option<String>,
    image: Option<ListImage>,
    /// Whether it's in the Pinned list too; always true for a pinned item.
    pinned: bool,
}

#[derive(Serialize)]
pub struct ListImage {
    /// What the menus call it, e.g. "Image 640 \u{d7} 480".
    title: String,
    /// A `data:` URL of the thumbnail, or `None` if its file is missing.
    thumbnail: Option<String>,
}

/// `clips` as listed, newest first to match the menus.
fn listed(engine: &Engine, clips: Vec<Clip>) -> Vec<ListItem> {
    clips
        .into_iter()
        .enumerate()
        .rev()
        .map(|(index, clip)| {
            let pinned = engine.is_pinned(&clip.content);
            match clip.content {
                ClipContent::Text(text) => ListItem {
                    index,
                    text: Some(text),
                    image: None,
                    pinned,
                },
                ClipContent::Image(image) => ListItem {
                    index,
                    text: None,
                    image: Some(ListImage {
                        title: image_title(&image),
                        thumbnail: engine.image_thumbnail_png(&image).map(|png| {
                            format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png))
                        }),
                    }),
                    pinned,
                },
            }
        })
        .collect()
}

#[tauri::command]
pub async fn list_pinned(state: State<'_, AppState>) -> Result<Vec<ListItem>, String> {
    let engine = state.engine.lock().unwrap();
    Ok(listed(&engine, engine.pinned()))
}

#[tauri::command]
pub async fn list_history(state: State<'_, AppState>) -> Result<Vec<ListItem>, String> {
    let engine = state.engine.lock().unwrap();
    Ok(listed(&engine, engine.history()))
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
        let index = history_index(&engine, &ClipContent::Text(original))
            .ok_or("That Clip is no longer in History.")?;
        engine.update_history(index, text)
    };
    clip_menu::refresh_tray_menu(&app);
    edited
        .then_some(())
        .ok_or_else(|| "A Clip can't be blank.".into())
}

fn history_index(engine: &Engine, content: &ClipContent) -> Option<usize> {
    engine
        .history()
        .iter()
        .position(|clip| &clip.content == content)
}

/// The History Clip at `index` (chronological), if it's still there.
fn history_clip(engine: &Engine, index: usize) -> Result<ClipContent, String> {
    engine
        .history()
        .into_iter()
        .nth(index)
        .map(|clip| clip.content)
        .ok_or_else(|| "That Clip is no longer in History.".into())
}

#[tauri::command]
pub async fn remove_history_clip(
    app: AppHandle,
    state: State<'_, AppState>,
    index: usize,
) -> Result<(), String> {
    state.engine.lock().unwrap().delete(index);
    clip_menu::refresh_tray_menu(&app);
    Ok(())
}

/// Pins the History Clip at `index`, or unpins it if it's pinned already.
#[tauri::command]
pub async fn toggle_history_pin(
    app: AppHandle,
    state: State<'_, AppState>,
    index: usize,
) -> Result<(), String> {
    {
        let mut engine = state.engine.lock().unwrap();
        let content = history_clip(&engine, index)?;
        let pinned_at = engine
            .pinned()
            .iter()
            .position(|clip| clip.content == content);
        match pinned_at {
            Some(pinned_index) => engine.unpin(pinned_index),
            None => engine.pin(index),
        };
    }
    clip_menu::refresh_tray_menu(&app);
    Ok(())
}

/// Asks before clearing History, as the menu's Clear History does.
#[tauri::command]
pub fn clear_history(app: AppHandle) {
    clip_menu::confirm_and_clear_history(&app);
}
