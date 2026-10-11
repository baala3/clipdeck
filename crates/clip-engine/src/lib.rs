use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;

mod image;
pub mod menu;
mod settings;
mod store_path;
pub use image::{
    CapturedImage, DirImageStore, ImageInfo, ImageStore, Rgba, MAX_IMAGE_PIXELS,
    MAX_IMAGE_PNG_BYTES, THUMBNAIL_MAX_HEIGHT, THUMBNAIL_MAX_WIDTH,
};
pub use settings::{Appearance, Settings, SettingsError, SettingsFile, ShortcutModifier};
pub use store_path::{adopt_legacy_store, LegacyStoreError};

#[derive(Debug, Clone, PartialEq)]
pub enum ClipContent {
    Text(String),
    Image(ImageInfo),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub content: ClipContent,
    pub source_app: Option<String>,
}

/// What a [`ClipboardSource`] read from the clipboard. Unlike [`ClipContent`],
/// an image still carries its PNG here; capturing it moves that into the
/// [`ImageStore`].
#[derive(Debug, Clone, PartialEq)]
pub enum IncomingContent {
    Text(String),
    Image(CapturedImage),
}

pub struct IncomingClip {
    pub content: IncomingContent,
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
/// How much disk the images in History may take together. A full History of
/// large screenshots would otherwise run to gigabytes.
pub const DEFAULT_HISTORY_IMAGE_BUDGET: u64 = 256 * 1024 * 1024;

pub struct EngineConfig {
    pub history_capacity: usize,
    pub excluded_apps: HashSet<String>,
    /// Total PNG bytes History's images may take. Once over it, the oldest
    /// image Clips are evicted, whatever the capacity. Pinned images don't count.
    pub history_image_budget: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            history_capacity: DEFAULT_HISTORY_CAPACITY,
            excluded_apps: HashSet::new(),
            history_image_budget: DEFAULT_HISTORY_IMAGE_BUDGET,
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

pub struct ClipEngine<S: ClipStore, P: PinnedStore, I: ImageStore> {
    store: S,
    pinned_store: P,
    image_store: I,
    config: EngineConfig,
}

impl<S: ClipStore, P: PinnedStore, I: ImageStore> ClipEngine<S, P, I> {
    pub fn new(store: S, pinned_store: P, image_store: I, config: EngineConfig) -> Self {
        let mut engine = Self {
            store,
            pinned_store,
            image_store,
            config,
        };
        // An image saved just before a crash may never have reached a list.
        engine.drop_unreferenced_images();
        engine
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

    /// Replaces a pinned item's text, keeping its position. Blank text is
    /// rejected, and so is rewriting a pinned image.
    pub fn update_pinned(&mut self, pinned_index: usize, text: String) -> bool {
        let is_text = matches!(
            self.pinned_store.all().get(pinned_index),
            Some(Clip {
                content: ClipContent::Text(_),
                ..
            })
        );
        if text.trim().is_empty() || !is_text {
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
            self.drop_unreferenced_images();
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
        if matches!(&incoming.content, IncomingContent::Text(text) if text.trim().is_empty()) {
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
        let content = match incoming.content {
            IncomingContent::Text(text) => ClipContent::Text(text),
            IncomingContent::Image(image) => {
                self.image_store.put(&image);
                ClipContent::Image(image.info().clone())
            }
        };
        // Copying something already in History (including Clipdeck's own
        // clipboard write after a selection) moves it to the top rather than
        // adding a duplicate.
        if let Some(existing) = self
            .store
            .all()
            .iter()
            .position(|clip| clip.content == content)
        {
            self.store.remove(existing);
        }
        self.store.push(Clip {
            content,
            source_app: incoming.source_app,
        });
        self.evict_down_to_limits();
        CaptureOutcome::Captured
    }

    /// Swaps in a new config at runtime (e.g. after the user edits Settings).
    /// Lowering the capacity evicts the oldest Clips right away rather than
    /// waiting for the next capture.
    pub fn reconfigure(&mut self, config: EngineConfig) {
        self.config = config;
        self.evict_down_to_limits();
    }

    fn evict_down_to_limits(&mut self) {
        while self.store.len() > self.history_capacity() {
            self.store.evict_oldest();
        }
        let image_bytes = |clip: &Clip| match &clip.content {
            ClipContent::Image(info) => Some(info.byte_len),
            ClipContent::Text(_) => None,
        };
        let clips = self.store.all();
        let mut total: u64 = clips.iter().filter_map(image_bytes).sum();
        let mut evicted = 0;
        for (index, clip) in clips.iter().enumerate() {
            if total <= self.config.history_image_budget {
                break;
            }
            if let Some(bytes) = image_bytes(clip) {
                self.store.remove(index - evicted);
                evicted += 1;
                total -= bytes;
            }
        }
        self.drop_unreferenced_images();
    }

    /// Deletes the stored images that neither History nor the Pinned list
    /// refers to any more.
    fn drop_unreferenced_images(&mut self) {
        let stored = self.image_store.digests();
        if stored.is_empty() {
            return;
        }
        let referenced: HashSet<String> = self
            .store
            .all()
            .into_iter()
            .chain(self.pinned_store.all())
            .filter_map(|clip| match clip.content {
                ClipContent::Image(info) => Some(info.digest),
                ClipContent::Text(_) => None,
            })
            .collect();
        for digest in stored {
            if !referenced.contains(&digest) {
                self.image_store.remove(&digest);
            }
        }
    }

    /// The PNG of an image Clip, or `None` if its file has gone missing.
    pub fn image_png(&self, image: &ImageInfo) -> Option<Vec<u8>> {
        self.image_store.png(&image.digest)
    }

    /// The PNG of an image Clip's thumbnail, which fits in
    /// [`THUMBNAIL_MAX_WIDTH`] x [`THUMBNAIL_MAX_HEIGHT`].
    pub fn image_thumbnail_png(&self, image: &ImageInfo) -> Option<Vec<u8>> {
        self.image_store.thumbnail_png(&image.digest)
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
            self.drop_unreferenced_images();
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
        self.drop_unreferenced_images();
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

/// The columns both SQLite stores keep a Clip in. An image's PNG isn't among
/// them; it's in the [`ImageStore`].
const CLIP_COLUMNS: &str = "text, image_digest, image_width, image_height, image_bytes, source_app";

/// Opens the store at `path` and makes sure `table` has today's columns.
fn open_clip_table(path: impl AsRef<Path>, table: &str) -> Connection {
    let conn = Connection::open(path).expect("failed to open sqlite store");
    // `image` is unused: versions before image capture created it and never
    // wrote to it. It stays so that an older Clipdeck can still open the store.
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS {table} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT,
                image BLOB,
                source_app TEXT
            )"
        ),
        (),
    )
    .expect("failed to create the clips table");
    let has_image_columns: bool = conn
        .query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name = 'image_digest')"
            ),
            (),
            |row| row.get(0),
        )
        .expect("failed to read the clips table's columns");
    if !has_image_columns {
        conn.execute_batch(&format!(
            "BEGIN;
             ALTER TABLE {table} ADD COLUMN image_digest TEXT;
             ALTER TABLE {table} ADD COLUMN image_width INTEGER;
             ALTER TABLE {table} ADD COLUMN image_height INTEGER;
             ALTER TABLE {table} ADD COLUMN image_bytes INTEGER;
             COMMIT;"
        ))
        .expect("failed to add the image columns");
    }
    // A row with neither text nor an image is no Clip at all.
    conn.execute(
        &format!("DELETE FROM {table} WHERE text IS NULL AND image_digest IS NULL"),
        (),
    )
    .expect("failed to drop empty rows");
    conn
}

/// A Clip's values for [`CLIP_COLUMNS`].
type ClipRow = (
    Option<String>,
    Option<String>,
    Option<u32>,
    Option<u32>,
    Option<i64>,
    Option<String>,
);

fn clip_row(clip: Clip) -> ClipRow {
    match clip.content {
        ClipContent::Text(text) => (Some(text), None, None, None, None, clip.source_app),
        ClipContent::Image(image) => (
            None,
            Some(image.digest),
            Some(image.width),
            Some(image.height),
            Some(image.byte_len as i64),
            clip.source_app,
        ),
    }
}

fn clip_from_row(row: &rusqlite::Row) -> rusqlite::Result<Clip> {
    let text: Option<String> = row.get(0)?;
    let content = match text {
        Some(text) => ClipContent::Text(text),
        None => ClipContent::Image(ImageInfo {
            digest: row.get(1)?,
            width: row.get(2)?,
            height: row.get(3)?,
            byte_len: row.get::<_, i64>(4)? as u64,
        }),
    };
    Ok(Clip {
        content,
        source_app: row.get(5)?,
    })
}

fn all_clips(conn: &Connection, table: &str) -> Vec<Clip> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {CLIP_COLUMNS} FROM {table} ORDER BY id ASC"
        ))
        .expect("failed to prepare select");
    stmt.query_map((), clip_from_row)
        .expect("failed to query clips")
        .map(|row| row.expect("failed to read clip row"))
        .collect()
}

