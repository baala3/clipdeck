#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;

use clip_engine::{ClipContent, ClipEngine, ClipboardSource, SqliteClipStore, SqlitePinnedStore};
use serde::Serialize;
use settings::{Hotkeys, SettingsState, SETTINGS_WINDOW};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, State, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::ShortcutState;

/// Spurious blur events can fire right after `window.show()` + `window.set_focus()`
/// (a known Tauri/WebView2 race), so hide-on-blur ignores any blur within this
/// window of a deliberate show.
const SHOW_GRACE_PERIOD: Duration = Duration::from_millis(400);

/// One row in a popup. `id` is the Clip's chronological index in the engine
/// (0 = oldest), which is what every engine call takes, so the frontend never
/// has to convert display positions back into engine indices.
#[derive(Serialize, Clone)]
struct ClipDto {
    id: usize,
    text: String,
    pinned: bool,
}

struct AppState {
    engine: Mutex<Engine>,
    paused: AtomicBool,
    shown_at: Mutex<HashMap<String, Instant>>,
    settings: Mutex<SettingsState>,
    /// The hotkeys currently in effect, read by the global shortcut handler.
    hotkeys: Mutex<Hotkeys>,
    /// Whatever window was focused right before a popup was shown (Windows
    /// HWND as a raw value; 0 means none captured). Restored and pasted into
    /// after a selection, so picking a Clip pastes it directly.
    previous_foreground: Mutex<isize>,
}

type Engine = ClipEngine<SqliteClipStore, SqlitePinnedStore>;

/// Text Clips, newest first, as popup rows. Images aren't shown in the popups yet.
fn text_rows(clips: Vec<clip_engine::Clip>, is_pinned: impl Fn(&ClipContent) -> bool) -> Vec<ClipDto> {
    let mut rows: Vec<ClipDto> = clips
        .into_iter()
        .enumerate()
        .filter_map(|(id, clip)| {
            let pinned = is_pinned(&clip.content);
            match clip.content {
                ClipContent::Text(text) => Some(ClipDto { id, text, pinned }),
                ClipContent::Image(_) => None,
            }
        })
        .collect();
    rows.reverse();
    rows
}

fn clip_text(clips: Vec<clip_engine::Clip>, id: usize) -> Option<String> {
    match clips.into_iter().nth(id)?.content {
        ClipContent::Text(text) => Some(text),
        ClipContent::Image(_) => None,
    }
}

#[tauri::command]
fn get_history(state: State<AppState>) -> Vec<ClipDto> {
    let engine = state.engine.lock().unwrap();
    let pinned: Vec<ClipContent> = engine.pinned().into_iter().map(|clip| clip.content).collect();
    text_rows(engine.history(), |content| pinned.contains(content))
}

#[tauri::command]
fn get_pinned(state: State<AppState>) -> Vec<ClipDto> {
    let engine = state.engine.lock().unwrap();
    text_rows(engine.pinned(), |_| true)
}

fn copy_and_hide(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    state: &AppState,
    text: Option<String>,
) {
    if let Some(text) = text {
        if let Err(err) = app.clipboard().write_text(text) {
            eprintln!("copy_and_hide: failed to write clipboard: {err}");
        }
    }
    let _ = window.hide();

    #[cfg(windows)]
    {
        let handle = *state.previous_foreground.lock().unwrap();
        clip_windows::focus_window_and_paste(handle);
    }
    #[cfg(not(windows))]
    {
        let _ = state;
    }
}

/// Selecting a History Clip also moves it to the top, so it's first next time.
#[tauri::command]
fn select_clip(app: tauri::AppHandle, window: tauri::WebviewWindow, state: State<AppState>, id: usize) {
    let text = {
        let mut engine = state.engine.lock().unwrap();
        let text = clip_text(engine.history(), id);
        if text.is_some() {
            engine.promote(id);
        }
        text
    };
    copy_and_hide(&app, &window, &state, text);
}

