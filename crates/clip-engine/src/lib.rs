use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;

pub mod menu;
mod settings;
mod store_path;
pub use settings::{Settings, SettingsError, SettingsFile, ShortcutModifier};
pub use store_path::{adopt_legacy_store, LegacyStoreError};

#[derive(Debug, Clone, PartialEq)]
pub enum ClipContent {
    Text(String),
    Image(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub content: ClipContent,
    pub source_app: Option<String>,
}

pub struct IncomingClip {
    pub content: ClipContent,
    pub source_app: Option<String>,
    pub concealed: bool,
}

#[derive(Debug, PartialEq)]
pub enum CaptureOutcome {
    Captured,
    Dropped,
}

pub const MAX_HISTORY_CAPACITY: usize = 2000;
pub(crate) const DEFAULT_HISTORY_CAPACITY: usize = 200;

pub struct EngineConfig {
    pub history_capacity: usize,
    pub excluded_apps: HashSet<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            history_capacity: DEFAULT_HISTORY_CAPACITY,
            excluded_apps: HashSet::new(),
        }
    }
}

/// App names are compared case-insensitively and without a trailing `.exe`, so
/// an exclusion entry typed as "keepass" matches a Windows source app reported
/// as "KeePass.exe".
fn normalize_app_name(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    match lower.strip_suffix(".exe") {
        Some(stem) => stem.to_string(),
        None => lower,
    }
}

pub trait ClipboardSource {
    fn next_event(&mut self) -> Option<IncomingClip>;
}

pub trait ClipStore {
    fn push(&mut self, clip: Clip);
    fn evict_oldest(&mut self);
    /// Removes the Clip at `index` in chronological order (0 = oldest).
    fn remove(&mut self, index: usize);
    fn len(&self) -> usize;
    fn all(&self) -> Vec<Clip>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub trait PinnedStore {
    fn push(&mut self, clip: Clip);
    fn remove(&mut self, index: usize);
    /// Swaps the Clip at `index` for `clip`, keeping its position.
    fn replace(&mut self, index: usize, clip: Clip);
    fn all(&self) -> Vec<Clip>;

    fn contains(&self, content: &ClipContent) -> bool {
        self.all().iter().any(|clip| &clip.content == content)
    }
}

pub struct ClipEngine<S: ClipStore, P: PinnedStore> {
    store: S,
    pinned_store: P,
    config: EngineConfig,
}

impl<S: ClipStore, P: PinnedStore> ClipEngine<S, P> {
    pub fn new(store: S, pinned_store: P, config: EngineConfig) -> Self {
        Self {
            store,
            pinned_store,
            config,
        }
    }

    /// Copies a History Clip into the Pinned list. Pinning content that's
    /// already pinned is a no-op that still reports success.
    pub fn pin(&mut self, history_index: usize) -> bool {
        match self.store.all().into_iter().nth(history_index) {
            Some(clip) => {
                if !self.is_pinned(&clip.content) {
                    self.pinned_store.push(clip);
                }
                true
            }
            None => false,
        }
    }

    /// Pins text the user wrote themselves rather than copied, so it never
    /// passes through History. Blank text is rejected; already-pinned text is
    /// a no-op that still reports success.
    pub fn pin_text(&mut self, text: String) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let content = ClipContent::Text(text);
        if !self.is_pinned(&content) {
            self.pinned_store.push(Clip {
                content,
                source_app: None,
            });
        }
        true
    }

    /// Replaces a pinned item's text, keeping its position. Blank text is rejected.
    pub fn update_pinned(&mut self, pinned_index: usize, text: String) -> bool {
        if text.trim().is_empty() || pinned_index >= self.pinned_store.all().len() {
            return false;
        }
        self.pinned_store.replace(
            pinned_index,
            Clip {
                content: ClipContent::Text(text),
                source_app: None,
            },
        );
        true
    }

    pub fn is_pinned(&self, content: &ClipContent) -> bool {
        self.pinned_store.contains(content)
    }

    pub fn unpin(&mut self, pinned_index: usize) -> bool {
        if pinned_index < self.pinned_store.all().len() {
            self.pinned_store.remove(pinned_index);
            true
        } else {
            false
        }
    }

    pub fn pinned(&self) -> Vec<Clip> {
        self.pinned_store.all()
    }