fn insert_clip(conn: &Connection, table: &str, clip: Clip) {
    conn.execute(
        &format!("INSERT INTO {table} ({CLIP_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
        clip_row(clip),
    )
    .expect("failed to insert clip");
}

pub struct SqliteClipStore {
    conn: Connection,
}

impl SqliteClipStore {
    pub fn open(path: impl AsRef<Path>) -> Self {
        Self {
            conn: open_clip_table(path, "clips"),
        }
    }
}

pub struct SqlitePinnedStore {
    conn: Connection,
}

impl SqlitePinnedStore {
    pub fn open(path: impl AsRef<Path>) -> Self {
        Self {
            conn: open_clip_table(path, "pinned_clips"),
        }
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
            ClipContent::Image(image) => query(
                "SELECT EXISTS(SELECT 1 FROM pinned_clips WHERE image_digest = ?1)",
                &image.digest,
            ),
        }
    }

    fn push(&mut self, clip: Clip) {
        insert_clip(&self.conn, "pinned_clips", clip);
    }

    fn replace(&mut self, index: usize, clip: Clip) {
        let (text, digest, width, height, bytes, source_app) = clip_row(clip);
        self.conn
            .execute(
                "UPDATE pinned_clips
                 SET text = ?1, image_digest = ?2, image_width = ?3, image_height = ?4,
                     image_bytes = ?5, source_app = ?6
                 WHERE id = (SELECT id FROM pinned_clips ORDER BY id ASC LIMIT 1 OFFSET ?7)",
                (text, digest, width, height, bytes, source_app, index as i64),
            )
            .expect("failed to replace pinned clip");
    }

    fn remove(&mut self, index: usize) {
        self.conn
            .execute(
                "DELETE FROM pinned_clips WHERE id = (SELECT id FROM pinned_clips ORDER BY id ASC LIMIT 1 OFFSET ?1)",
                (index as i64,),
            )
            .expect("failed to delete pinned clip");
    }

    fn all(&self) -> Vec<Clip> {
        all_clips(&self.conn, "pinned_clips")
    }
}

impl ClipStore for SqliteClipStore {
    fn push(&mut self, clip: Clip) {
        insert_clip(&self.conn, "clips", clip);
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
        all_clips(&self.conn, "clips")
    }
}
