use clip_engine::{
    Clip, ClipContent, ClipStore, ImageInfo, PinnedStore, SqliteClipStore, SqlitePinnedStore,
};

fn image_info(digest: &str) -> ImageInfo {
    ImageInfo {
        digest: digest.into(),
        width: 640,
        height: 480,
        byte_len: 1234,
    }
}

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
        content: ClipContent::Image(image_info("aaa")),
        source_app: None,
    });

    assert!(store.contains(&ClipContent::Text("kept".into())));
    assert!(!store.contains(&ClipContent::Text("other".into())));
    assert!(store.contains(&ClipContent::Image(image_info("aaa"))));
    assert!(!store.contains(&ClipContent::Image(image_info("bbb"))));
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

#[test]
fn sqlite_stores_persist_image_clips_with_their_size() {
    let dir = tempfile::tempdir().unwrap();
    let history_path = dir.path().join("clips.sqlite3");
    let pinned_path = dir.path().join("pinned.sqlite3");
    let clip = Clip {
        content: ClipContent::Image(image_info("aaa")),
        source_app: Some("Preview".into()),
    };
    SqliteClipStore::open(&history_path).push(clip.clone());
    SqlitePinnedStore::open(&pinned_path).push(clip.clone());

    for stored in [
        SqliteClipStore::open(&history_path).all(),
        SqlitePinnedStore::open(&pinned_path).all(),
    ] {
        assert_eq!(stored, vec![clip.clone()]);
        let ClipContent::Image(image) = &stored[0].content else {
            panic!("expected an image")
        };
        assert_eq!(
            (image.width, image.height, image.byte_len),
            (640, 480, 1234)
        );
    }
}

#[test]
fn a_store_from_before_image_capture_keeps_its_clips_and_takes_images() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clips.sqlite3");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE clips (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT,
                image BLOB,
                source_app TEXT
            );
            INSERT INTO clips (text, source_app) VALUES ('old', 'Notes');",
        )
        .unwrap();
    }

    let mut store = SqliteClipStore::open(&path);
    store.push(Clip {
        content: ClipContent::Image(image_info("aaa")),
        source_app: None,
    });

    let contents: Vec<ClipContent> = SqliteClipStore::open(&path)
        .all()
        .into_iter()
        .map(|clip| clip.content)
        .collect();
    assert_eq!(
        contents,
        vec![
            ClipContent::Text("old".into()),
            ClipContent::Image(image_info("aaa"))
        ]
    );
}