    pub fn capture(&mut self, incoming: IncomingClip) -> CaptureOutcome {
        if incoming.concealed {
            return CaptureOutcome::Dropped;
        }
        // Copying a stray space or line break isn't worth a row in the menu,
        // where it would show up blank.
        if matches!(&incoming.content, ClipContent::Text(text) if text.trim().is_empty()) {
            return CaptureOutcome::Dropped;
        }
        if let Some(app) = &incoming.source_app {
            let app = normalize_app_name(app);
            if self
                .config
                .excluded_apps
                .iter()
                .any(|excluded| normalize_app_name(excluded) == app)
            {
                return CaptureOutcome::Dropped;
            }
        }
        // Copying something already in History (including Clipdeck's own
        // clipboard write after a selection) moves it to the top rather than
        // adding a duplicate.
        if let Some(existing) = self
            .store
            .all()
            .iter()
            .position(|clip| clip.content == incoming.content)
        {
            self.store.remove(existing);
        }
        self.store.push(Clip {
            content: incoming.content,
            source_app: incoming.source_app,
        });
        self.evict_down_to_capacity();
        CaptureOutcome::Captured
    }

    /// Swaps in a new config at runtime (e.g. after the user edits Settings).
    /// Lowering the capacity evicts the oldest Clips right away rather than
    /// waiting for the next capture.
    pub fn reconfigure(&mut self, config: EngineConfig) {
        self.config = config;
        self.evict_down_to_capacity();
    }

    fn evict_down_to_capacity(&mut self) {
        while self.store.len() > self.history_capacity() {
            self.store.evict_oldest();
        }
    }

    /// Moves a History Clip to the newest position, e.g. after the user selects
    /// it, so it's at the top next time. `history_index` is chronological
    /// (0 = oldest).
    pub fn promote(&mut self, history_index: usize) -> bool {
        match self.store.all().into_iter().nth(history_index) {
            Some(clip) => {
                self.store.remove(history_index);
                self.store.push(clip);
                true
            }
            None => false,
        }
    }

    /// Removes one Clip from History. `history_index` is chronological (0 = oldest).
    pub fn delete(&mut self, history_index: usize) -> bool {
        if history_index < self.store.len() {
            self.store.remove(history_index);
            true
        } else {
            false
        }
    }

    /// Empties History. The Pinned list is untouched.
    pub fn clear_history(&mut self) {
        while !self.store.is_empty() {
            self.store.evict_oldest();
        }
    }

    pub fn history(&self) -> Vec<Clip> {
        self.store.all()
    }

    pub fn history_capacity(&self) -> usize {
        self.config.history_capacity.min(MAX_HISTORY_CAPACITY)
    }

    /// Throws away whatever `source` has seen, for while capture is paused.
    /// Left in the source, those copies would be captured the moment capture
    /// resumes.
    pub fn discard(&mut self, source: &mut impl ClipboardSource) {
        while source.next_event().is_some() {}
    }

    pub fn drain(&mut self, source: &mut impl ClipboardSource) {
        while let Some(incoming) = source.next_event() {
            self.capture(incoming);
        }
    }
}

pub struct SqliteClipStore {
    conn: Connection,
}

impl SqliteClipStore {
    pub fn open(path: impl AsRef<Path>) -> Self {
        let conn = Connection::open(path).expect("failed to open sqlite clip store");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS clips (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT,
                image BLOB,
                source_app TEXT
            )",
            (),
        )
        .expect("failed to create clips table");
        Self { conn }
    }
}

pub struct SqlitePinnedStore {
    conn: Connection,
}

impl SqlitePinnedStore {
    pub fn open(path: impl AsRef<Path>) -> Self {
        let conn = Connection::open(path).expect("failed to open sqlite pinned store");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS pinned_clips (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT,
                image BLOB,
                source_app TEXT
            )",
            (),
        )
        .expect("failed to create pinned_clips table");
        Self { conn }
    }
}

impl PinnedStore for SqlitePinnedStore {
    fn contains(&self, content: &ClipContent) -> bool {
        let query = |sql: &str, value: &dyn rusqlite::ToSql| -> bool {
            self.conn
                .query_row(sql, [value], |row| row.get(0))
                .expect("failed to check pinned clips")
        };
        match content {
            ClipContent::Text(text) => query(
                "SELECT EXISTS(SELECT 1 FROM pinned_clips WHERE text = ?1)",
                text,
            ),
            ClipContent::Image(bytes) => query(
                "SELECT EXISTS(SELECT 1 FROM pinned_clips WHERE image = ?1)",
                bytes,
            ),
        }
    }

