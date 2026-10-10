use clip_engine::{
    CaptureOutcome, Clip, ClipContent, ClipEngine, ClipStore, ClipboardSource, EngineConfig,
    IncomingClip, PinnedStore, Settings,
};
use std::collections::VecDeque;

struct FakeSource {
    events: VecDeque<IncomingClip>,
}

impl ClipboardSource for FakeSource {
    fn next_event(&mut self) -> Option<IncomingClip> {
        self.events.pop_front()
    }
}

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

fn text_clip(text: &str) -> IncomingClip {
    IncomingClip {
        content: ClipContent::Text(text.into()),
        source_app: Some("TextEdit".into()),
        concealed: false,
    }
}

#[test]
fn default_engine_config_has_a_history_capacity_of_200() {
    let config = EngineConfig::default();
    assert_eq!(config.history_capacity, 200);
}

#[test]
fn history_capacity_is_clamped_to_the_hard_ceiling_of_2000() {
    let store = InMemoryStore::default();
    let config = EngineConfig {
        history_capacity: 5000,
        excluded_apps: Default::default(),
    };
    let engine = ClipEngine::new(store, InMemoryPinnedStore::default(), config);

    assert_eq!(engine.history_capacity(), 2000);
}

#[test]
fn history_evicts_the_oldest_clip_once_capacity_is_exceeded() {
    let store = InMemoryStore::default();
    let config = EngineConfig {
        history_capacity: 2,
        excluded_apps: Default::default(),
    };
    let mut engine = ClipEngine::new(store, InMemoryPinnedStore::default(), config);

    engine.capture(text_clip("first"));
    engine.capture(text_clip("second"));
    engine.capture(text_clip("third"));

    let history = engine.history();
    assert_eq!(history.len(), 2);
    assert!(!history
        .iter()
        .any(|c| c.content == ClipContent::Text("first".into())));
    assert!(history
        .iter()
        .any(|c| c.content == ClipContent::Text("second".into())));
    assert!(history
        .iter()
        .any(|c| c.content == ClipContent::Text("third".into())));
}

#[test]
fn clips_from_an_excluded_app_are_dropped() {
    let store = InMemoryStore::default();
    let mut excluded_apps = std::collections::HashSet::new();
    excluded_apps.insert("1Password".to_string());
    let config = EngineConfig {
        history_capacity: 200,
        excluded_apps,
    };
    let mut engine = ClipEngine::new(store, InMemoryPinnedStore::default(), config);

    let outcome = engine.capture(IncomingClip {
        content: ClipContent::Text("secret".into()),
        source_app: Some("1Password".into()),
        concealed: false,
    });

    assert_eq!(outcome, CaptureOutcome::Dropped);
    assert_eq!(engine.history().len(), 0);
}

#[test]
fn draining_a_clipboard_source_captures_each_event_in_order() {
    let store = InMemoryStore::default();
    let config = EngineConfig {
        history_capacity: 200,
        excluded_apps: Default::default(),
    };
    let mut engine = ClipEngine::new(store, InMemoryPinnedStore::default(), config);

    let mut source = FakeSource {
        events: VecDeque::from([text_clip("first"), text_clip("second")]),
    };

    engine.drain(&mut source);

    let history = engine.history();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].content, ClipContent::Text("first".into()));
    assert_eq!(history[1].content, ClipContent::Text("second".into()));
}

#[test]
fn clips_copied_while_capture_is_paused_never_reach_history_after_resuming() {
    let mut engine = ClipEngine::new(
        InMemoryStore::default(),
        InMemoryPinnedStore::default(),
        EngineConfig::default(),
    );
    let mut source = FakeSource {
        events: VecDeque::from([text_clip("copied while paused")]),
    };

    engine.discard(&mut source);
    // Capture resumes, and the user copies something else.
    source.events.push_back(text_clip("copied after resuming"));
    engine.drain(&mut source);

    assert_eq!(history_texts(&engine), vec!["copied after resuming"]);
}

#[test]
fn text_that_is_only_whitespace_is_dropped() {
    let mut engine = ClipEngine::new(
        InMemoryStore::default(),
        InMemoryPinnedStore::default(),
        EngineConfig::default(),
    );

    for blank in ["", "   ", "\r\n\t \n"] {
        assert_eq!(engine.capture(text_clip(blank)), CaptureOutcome::Dropped);
    }
    assert_eq!(engine.capture(text_clip("  x  ")), CaptureOutcome::Captured);

    assert_eq!(history_texts(&engine), vec!["  x  "]);
}

#[test]
fn clips_marked_concealed_are_dropped() {
    let store = InMemoryStore::default();
    let config = EngineConfig {
        history_capacity: 200,
        excluded_apps: Default::default(),
    };
    let mut engine = ClipEngine::new(store, InMemoryPinnedStore::default(), config);

    let outcome = engine.capture(IncomingClip {
        content: ClipContent::Text("one-time password".into()),
        source_app: Some("SomeApp".into()),
        concealed: true,
    });

    assert_eq!(outcome, CaptureOutcome::Dropped);
    assert_eq!(engine.history().len(), 0);
}

