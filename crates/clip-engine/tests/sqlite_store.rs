use clip_engine::{Clip, ClipContent, ClipStore, PinnedStore, SqliteClipStore, SqlitePinnedStore};

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

#[test]
fn sqlite_pinned_store_persists_clips_across_separate_connections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinned.sqlite3");
    let path_str = path.to_str().unwrap();

    {
        let mut store = SqlitePinnedStore::open(path_str);
        store.push(Clip {
            content: ClipContent::Text("hello".into()),
            source_app: Some("Notes".into()),
        });
    }

    let store = SqlitePinnedStore::open(path_str);
    let all = store.all();

    assert_eq!(all.len(), 1);
    assert_eq!(all[0].content, ClipContent::Text("hello".into()));
    assert_eq!(all[0].source_app, Some("Notes".into()));
}

#[test]
fn sqlite_pinned_store_removes_by_display_order_index_not_row_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinned.sqlite3");
    let path_str = path.to_str().unwrap();
    let mut store = SqlitePinnedStore::open(path_str);
    store.push(Clip {
        content: ClipContent::Text("first".into()),
        source_app: None,
    });
    store.push(Clip {
        content: ClipContent::Text("second".into()),
        source_app: None,
    });

    store.remove(0);

    let all = store.all();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].content, ClipContent::Text("second".into()));
}

#[test]
fn sqlite_store_removes_the_clip_at_a_chronological_index() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clips.sqlite3");
    let mut store = SqliteClipStore::open(path.to_str().unwrap());
    for text in ["one", "two", "three"] {
        store.push(Clip {
            content: ClipContent::Text(text.into()),
            source_app: None,
        });
    }

    store.remove(1);

    let texts: Vec<ClipContent> = store.all().into_iter().map(|c| c.content).collect();
    assert_eq!(
        texts,
        vec![
            ClipContent::Text("one".into()),
            ClipContent::Text("three".into())
        ]
    );
}

#[test]
fn sqlite_pinned_store_reports_whether_it_contains_some_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinned.sqlite3");
    let mut store = SqlitePinnedStore::open(path.to_str().unwrap());
    store.push(Clip {
        content: ClipContent::Text("kept".into()),
        source_app: None,
    });
    store.push(Clip {
        content: ClipContent::Image(vec![1, 2, 3]),
        source_app: None,
    });

    assert!(store.contains(&ClipContent::Text("kept".into())));
    assert!(!store.contains(&ClipContent::Text("other".into())));
    assert!(store.contains(&ClipContent::Image(vec![1, 2, 3])));
    assert!(!store.contains(&ClipContent::Image(vec![9])));
}

#[test]
fn sqlite_pinned_store_replaces_a_clip_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinned.sqlite3");
    let mut store = SqlitePinnedStore::open(path.to_str().unwrap());
    for text in ["one", "two", "three"] {
        store.push(Clip {
            content: ClipContent::Text(text.into()),
            source_app: None,
        });
    }

    store.replace(
        1,
        Clip {
            content: ClipContent::Text("TWO".into()),
            source_app: None,
        },
    );

    let texts: Vec<ClipContent> = store.all().into_iter().map(|c| c.content).collect();
    assert_eq!(
        texts,
        vec![
            ClipContent::Text("one".into()),
            ClipContent::Text("TWO".into()),
            ClipContent::Text("three".into())
        ]
    );
}