#[tauri::command]
fn select_pinned(app: tauri::AppHandle, window: tauri::WebviewWindow, state: State<AppState>, id: usize) {
    let text = {
        let engine = state.engine.lock().unwrap();
        clip_text(engine.pinned(), id)
    };
    copy_and_hide(&app, &window, &state, text);
}

#[tauri::command]
fn pin_clip(state: State<AppState>, id: usize) -> bool {
    state.engine.lock().unwrap().pin(id)
}

#[tauri::command]
fn unpin_clip(state: State<AppState>, id: usize) -> bool {
    state.engine.lock().unwrap().unpin(id)
}

/// Pins text the user wrote in the Pinned popup, rather than copied.
#[tauri::command]
fn pin_text(state: State<AppState>, text: String) -> bool {
    state.engine.lock().unwrap().pin_text(text)
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
            #[cfg(windows)]
            {
                *state.previous_foreground.lock().unwrap() = clip_windows::foreground_window();
            }
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            get_history,
            get_pinned,
            select_clip,
            select_pinned,
            pin_clip,
            unpin_clip,
            pin_text,
            close_popup,
            settings::get_settings,
            settings::update_settings,
            settings::set_hotkeys_suspended
        ])
        .setup(|app| {
            let settings_state = SettingsState::load(settings::settings_file_path(app));
            let store = SqliteClipStore::open("clipdeck-history.sqlite3");
            let pinned_store = SqlitePinnedStore::open("clipdeck-pinned.sqlite3");
            let engine = ClipEngine::new(store, pinned_store, settings_state.current().engine_config());
            let hotkeys = Hotkeys::parse(settings_state.current())
                .expect("SettingsState::load only keeps settings with valid hotkeys");
            app.manage(AppState {
                engine: Mutex::new(engine),
                paused: AtomicBool::new(false),
                shown_at: Mutex::new(HashMap::new()),
                settings: Mutex::new(settings_state),
                hotkeys: Mutex::new(hotkeys),
                previous_foreground: Mutex::new(0),
            });

            #[cfg(windows)]
            spawn_capture_thread(app.handle().clone(), clip_windows::WindowsClipboardSource::new());
            #[cfg(target_os = "macos")]
            spawn_capture_thread(app.handle().clone(), clip_macos::MacClipboardSource::new());

            let settings_item = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let pause_item =
                CheckMenuItem::with_id(app, "pause", "Pause capture", true, false, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &pause_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().expect("default window icon missing"))
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "settings" => settings::show_settings_window(app),
                    "quit" => app.exit(0),
                    "pause" => {
                        let state = app.state::<AppState>();
                        let current = state.paused.load(Ordering::SeqCst);
                        state.paused.store(!current, Ordering::SeqCst);
                    }
                    _ => {}
                })
                .build(app)?;

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, shortcut, event| {
                        if event.state() != ShortcutState::Pressed {
                            return;
                        }
                        let hotkeys = *app.state::<AppState>().hotkeys.lock().unwrap();
                        if shortcut == &hotkeys.history {
                            toggle_window(app, "history");
                        } else if shortcut == &hotkeys.pinned {
                            toggle_window(app, "pinned");
                        }
                    })
                    .build(),
            )?;
            // A hotkey another app already owns shouldn't stop Clipdeck starting;
            // the user can pick a different one in Settings.
            if let Err(err) = settings::register_initial_hotkeys(app.handle()) {
                eprintln!("{err}");
            }
            settings::spawn_file_watcher(app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing Settings just hides it, so the app keeps running in the tray.
            WindowEvent::CloseRequested { api, .. } if window.label() == SETTINGS_WINDOW => {
                api.prevent_close();
                let _ = window.hide();
            }
            // Only the popups are ephemeral; the Settings Window stays put when it loses focus.
            WindowEvent::Focused(false) if window.label() != SETTINGS_WINDOW => {
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
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