#[test]
fn capturing_a_text_clip_makes_it_appear_in_history() {
    let store = InMemoryStore::default();
    let config = EngineConfig {
        history_capacity: 200,
        excluded_apps: Default::default(),
    };
    let mut engine = ClipEngine::new(store, InMemoryPinnedStore::default(), config);

    let outcome = engine.capture(IncomingClip {
        content: ClipContent::Text("hello world".into()),
        source_app: Some("TextEdit".into()),
        concealed: false,
    });

    assert_eq!(outcome, CaptureOutcome::Captured);

    let history = engine.history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].content, ClipContent::Text("hello world".into()));
}

#[test]
fn lowering_history_capacity_at_runtime_evicts_the_oldest_clips_immediately() {
    let config = EngineConfig {
        history_capacity: 5,
        excluded_apps: Default::default(),
    };
    let mut engine = ClipEngine::new(
        InMemoryStore::default(),
        InMemoryPinnedStore::default(),
        config,
    );
    for text in ["one", "two", "three", "four"] {
        engine.capture(text_clip(text));
    }

    engine.reconfigure(EngineConfig {
        history_capacity: 2,
        excluded_apps: Default::default(),
    });

    let history = engine.history();
    assert_eq!(engine.history_capacity(), 2);
    assert_eq!(
        history
            .iter()
            .map(|c| c.content.clone())
            .collect::<Vec<_>>(),
        vec![
            ClipContent::Text("three".into()),
            ClipContent::Text("four".into())
        ]
    );
}

#[test]
fn exclusion_entries_match_source_apps_ignoring_case_and_a_windows_exe_suffix() {
    let config = EngineConfig {
        history_capacity: 200,
        excluded_apps: ["keepass".to_string(), "Bitwarden.exe".to_string()].into(),
    };
    let mut engine = ClipEngine::new(
        InMemoryStore::default(),
        InMemoryPinnedStore::default(),
        config,
    );

    let from = |app: &str| IncomingClip {
        content: ClipContent::Text("secret".into()),
        source_app: Some(app.into()),
        concealed: false,
    };

    assert_eq!(engine.capture(from("KeePass.exe")), CaptureOutcome::Dropped);
    assert_eq!(engine.capture(from("KEEPASS")), CaptureOutcome::Dropped);
    assert_eq!(engine.capture(from("bitwarden")), CaptureOutcome::Dropped);
    assert_eq!(
        engine.capture(from("KeePassXC.exe")),
        CaptureOutcome::Captured
    );
    assert_eq!(engine.history().len(), 1);
}

#[test]
fn an_engine_reconfigured_from_settings_applies_their_capacity_and_exclusions() {
    let mut engine = ClipEngine::new(
        InMemoryStore::default(),
        InMemoryPinnedStore::default(),
        EngineConfig::default(),
    );
    let settings = Settings {
        history_capacity: 1,
        excluded_apps: vec!["TextEdit".into()],
        ..Settings::default()
    };

    engine.reconfigure(settings.engine_config());

    assert_eq!(engine.history_capacity(), 1);
    assert_eq!(
        engine.capture(text_clip("from TextEdit")),
        CaptureOutcome::Dropped
    );
}

fn history_texts(engine: &ClipEngine<InMemoryStore, InMemoryPinnedStore>) -> Vec<String> {
    engine
        .history()
        .into_iter()
        .map(|clip| match clip.content {
            ClipContent::Text(text) => text,
            ClipContent::Image(_) => "<image>".into(),
        })
        .collect()
}

#[test]
fn promoting_a_clip_moves_it_to_the_newest_position_without_duplicating_it() {
    let mut engine = ClipEngine::new(
        InMemoryStore::default(),
        InMemoryPinnedStore::default(),
        EngineConfig::default(),
    );
    for text in ["one", "two", "three"] {
        engine.capture(text_clip(text));
    }

    assert!(engine.promote(0));

    assert_eq!(history_texts(&engine), vec!["two", "three", "one"]);
    assert!(!engine.promote(3), "out of range does nothing");
    assert_eq!(history_texts(&engine), vec!["two", "three", "one"]);
}

#[test]
fn capturing_content_already_in_history_moves_it_to_the_top_instead_of_duplicating_it() {
    let mut engine = ClipEngine::new(
        InMemoryStore::default(),
        InMemoryPinnedStore::default(),
        EngineConfig::default(),
    );
    for text in ["one", "two", "three"] {
        engine.capture(text_clip(text));
    }

    assert_eq!(engine.capture(text_clip("one")), CaptureOutcome::Captured);

    assert_eq!(history_texts(&engine), vec!["two", "three", "one"]);
}

#[test]
fn deleting_a_clip_removes_only_that_clip_from_history() {
    let mut engine = ClipEngine::new(
        InMemoryStore::default(),
        InMemoryPinnedStore::default(),
        EngineConfig::default(),
    );
    for text in ["one", "two", "three"] {
        engine.capture(text_clip(text));
    }

    assert!(engine.delete(1));
    assert!(!engine.delete(5), "out of range does nothing");

    assert_eq!(history_texts(&engine), vec!["one", "three"]);
}

#[test]
fn clearing_history_removes_every_clip_but_leaves_pinned_ones() {
    let mut engine = ClipEngine::new(
        InMemoryStore::default(),
        InMemoryPinnedStore::default(),
        EngineConfig::default(),
    );
    for text in ["one", "two"] {
        engine.capture(text_clip(text));
    }
    engine.pin(0);

    engine.clear_history();

    assert!(engine.history().is_empty());
    assert_eq!(engine.pinned().len(), 1);
}
