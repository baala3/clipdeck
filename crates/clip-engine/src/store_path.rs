//! Where the History and Pinned databases live.
//!
//! Older versions opened them relative to the working directory, so launching
//! the app from anywhere but its own folder started an empty History. They now
//! live in a fixed per-user data dir, and a store an older version left behind
//! is moved there the first time it's found.

use std::io;
use std::path::{Path, PathBuf};

/// Why a legacy store couldn't be moved into the data dir. The caller should
/// open it where it is for now, so no Clips go missing, and the move is
/// retried on the next launch.
#[derive(Debug)]
pub struct LegacyStoreError {
    pub legacy: PathBuf,
    reason: String,
}

impl std::fmt::Display for LegacyStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Couldn't move {} into the data dir ({}), so it's used where it is for now.",
            self.legacy.display(),
            self.reason
        )
    }
}

impl std::error::Error for LegacyStoreError {}

/// Returns the path to open the store `file_name` at: `data_dir/file_name`,
/// creating `data_dir` if needed. If that file doesn't exist yet but one does
/// in a `legacy_dirs` entry (checked in order), it's moved into `data_dir`
/// first. An existing store in `data_dir` is never replaced.
pub fn adopt_legacy_store(
    data_dir: &Path,
    file_name: &str,
    legacy_dirs: &[PathBuf],
) -> Result<PathBuf, LegacyStoreError> {
    let dest = data_dir.join(file_name);
    // Without a legacy store to fall back on, a data dir that can't be created
    // is reported by opening the store there.
    let created_dir = std::fs::create_dir_all(data_dir);
    if dest.exists() {
        return Ok(dest);
    }
    let Some(legacy) = legacy_dirs
        .iter()
        .map(|dir| dir.join(file_name))
        .find(|candidate| candidate.is_file())
    else {
        return Ok(dest);
    };
    let fail = |reason: String| LegacyStoreError {
        legacy: legacy.clone(),
        reason,
    };
    created_dir.map_err(|err| fail(format!("can't create {}: {err}", data_dir.display())))?;
    // SQLite needs a leftover rollback journal next to its database to undo a
    // half-done write, so leave the pair alone until opening it in place has
    // rolled the journal back.
    if journal_path(&legacy).exists() {
        return Err(fail("it has an unfinished transaction".into()));
    }
    move_file(&legacy, &dest).map_err(|err| fail(err.to_string()))?;
    Ok(dest)
}

fn journal_path(db: &Path) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push("-journal");
    PathBuf::from(name)
}

/// Renames `from` to `to`, falling back to copy-then-delete across volumes
/// (e.g. the app unzipped onto another drive). The copy is written under a
/// temp name and renamed into place, so `to` is never half-written.
fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Err(err) if err.kind() == io::ErrorKind::CrossesDevices => {
            let mut tmp = to.as_os_str().to_owned();
            tmp.push(".tmp");
            let tmp = PathBuf::from(tmp);
            std::fs::copy(from, &tmp)?;
            std::fs::rename(&tmp, to)?;
            // The data is safe in its new home; a stale copy left behind is
            // harmless, since `to` now exists and wins from here on.
            let _ = std::fs::remove_file(from);
            Ok(())
        }
        result => result,
    }
}
