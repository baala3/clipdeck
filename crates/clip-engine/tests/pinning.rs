use clip_engine::{Clip, ClipContent, ClipEngine, ClipStore, EngineConfig, PinnedStore};

#[derive(Default)]
struct InMemoryStore {
    clips: Vec<Clip>,
}

impl ClipStore for InMemoryStore {
    fn push(&mut self, clip: Clip) {
        self.clips.push(clip);
    }

    fn evict_oldest(&mut self) {
        if !self.clips.is_empty() {
            self.clips.remove(0);
        }
    }

    fn remove(&mut self, index: usize) {
        if index < self.clips.len() {
            self.clips.remove(index);
        }
    }

    fn len(&self) -> usize {
        self.clips.len()
    }

    fn all(&self) -> Vec<Clip> {
        self.clips.clone()
    }
}

#[derive(Default)]
struct InMemoryPinnedStore {
    clips: Vec<Clip>,
}

impl PinnedStore for InMemoryPinnedStore {
    fn replace(&mut self, index: usize, clip: Clip) {
        if let Some(slot) = self.clips.get_mut(index) {
            *slot = clip;
        }
    }

    fn contains(&self, content: &ClipContent) -> bool {
        self.clips.iter().any(|clip| &clip.content == content)
    }

    fn push(&mut self, clip: Clip) {
        self.clips.push(clip);
    }

    fn remove(&mut self, index: usize) {
        if index < self.clips.len() {
            self.clips.remove(index);
        }
    }

    fn all(&self) -> Vec<Clip> {
        self.clips.clone()
    }
}

fn text_clip(text: &str) -> Clip {
    Clip {
        content: ClipContent::Text(text.into()),
        source_app: Some("TextEdit".into()),
    }
}

fn engine_with_history(clips: Vec<Clip>) -> ClipEngine<InMemoryStore, InMemoryPinnedStore> {
    let mut store = InMemoryStore::default();
    for clip in clips {
        store.push(clip);
    }
    ClipEngine::new(
        store,
        InMemoryPinnedStore::default(),
        EngineConfig::default(),
    )
}

#[test]
fn pinning_a_history_item_copies_it_into_the_pinned_list_and_keeps_the_original_in_history() {
    let mut engine = engine_with_history(vec![text_clip("hello"), text_clip("world")]);

    let pinned = engine.pin(0);

    assert!(pinned);
    assert_eq!(engine.pinned(), vec![text_clip("hello")]);
    assert_eq!(
        engine.history(),
        vec![text_clip("hello"), text_clip("world")]
    );
}

#[test]
fn pinning_an_out_of_range_history_index_does_nothing() {
    let mut engine = engine_with_history(vec![text_clip("hello")]);

    let pinned = engine.pin(5);

    assert!(!pinned);
    assert_eq!(engine.pinned(), Vec::<Clip>::new());
}

#[test]
fn unpinning_removes_only_from_the_pinned_list() {
    let mut engine = engine_with_history(vec![text_clip("hello")]);
    engine.pin(0);

    let unpinned = engine.unpin(0);

    assert!(unpinned);
    assert_eq!(engine.pinned(), Vec::<Clip>::new());
    assert_eq!(engine.history(), vec![text_clip("hello")]);
}

#[test]
fn pinned_list_has_no_capacity_cap() {
    let many_clips: Vec<Clip> = (0..5000).map(|i| text_clip(&i.to_string())).collect();
    let mut engine = engine_with_history(many_clips.clone());

    for i in 0..5000 {
        engine.pin(i);
    }

    assert_eq!(engine.pinned().len(), 5000);
}

#[test]
fn pinning_content_that_is_already_pinned_does_not_add_a_second_copy() {
    let mut engine = engine_with_history(vec![text_clip("hello"), text_clip("hello again")]);
    engine.pin(0);

    assert!(engine.pin(0));

    assert_eq!(engine.pinned(), vec![text_clip("hello")]);
    assert!(engine.is_pinned(&ClipContent::Text("hello".into())));
    assert!(!engine.is_pinned(&ClipContent::Text("hello again".into())));
}

#[test]
fn text_written_by_the_user_can_be_pinned_directly_without_touching_history() {
    let mut engine = engine_with_history(vec![text_clip("copied")]);

    assert!(engine.pin_text("my email signature\nBala".into()));
    assert!(
        !engine.pin_text("   \n ".into()),
        "blank text is not pinned"
    );

    assert_eq!(
        engine.pinned(),
        vec![Clip {
            content: ClipContent::Text("my email signature\nBala".into()),
            source_app: None,
        }]
    );
    assert_eq!(engine.history(), vec![text_clip("copied")]);
}

#[test]
fn a_pinned_item_can_be_edited_in_place() {
    let mut engine = engine_with_history(vec![]);
    engine.pin_text("first".into());
    engine.pin_text("second".into());

    assert!(engine.update_pinned(0, "first, edited".into()));
    assert!(
        !engine.update_pinned(0, "  ".into()),
        "blank text is rejected"
    );
    assert!(
        !engine.update_pinned(9, "nope".into()),
        "out of range does nothing"
    );

    let texts: Vec<ClipContent> = engine.pinned().into_iter().map(|c| c.content).collect();
    assert_eq!(
        texts,
        vec![
            ClipContent::Text("first, edited".into()),
            ClipContent::Text("second".into())
        ]
    );
}
