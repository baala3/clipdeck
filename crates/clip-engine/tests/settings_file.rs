use clip_engine::{Settings, SettingsFile};

fn settings_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("settings.toml")
}

#[test]
fn loading_a_missing_settings_file_creates_it_with_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);

    let settings = SettingsFile::new(&path).load().unwrap();

    assert_eq!(
        settings,
        Settings {
            history_hotkey: "CommandOrControl+Shift+V".into(),
            pinned_hotkey: "CommandOrControl+Shift+P".into(),
            history_capacity: 200,
            excluded_apps: vec![],
        }
    );
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.starts_with("# Clipdeck settings."), "{written}");
    assert!(written.contains("history_capacity = 200"), "{written}");
}

#[test]
fn saving_from_the_settings_window_keeps_comments_a_developer_wrote_by_hand() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);
    std::fs::write(
        &path,
        "# my dotfiles copy\nhistory_capacity = 500 # plenty\nexcluded_apps = [\"KeePass.exe\"]\n",
    )
    .unwrap();
    let mut file = SettingsFile::new(&path);
    let mut settings = file.load().unwrap();
    assert_eq!(settings.history_capacity, 500);
    assert_eq!(settings.excluded_apps, vec!["KeePass.exe".to_string()]);

    settings.history_capacity = 750;
    file.save(&settings).unwrap();

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("# my dotfiles copy"), "{written}");
    assert!(written.contains("history_capacity = 750 # plenty"), "{written}");
    assert_eq!(SettingsFile::new(&path).load().unwrap(), settings);
}

#[test]
fn a_history_capacity_outside_1_to_2000_is_rejected_with_a_readable_reason() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);

    for bad in ["0", "2001"] {
        std::fs::write(&path, format!("history_capacity = {bad}\n")).unwrap();
        let err = SettingsFile::new(&path).load().unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("history_capacity must be between 1 and 2000 (got {bad})")
        );
    }
}

#[test]
fn the_history_and_pinned_popups_cannot_share_a_hotkey() {
    let settings = Settings {
        history_hotkey: "Ctrl+Shift+V".into(),
        pinned_hotkey: "ctrl+shift+v".into(),
        ..Settings::default()
    };

    assert_eq!(
        settings.validate().unwrap_err().to_string(),
        "history_hotkey and pinned_hotkey must be different (both are \"Ctrl+Shift+V\")"
    );
}

#[test]
fn reload_if_changed_reports_hand_edits_but_not_the_apps_own_saves() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_path(&dir);
    let mut file = SettingsFile::new(&path);
    let mut settings = file.load().unwrap();
    assert!(file.reload_if_changed().is_none());

    settings.history_capacity = 300;
    file.save(&settings).unwrap();
    assert!(file.reload_if_changed().is_none());

    std::fs::write(&path, "history_capacity = 42\n").unwrap();
    let reloaded = file.reload_if_changed().unwrap().unwrap();
    assert_eq!(reloaded.history_capacity, 42);
    assert!(file.reload_if_changed().is_none());

    std::fs::write(&path, "history_capacity = \"lots\"\n").unwrap();
    assert!(file.reload_if_changed().unwrap().is_err());
    assert!(file.reload_if_changed().is_none(), "a broken file is reported once, not every poll");
}
