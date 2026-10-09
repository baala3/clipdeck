use rusqlite::Connection;
use std::collections::HashSet;

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
const DEFAULT_HISTORY_CAPACITY: usize = 200;

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

pub trait ClipboardSource {
    fn next_event(&mut self) -> Option<IncomingClip>;
}

pub trait ClipStore {
    fn push(&mut self, clip: Clip);
    fn evict_oldest(&mut self);
    fn len(&self) -> usize;
    fn all(&self) -> Vec<Clip>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct ClipEngine<S: ClipStore> {
    store: S,
    config: EngineConfig,
}

impl<S: ClipStore> ClipEngine<S> {
    pub fn new(store: S, config: EngineConfig) -> Self {
        Self { store, config }
    }

    pub fn capture(&mut self, incoming: IncomingClip) -> CaptureOutcome {
        if incoming.concealed {
            return CaptureOutcome::Dropped;
        }
        if let Some(app) = &incoming.source_app {
            if self.config.excluded_apps.contains(app) {
                return CaptureOutcome::Dropped;
            }
        }
        self.store.push(Clip {
            content: incoming.content,
            source_app: incoming.source_app,
        });
        if self.store.len() > self.history_capacity() {
            self.store.evict_oldest();
        }
        CaptureOutcome::Captured
    }

    pub fn history(&self) -> Vec<Clip> {
        self.store.all()
    }

    pub fn history_capacity(&self) -> usize {
        self.config.history_capacity.min(MAX_HISTORY_CAPACITY)
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
    pub fn open(path: &str) -> Self {
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
