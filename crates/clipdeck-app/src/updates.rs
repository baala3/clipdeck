//! Auto-updates from GitHub Releases. Release builds check shortly after
//! launch and then daily; the Settings Window can also check on demand.
//! Installing always asks first, since it restarts the app.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Default)]
pub struct UpdateState {
    /// One check (and its prompt) at a time.
    checking: AtomicBool,
    /// A version the user said "Later" to. Background checks don't ask about
    /// it again this session; a check from Settings still does.
    declined: Mutex<Option<String>>,
}

pub fn spawn_background_checks(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        loop {
            if let Err(err) = tauri::async_runtime::block_on(check(&app, false)) {
                eprintln!("update check failed: {err}");
            }
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

/// What a check from the Settings Window found, shown next to its button.
#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<String, String> {
    match check(&app, true).await? {
        Outcome::UpToDate => Ok(format!(
            "Clipdeck {} is the latest version.",
            app.package_info().version
        )),
        Outcome::Declined(version) => Ok(format!("Clipdeck {version} is available.")),
        Outcome::Busy => Ok("Already checking for updates.".into()),
    }
}

enum Outcome {
    UpToDate,
    Declined(String),
    Busy,
}

async fn check(app: &AppHandle, asked_by_user: bool) -> Result<Outcome, String> {
    let state = app.state::<UpdateState>();
    if state.checking.swap(true, Ordering::SeqCst) {
        return Ok(Outcome::Busy);
    }
    let result = check_and_offer(app, &state, asked_by_user).await;
    state.checking.store(false, Ordering::SeqCst);
    result
}

async fn check_and_offer(
    app: &AppHandle,
    state: &UpdateState,
    asked_by_user: bool,
) -> Result<Outcome, String> {
    let update = app
        .updater()
        .map_err(|err| err.to_string())?
        .check()
        .await
        .map_err(|err| format!("Couldn't check for updates: {err}"))?;
    let Some(update) = update else {
        return Ok(Outcome::UpToDate);
    };
    let declined = state.declined.lock().unwrap().clone();
    if !asked_by_user && declined.as_deref() == Some(update.version.as_str()) {
        return Ok(Outcome::Declined(update.version));
    }

    let mut message = format!(
        "Clipdeck {} is available (you have {}).\n\nInstall it now? Clipdeck restarts to finish.",
        update.version, update.current_version
    );
    if let Some(notes) = update
        .body
        .as_deref()
        .map(str::trim)
        .filter(|notes| !notes.is_empty())
    {
        message.push_str(&format!("\n\nWhat's new:\n{notes}"));
    }
    let dialog_app = app.clone();
    let install = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .message(message)
            .title("Update available")
            .kind(MessageDialogKind::Info)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Install and Restart".into(),
                "Later".into(),
            ))
            .blocking_show()
    })
    .await
    .unwrap_or(false);
    if !install {
        *state.declined.lock().unwrap() = Some(update.version.clone());
        return Ok(Outcome::Declined(update.version));
    }

    // On Windows this hands over to the installer, which exits Clipdeck and
    // starts the new version itself.
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|err| format!("Couldn't install the update: {err}"))?;
    app.restart();
}
