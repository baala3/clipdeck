use clip_engine::{adopt_legacy_store, Clip, ClipContent, ClipStore, SqliteClipStore};
use std::path::PathBuf;

fn store_with(path: &std::path::Path, text: &str) {
    let mut store = SqliteClipStore::open(path);
    store.push(Clip {
        content: ClipContent::Text(text.into()),
        source_app: None,
    });
}

fn texts(path: &std::path::Path) -> Vec<ClipContent> {
    SqliteClipStore::open(path)
        .all()
        .into_iter()
        .map(|clip| clip.content)
        .collect()
}

struct Dirs {
    _root: tempfile::TempDir,
    data: PathBuf,
    exe: PathBuf,
    cwd: PathBuf,
}

fn dirs() -> Dirs {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let exe = root.path().join("exe");
    let cwd = root.path().join("cwd");
    for dir in [&exe, &cwd] {
        std::fs::create_dir_all(dir).unwrap();
    }
    Dirs {
        data,
        exe,
        cwd,
        _root: root,
    }
}

const NAME: &str = "clipdeck-history.sqlite3";

#[test]
fn with_nothing_to_adopt_the_store_lives_in_the_data_dir() {
    let d = dirs();

    let path = adopt_legacy_store(&d.data, NAME, &[d.exe.clone(), d.cwd.clone()]).unwrap();

    assert_eq!(path, d.data.join(NAME));
    assert!(d.data.is_dir(), "the data dir is created");
}

#[test]
fn a_store_left_next_to_the_exe_by_an_older_version_moves_into_the_data_dir() {
    let d = dirs();
    store_with(&d.exe.join(NAME), "kept");

    let path = adopt_legacy_store(&d.data, NAME, &[d.exe.clone(), d.cwd.clone()]).unwrap();

    assert_eq!(path, d.data.join(NAME));
    assert_eq!(texts(&path), vec![ClipContent::Text("kept".into())]);
    assert!(!d.exe.join(NAME).exists());
}

#[test]
fn the_first_candidate_with_a_store_wins() {
    let d = dirs();
    store_with(&d.exe.join(NAME), "from exe dir");
    store_with(&d.cwd.join(NAME), "from cwd");

    let path = adopt_legacy_store(&d.data, NAME, &[d.exe.clone(), d.cwd.clone()]).unwrap();

    assert_eq!(texts(&path), vec![ClipContent::Text("from exe dir".into())]);
    assert!(d.cwd.join(NAME).exists(), "other candidates are left alone");
}

#[test]
fn an_existing_store_in_the_data_dir_is_never_replaced() {
    let d = dirs();
    std::fs::create_dir_all(&d.data).unwrap();
    store_with(&d.data.join(NAME), "current");
    store_with(&d.exe.join(NAME), "stale");

    let path = adopt_legacy_store(&d.data, NAME, std::slice::from_ref(&d.exe)).unwrap();

    assert_eq!(texts(&path), vec![ClipContent::Text("current".into())]);
    assert!(d.exe.join(NAME).exists());
}

#[test]
fn a_candidate_that_is_the_data_dir_itself_is_ignored() {
    let d = dirs();
    std::fs::create_dir_all(&d.data).unwrap();
    store_with(&d.data.join(NAME), "current");

    let path = adopt_legacy_store(&d.data, NAME, std::slice::from_ref(&d.data)).unwrap();

    assert_eq!(texts(&path), vec![ClipContent::Text("current".into())]);
}

#[test]
fn a_store_with_an_unfinished_transaction_stays_put_and_is_reported() {
    let d = dirs();
    store_with(&d.exe.join(NAME), "mid-write");
    // A leftover rollback journal must stay next to its database, or SQLite
    // can't roll the half-done transaction back.
    std::fs::write(d.exe.join(format!("{NAME}-journal")), b"hot journal").unwrap();

    let err = adopt_legacy_store(&d.data, NAME, std::slice::from_ref(&d.exe)).unwrap_err();

    assert_eq!(err.legacy, d.exe.join(NAME));
    assert!(err.to_string().contains("unfinished transaction"), "{err}");
    assert!(d.exe.join(NAME).exists());
    assert!(!d.data.join(NAME).exists());
}
