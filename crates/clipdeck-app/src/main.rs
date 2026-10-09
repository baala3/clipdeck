#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clip_engine::{ClipContent, ClipEngine, ClipboardSource, EngineConfig, SqliteClipStore};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, State, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[derive(Serialize, Clone)]
struct ClipDto {
    text: String,
}

struct AppState {
    engine: Mutex<ClipEngine<SqliteClipStore>>,
    paused: AtomicBool,
}

fn text_history(engine: &ClipEngine<SqliteClipStore>) -> Vec<ClipDto> {
    let mut history = engine.history();
    history.reverse();
    history
        .into_iter()
        .filter_map(|clip| match clip.content {
            ClipContent::Text(text) => Some(ClipDto { text }),
            ClipContent::Image(_) => None,
        })
        .collect()
}

#[tauri::command]
fn get_history(state: State<AppState>) -> Vec<ClipDto> {
    let engine = state.engine.lock().unwrap();
    text_history(&engine)
}

#[tauri::command]
fn select_clip(app: tauri::AppHandle, state: State<AppState>, index: usize) {
    let text = {
        let engine = state.engine.lock().unwrap();
        text_history(&engine).get(index).map(|c| c.text.clone())
    };
    if let Some(text) = text {
        if let Err(err) = app.clipboard().write_text(text) {
            eprintln!("select_clip: failed to write clipboard: {err}");
        }
    }
    if let Some(window) = app.get_webview_window("history") {
        let _ = window.hide();
    }
}

#[tauri::command]
fn close_popup(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("history") {
        let _ = window.hide();
    }
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

fn main() {
    let store = SqliteClipStore::open("clipdeck-history.sqlite3");
    let engine = ClipEngine::new(store, EngineConfig::default());
    let state = AppState {
        engine: Mutex::new(engine),
        paused: AtomicBool::new(false),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![get_history, select_clip, close_popup])
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
            let shortcut_app_handle = app.handle().clone();
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |_app, shortcut, event| {
                        if shortcut == &history_shortcut && event.state() == ShortcutState::Pressed {
                            if let Some(window) = shortcut_app_handle.get_webview_window("history") {
                                let currently_visible = window.is_visible().unwrap_or(false);
                                if currently_visible {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                    })
                    .build(),
            )?;
            app.global_shortcut().register(history_shortcut)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
