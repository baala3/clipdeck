#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clip_engine::{
    ClipContent, ClipEngine, ClipboardSource, EngineConfig, SqliteClipStore, SqlitePinnedStore,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, State, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Spurious blur events can fire right after `window.show()` + `window.set_focus()`
/// (a known Tauri/WebView2 race), so hide-on-blur ignores any blur within this
/// window of a deliberate show.
const SHOW_GRACE_PERIOD: Duration = Duration::from_millis(400);

#[derive(Serialize, Clone)]
struct ClipDto {
    text: String,
}

struct AppState {
    engine: Mutex<ClipEngine<SqliteClipStore, SqlitePinnedStore>>,
    paused: AtomicBool,
    shown_at: Mutex<HashMap<String, Instant>>,
}

fn as_text_dtos(clips: Vec<clip_engine::Clip>) -> Vec<ClipDto> {
    clips
        .into_iter()
        .filter_map(|clip| match clip.content {
            ClipContent::Text(text) => Some(ClipDto { text }),
            ClipContent::Image(_) => None,
        })
        .collect()
}

fn text_history(engine: &ClipEngine<SqliteClipStore, SqlitePinnedStore>) -> Vec<ClipDto> {
    let mut history = engine.history();
    history.reverse();
    as_text_dtos(history)
}

fn text_pinned(engine: &ClipEngine<SqliteClipStore, SqlitePinnedStore>) -> Vec<ClipDto> {
    let mut pinned = engine.pinned();
    pinned.reverse();
    as_text_dtos(pinned)
}

#[tauri::command]
fn get_history(state: State<AppState>) -> Vec<ClipDto> {
    let engine = state.engine.lock().unwrap();
    text_history(&engine)
}

#[tauri::command]
fn get_pinned(state: State<AppState>) -> Vec<ClipDto> {
    let engine = state.engine.lock().unwrap();
    text_pinned(&engine)
}

fn copy_and_hide(app: &tauri::AppHandle, window: &tauri::WebviewWindow, text: Option<String>) {
    if let Some(text) = text {
        if let Err(err) = app.clipboard().write_text(text) {
            eprintln!("copy_and_hide: failed to write clipboard: {err}");
        }
    }
    let _ = window.hide();
}

#[tauri::command]
fn select_clip(app: tauri::AppHandle, window: tauri::WebviewWindow, state: State<AppState>, index: usize) {
    let text = {
        let engine = state.engine.lock().unwrap();
        text_history(&engine).get(index).map(|c| c.text.clone())
    };
    copy_and_hide(&app, &window, text);
}

#[tauri::command]
fn select_pinned(app: tauri::AppHandle, window: tauri::WebviewWindow, state: State<AppState>, index: usize) {
    let text = {
        let engine = state.engine.lock().unwrap();
        text_pinned(&engine).get(index).map(|c| c.text.clone())
    };
    copy_and_hide(&app, &window, text);
}

/// `index` is into the display order the History popup shows (most recent first),
/// so it must be converted to the chronological-order index the engine expects.
#[tauri::command]
fn pin_clip(state: State<AppState>, index: usize) -> bool {
    let mut engine = state.engine.lock().unwrap();
    let history_len = engine.history().len();
    if index >= history_len {
        return false;
    }
    engine.pin(history_len - 1 - index)
}

#[tauri::command]
fn unpin_clip(state: State<AppState>, index: usize) -> bool {
    let mut engine = state.engine.lock().unwrap();
    let pinned_len = engine.pinned().len();
    if index >= pinned_len {
        return false;
    }
    engine.unpin(pinned_len - 1 - index)
}

#[tauri::command]
fn close_popup(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

fn spawn_capture_thread(app: tauri::AppHandle, mut source: impl ClipboardSource + Send + 'static) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let state = app.state::<AppState>();
        if state.paused.load(Ordering::SeqCst) {
            continue;
        }
        while let Some(incoming) = source.next_event() {
            let mut engine = state.engine.lock().unwrap();
            engine.capture(incoming);
        }
    });
}

fn toggle_window(app: &tauri::AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let currently_visible = window.is_visible().unwrap_or(false);
        if currently_visible {
            let _ = window.hide();
        } else {
            let state = app.state::<AppState>();
            state
                .shown_at
                .lock()
                .unwrap()
                .insert(label.to_string(), Instant::now());
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn main() {
    let store = SqliteClipStore::open("clipdeck-history.sqlite3");
    let pinned_store = SqlitePinnedStore::open("clipdeck-pinned.sqlite3");
    let engine = ClipEngine::new(store, pinned_store, EngineConfig::default());
    let state = AppState {
        engine: Mutex::new(engine),
        paused: AtomicBool::new(false),
        shown_at: Mutex::new(HashMap::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_history,
            get_pinned,
            select_clip,
            select_pinned,
            pin_clip,
            unpin_clip,
            close_popup
        ])
        .setup(|app| {
            #[cfg(windows)]
            spawn_capture_thread(app.handle().clone(), clip_windows::WindowsClipboardSource::new());
            #[cfg(target_os = "macos")]
            spawn_capture_thread(app.handle().clone(), clip_macos::MacClipboardSource::new());

            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let pause_item =
                CheckMenuItem::with_id(app, "pause", "Pause capture", true, false, None::<&str>)?;
            let menu = Menu::with_items(app, &[&pause_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().expect("default window icon missing"))
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "pause" => {
                        let state = app.state::<AppState>();
                        let current = state.paused.load(Ordering::SeqCst);
                        state.paused.store(!current, Ordering::SeqCst);
                    }
                    _ => {}
                })
                .build(app)?;

            let history_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyV);
            let pinned_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP);
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, shortcut, event| {
                        if event.state() != ShortcutState::Pressed {
                            return;
                        }
                        if shortcut == &history_shortcut {
                            toggle_window(app, "history");
                        } else if shortcut == &pinned_shortcut {
                            toggle_window(app, "pinned");
                        }
                    })
                    .build(),
            )?;
            app.global_shortcut().register(history_shortcut)?;
            app.global_shortcut().register(pinned_shortcut)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                let state = window.state::<AppState>();
                let recently_shown = state
                    .shown_at
                    .lock()
                    .unwrap()
                    .get(window.label())
                    .is_some_and(|shown_at| shown_at.elapsed() < SHOW_GRACE_PERIOD);
                if !recently_shown {
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
