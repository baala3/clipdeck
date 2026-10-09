#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod clip_menu;
mod pinned;
mod settings;
mod updates;

use clip_engine::{
    adopt_legacy_store, ClipEngine, ClipboardSource, Settings, SqliteClipStore, SqlitePinnedStore,
};
use clip_menu::{MenuKind, MenuTargets};
use settings::{HotkeyAction, Hotkeys, SettingsState, SETTINGS_WINDOW};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

type Engine = ClipEngine<SqliteClipStore, SqlitePinnedStore>;

struct AppState {
    engine: Mutex<Engine>,
    paused: AtomicBool,
    settings: Mutex<SettingsState>,
    /// A copy of the settings in effect, for code that must never wait on the
    /// `settings` lock (menus are built on the event loop while a settings
    /// change may be holding that lock and waiting on the event loop itself).
    active_settings: Mutex<Settings>,
    /// The hotkeys currently in effect, read by the global shortcut handler.
    hotkeys: Mutex<Hotkeys>,
    /// Whatever window was focused right before a popup menu opened (Windows
    /// HWND as a raw value; 0 means none captured). Focus goes back there, and
    /// a chosen Clip is pasted into it.
    #[cfg(windows)]
    previous_foreground: Mutex<isize>,
    menu_targets: Mutex<MenuTargets>,
    /// A popup menu is modal; a hotkey pressed while one is open is ignored.
    menu_open: AtomicBool,
}

fn spawn_capture_thread(app: tauri::AppHandle, mut source: impl ClipboardSource + Send + 'static) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let state = app.state::<AppState>();
        if state.paused.load(Ordering::SeqCst) {
            continue;
        }
        let mut captured = false;
        while let Some(incoming) = source.next_event() {
            state.engine.lock().unwrap().capture(incoming);
            captured = true;
        }
        if captured {
            clip_menu::refresh_tray_menu(&app);
        }
    });
}

fn on_hotkey(app: &tauri::AppHandle, action: HotkeyAction) {
    match action {
        HotkeyAction::MainMenu => clip_menu::request_popup(app, MenuKind::Main),
        HotkeyAction::HistoryMenu => clip_menu::request_popup(app, MenuKind::History),
        HotkeyAction::PinnedMenu => clip_menu::request_popup(app, MenuKind::Pinned),
        HotkeyAction::ClearHistory => clip_menu::confirm_and_clear_history(app),
    }
}

/// Where the store `file_name` lives: the per-user local data dir, which is
/// machine-local (unlike the roaming settings file) since History can hold
/// large images. Older versions kept stores in the working directory, which
/// for a double-clicked exe is its own folder; those are moved over once.
fn store_path(app: &tauri::App, file_name: &str) -> PathBuf {
    let data_dir = app
        .path()
        .app_local_data_dir()
        .expect("no app data directory on this OS");
    let legacy_dirs: Vec<PathBuf> = [
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from)),
        std::env::current_dir().ok(),
    ]
    .into_iter()
    .flatten()
    .collect();
    adopt_legacy_store(&data_dir, file_name, &legacy_dirs).unwrap_or_else(|err| {
        eprintln!("{err}");
        err.legacy
    })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::update_settings,
            settings::set_hotkeys_suspended,
            pinned::list_pinned,
            pinned::add_pinned,
            pinned::edit_pinned,
            pinned::remove_pinned,
            updates::check_for_updates
        ])
        .setup(|app| {
            // A menu bar app: no Dock icon or app menu, just the tray icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings_state = SettingsState::load(settings::settings_file_path(app));
            let store = SqliteClipStore::open(store_path(app, "clipdeck-history.sqlite3"));
            let pinned_store = SqlitePinnedStore::open(store_path(app, "clipdeck-pinned.sqlite3"));
            let current = settings_state.current().clone();
            let current_launch_at_login = current.launch_at_login;
            let engine = ClipEngine::new(store, pinned_store, current.engine_config());
            let hotkeys = Hotkeys::parse(&current)
                .expect("SettingsState::load only keeps settings with valid hotkeys");
            app.manage(AppState {
                engine: Mutex::new(engine),
                paused: AtomicBool::new(false),
                settings: Mutex::new(settings_state),
                active_settings: Mutex::new(current),
                hotkeys: Mutex::new(hotkeys),
                #[cfg(windows)]
                previous_foreground: Mutex::new(0),
                menu_targets: Mutex::new(MenuTargets::default()),
                menu_open: AtomicBool::new(false),
            });

            #[cfg(windows)]
            spawn_capture_thread(
                app.handle().clone(),
                clip_windows::WindowsClipboardSource::new(),
            );
            #[cfg(target_os = "macos")]
            spawn_capture_thread(app.handle().clone(), clip_macos::MacClipboardSource::new());

            // Like Clipy, clicking the tray icon shows the full menu.
            // The tray gets its own simplified icon, drawn to stay legible at 16px.
            TrayIconBuilder::with_id(clip_menu::TRAY_ID)
                .icon(tauri::include_image!("icons/tray.png"))
                .tooltip("Clipdeck")
                .menu(&clip_menu::build_initial_tray_menu(app.handle())?)
                .build(app)?;

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, shortcut, event| {
                        if event.state() != ShortcutState::Pressed {
                            return;
                        }
                        let action = app
                            .state::<AppState>()
                            .hotkeys
                            .lock()
                            .unwrap()
                            .action_for(shortcut);
                        if let Some(action) = action {
                            on_hotkey(app, action);
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
            // Re-registering on every launch keeps the login item pointing at
            // this exe even if the app was moved or reinstalled elsewhere.
            if let Err(err) = autostart::sync(current_launch_at_login) {
                eprintln!("{err}");
            }
            updates::spawn_background_checks(app.handle().clone());

            Ok(())
        })
        .on_menu_event(|app, event| clip_menu::handle_menu_event(app, event.id.as_ref()))
        .on_window_event(|window, event| {
            // Closing Settings just hides it, so the app keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == SETTINGS_WINDOW {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
