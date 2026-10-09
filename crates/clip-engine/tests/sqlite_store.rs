use clip_engine::{Clip, ClipContent, ClipStore, SqliteClipStore};

#[test]
fn sqlite_store_persists_clips_across_separate_connections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clips.sqlite3");
    let path_str = path.to_str().unwrap();

    {
        let mut store = SqliteClipStore::open(path_str);
        store.push(Clip {
            content: ClipContent::Text("hello".into()),
            source_app: Some("Notes".into()),
        });
    }

    let store = SqliteClipStore::open(path_str);
    let all = store.all();

    assert_eq!(all.len(), 1);
    assert_eq!(all[0].content, ClipContent::Text("hello".into()));
    assert_eq!(all[0].source_app, Some("Notes".into()));
}