    fn push(&mut self, clip: Clip) {
        let (text, image) = match clip.content {
            ClipContent::Text(text) => (Some(text), None),
            ClipContent::Image(bytes) => (None, Some(bytes)),
        };
        self.conn
            .execute(
                "INSERT INTO pinned_clips (text, image, source_app) VALUES (?1, ?2, ?3)",
                (&text, &image, &clip.source_app),
            )
            .expect("failed to insert pinned clip");
    }

    fn replace(&mut self, index: usize, clip: Clip) {
        let (text, image) = match clip.content {
            ClipContent::Text(text) => (Some(text), None),
            ClipContent::Image(bytes) => (None, Some(bytes)),
        };
        self.conn
            .execute(
                "UPDATE pinned_clips SET text = ?1, image = ?2, source_app = ?3
                 WHERE id = (SELECT id FROM pinned_clips ORDER BY id ASC LIMIT 1 OFFSET ?4)",
                (&text, &image, &clip.source_app, index as i64),
            )
            .expect("failed to replace pinned clip");
    }

    fn remove(&mut self, index: usize) {
        let ids: Vec<i64> = self
            .conn
            .prepare("SELECT id FROM pinned_clips ORDER BY id ASC")
            .expect("failed to prepare select ids")
            .query_map((), |row| row.get(0))
            .expect("failed to query ids")
            .map(|row| row.expect("failed to read id"))
            .collect();
        if let Some(id) = ids.get(index) {
            self.conn
                .execute("DELETE FROM pinned_clips WHERE id = ?1", (id,))
                .expect("failed to delete pinned clip");
        }
    }

    fn all(&self) -> Vec<Clip> {
        let mut stmt = self
            .conn
            .prepare("SELECT text, image, source_app FROM pinned_clips ORDER BY id ASC")
            .expect("failed to prepare select");
        stmt.query_map((), |row| {
            let text: Option<String> = row.get(0)?;
            let image: Option<Vec<u8>> = row.get(1)?;
            let source_app: Option<String> = row.get(2)?;
            let content = match text {
                Some(text) => ClipContent::Text(text),
                None => ClipContent::Image(image.unwrap_or_default()),
            };
            Ok(Clip {
                content,
                source_app,
            })
        })
        .expect("failed to query pinned clips")
        .map(|row| row.expect("failed to read pinned clip row"))
        .collect()
    }
}

impl ClipStore for SqliteClipStore {
    fn push(&mut self, clip: Clip) {
        let (text, image) = match clip.content {
            ClipContent::Text(text) => (Some(text), None),
            ClipContent::Image(bytes) => (None, Some(bytes)),
        };
        self.conn
            .execute(
                "INSERT INTO clips (text, image, source_app) VALUES (?1, ?2, ?3)",
                (&text, &image, &clip.source_app),
            )
            .expect("failed to insert clip");
    }

    fn evict_oldest(&mut self) {
        self.conn
            .execute(
                "DELETE FROM clips WHERE id = (SELECT MIN(id) FROM clips)",
                (),
            )
            .expect("failed to evict oldest clip");
    }

    fn remove(&mut self, index: usize) {
        self.conn
            .execute(
                "DELETE FROM clips WHERE id = (SELECT id FROM clips ORDER BY id ASC LIMIT 1 OFFSET ?1)",
                (index as i64,),
            )
            .expect("failed to remove clip");
    }

    fn len(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM clips", (), |row| row.get(0))
            .expect("failed to count clips")
    }

    fn all(&self) -> Vec<Clip> {
        let mut stmt = self
            .conn
            .prepare("SELECT text, image, source_app FROM clips ORDER BY id ASC")
            .expect("failed to prepare select");
        stmt.query_map((), |row| {
            let text: Option<String> = row.get(0)?;
            let image: Option<Vec<u8>> = row.get(1)?;
            let source_app: Option<String> = row.get(2)?;
            let content = match text {
                Some(text) => ClipContent::Text(text),
                None => ClipContent::Image(image.unwrap_or_default()),
            };
            Ok(Clip {
                content,
                source_app,
            })
        })
        .expect("failed to query clips")
        .map(|row| row.expect("failed to read clip row"))
        .collect()
    }
}
