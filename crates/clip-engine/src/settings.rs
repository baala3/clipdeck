//! User settings, persisted as a human-editable TOML file.
//!
//! The Settings Window and a developer hand-editing the file are equally valid
//! ways to change settings, so writes preserve whatever comments and layout the
//! file already has, and the app re-reads the file when it changes on disk.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub history_hotkey: String,
    pub pinned_hotkey: String,
    pub history_capacity: usize,
    pub excluded_apps: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            history_hotkey: "CommandOrControl+Shift+V".into(),
            pinned_hotkey: "CommandOrControl+Shift+P".into(),
            history_capacity: crate::DEFAULT_HISTORY_CAPACITY,
            excluded_apps: Vec::new(),
        }
    }
}

impl Settings {
    /// The subset of settings the core engine enforces. Hotkeys are the app
    /// shell's concern, not the engine's.
    pub fn engine_config(&self) -> crate::EngineConfig {
        crate::EngineConfig {
            history_capacity: self.history_capacity,
            excluded_apps: self.excluded_apps.iter().cloned().collect(),
        }
    }

    pub fn validate(&self) -> Result<(), SettingsError> {
        if !(1..=crate::MAX_HISTORY_CAPACITY).contains(&self.history_capacity) {
            return Err(SettingsError::Invalid(format!(
                "history_capacity must be between 1 and {} (got {})",
                crate::MAX_HISTORY_CAPACITY,
                self.history_capacity
            )));
        }
        if self.history_hotkey.eq_ignore_ascii_case(&self.pinned_hotkey) {
            return Err(SettingsError::Invalid(format!(
                "history_hotkey and pinned_hotkey must be different (both are {:?})",
                self.history_hotkey
            )));
        }
        Ok(())
    }
}

const NEW_FILE_HEADER: &str = "\
# Clipdeck settings.
# Edit this file directly or use the Settings window - both stay in sync.
";

#[derive(Debug)]
pub enum SettingsError {
    Io(std::io::Error),
    Invalid(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::Io(err) => write!(f, "{err}"),
            SettingsError::Invalid(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<std::io::Error> for SettingsError {
    fn from(err: std::io::Error) -> Self {
        SettingsError::Io(err)
    }
}

pub struct SettingsFile {
    path: PathBuf,
    /// The file contents as of our last read or write, so a poll can tell a
    /// hand edit apart from the app's own save.
    last_seen: Option<String>,
}

impl SettingsFile {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            last_seen: None,
        }
    }

    /// Reads the settings file, creating it with the defaults if it doesn't exist yet.
    pub fn load(&mut self) -> Result<Settings, SettingsError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => {
                let parsed = parse(&text);
                self.last_seen = Some(text);
                parsed
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let settings = Settings::default();
                self.save(&settings)?;
                Ok(settings)
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Writes `settings` back to disk, keeping any comments and formatting the
    /// file already has. The write goes through a temp file and a rename so a
    /// reader never sees a half-written file.
    pub fn save(&mut self, settings: &Settings) -> Result<(), SettingsError> {
        settings.validate()?;
        let existing = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok());
        let is_new_file = existing.is_none();
        let mut doc = existing.unwrap_or_default();
        set_value(&mut doc, "history_hotkey", settings.history_hotkey.as_str().into());
        set_value(&mut doc, "pinned_hotkey", settings.pinned_hotkey.as_str().into());
        set_value(&mut doc, "history_capacity", (settings.history_capacity as i64).into());
        set_value(
            &mut doc,
            "excluded_apps",
            settings.excluded_apps.iter().collect::<toml_edit::Array>().into(),
        );

        let tmp_path = self.path.with_extension("toml.tmp");
        let text = if is_new_file {
            format!("{NEW_FILE_HEADER}{doc}")
        } else {
            doc.to_string()
        };
        std::fs::write(&tmp_path, &text)?;
        std::fs::rename(&tmp_path, &self.path)?;
        self.last_seen = Some(text);
        Ok(())
    }

    /// Meant to be polled. Returns `None` while the file still holds what we last
    /// read or wrote, and the freshly parsed result once someone else changes it.
    /// A missing file is ignored here, since some editors briefly delete a file
    /// while saving it.
    pub fn reload_if_changed(&mut self) -> Option<Result<Settings, SettingsError>> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        if self.last_seen.as_deref() == Some(text.as_str()) {
            return None;
        }
        let parsed = parse(&text);
        self.last_seen = Some(text);
        Some(parsed)
    }
}

/// Replaces a top-level value while keeping the comments/whitespace around it.
fn set_value(doc: &mut toml_edit::DocumentMut, key: &str, mut value: toml_edit::Value) {
    match doc.get_mut(key).and_then(|item| item.as_value_mut()) {
        Some(existing) => {
            *value.decor_mut() = existing.decor().clone();
            *existing = value;
        }
        None => doc[key] = toml_edit::Item::Value(value),
    }
}

fn parse(text: &str) -> Result<Settings, SettingsError> {
    let settings: Settings =
        toml::from_str(text).map_err(|err| SettingsError::Invalid(err.to_string()))?;
    settings.validate()?;
    Ok(settings)
}
