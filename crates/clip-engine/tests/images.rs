use clip_engine::{
    CaptureOutcome, CapturedImage, Clip, ClipContent, ClipEngine, DirImageStore, EngineConfig,
    ImageStore, IncomingClip, IncomingContent, Rgba, SqliteClipStore, SqlitePinnedStore,
    MAX_IMAGE_PIXELS, THUMBNAIL_MAX_HEIGHT, THUMBNAIL_MAX_WIDTH,
};

mod common;
use common::{captured, solid, InMemoryImageStore};

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];

type Engine<I> = ClipEngine<SqliteClipStore, SqlitePinnedStore, I>;

/// An engine over real stores in a temp dir, which must outlive it.
fn engine_in<I: ImageStore>(dir: &tempfile::TempDir, images: I, config: EngineConfig) -> Engine<I> {
    ClipEngine::new(
        SqliteClipStore::open(dir.path().join("clips.sqlite3")),
        SqlitePinnedStore::open(dir.path().join("pinned.sqlite3")),
        images,
        config,
    )
}

fn engine(dir: &tempfile::TempDir) -> Engine<InMemoryImageStore> {
    engine_in(dir, InMemoryImageStore::default(), EngineConfig::default())
}

fn image_clip(image: &Rgba) -> IncomingClip {
    IncomingClip {
        content: IncomingContent::Image(captured(image)),
        source_app: Some("Preview".into()),
        concealed: false,
    }
}

fn text_clip(text: &str) -> IncomingClip {
    IncomingClip {
        content: IncomingContent::Text(text.into()),
        source_app: Some("TextEdit".into()),
        concealed: false,
    }
}

/// History as short names: a text Clip's text, or an image Clip's size.
fn history<I: ImageStore>(engine: &Engine<I>) -> Vec<String> {
    names(engine.history())
}

fn names(clips: Vec<Clip>) -> Vec<String> {
    clips
        .into_iter()
        .map(|clip| match clip.content {
            ClipContent::Text(text) => text,
            ClipContent::Image(image) => format!("{}x{}", image.width, image.height),
        })
        .collect()
}

fn image_at<I: ImageStore>(engine: &Engine<I>, history_index: usize) -> clip_engine::ImageInfo {
    match engine.history().remove(history_index).content {
        ClipContent::Image(image) => image,
        ClipContent::Text(text) => panic!("expected an image, found {text:?}"),
    }
}

#[test]
fn a_copied_image_is_captured_with_its_size_and_can_be_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = engine(&dir);
    let picture = solid(40, 30, RED);

    assert_eq!(
        engine.capture(image_clip(&picture)),
        CaptureOutcome::Captured
    );

    let image = image_at(&engine, 0);
    assert_eq!((image.width, image.height), (40, 30));
    let png = engine.image_png(&image).unwrap();
    assert_eq!(image.byte_len, png.len() as u64);
    assert_eq!(Rgba::from_png(&png).unwrap(), picture);
}

#[test]
fn copying_the_same_picture_again_moves_it_to_the_top_instead_of_duplicating_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = engine(&dir);

    engine.capture(image_clip(&solid(40, 30, RED)));
    engine.capture(text_clip("between"));
    engine.capture(image_clip(&solid(40, 30, RED)));

    assert_eq!(history(&engine), ["between", "40x30"]);
}

#[test]
fn pictures_of_the_same_size_with_different_pixels_are_different_clips() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = engine(&dir);

    engine.capture(image_clip(&solid(40, 30, RED)));
    engine.capture(image_clip(&solid(40, 30, BLUE)));

    assert_eq!(history(&engine), ["40x30", "40x30"]);
}

#[test]
fn a_picture_read_from_a_png_matches_the_same_picture_read_as_pixels() {
    let picture = solid(8, 8, GREEN);
    let from_pixels = captured(&picture);

    let from_png = CapturedImage::from_png(&picture.to_png().unwrap()).unwrap();

    assert_eq!(from_png.info(), from_pixels.info());
}

#[test]
fn a_concealed_or_excluded_image_is_neither_captured_nor_stored() {
    let dir = tempfile::tempdir().unwrap();
    let images_dir = dir.path().join("images");
    let config = EngineConfig {
        excluded_apps: ["Preview".to_string()].into(),
        ..Default::default()
    };
    let mut engine = engine_in(&dir, DirImageStore::open(&images_dir), config);

    let excluded = engine.capture(image_clip(&solid(40, 30, RED)));
    let concealed = engine.capture(IncomingClip {
        source_app: Some("Passwords".into()),
        concealed: true,
        ..image_clip(&solid(40, 30, BLUE))
    });

    assert_eq!(excluded, CaptureOutcome::Dropped);
    assert_eq!(concealed, CaptureOutcome::Dropped);
    assert!(history(&engine).is_empty());
    assert_eq!(std::fs::read_dir(&images_dir).unwrap().count(), 0);
}

#[test]
fn an_image_evicted_from_history_is_deleted_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    let images_dir = dir.path().join("images");
    let config = EngineConfig {
        history_capacity: 1,
        ..Default::default()
    };
    let mut engine = engine_in(&dir, DirImageStore::open(&images_dir), config);

    engine.capture(image_clip(&solid(40, 30, RED)));
    let evicted = image_at(&engine, 0);
    assert!(engine.image_png(&evicted).is_some());
    engine.capture(text_clip("pushes the image out"));

    assert_eq!(history(&engine), ["pushes the image out"]);
    assert!(engine.image_png(&evicted).is_none());
    assert_eq!(std::fs::read_dir(&images_dir).unwrap().count(), 0);
}

#[test]
fn a_pinned_image_outlives_its_history_clip_until_it_is_unpinned() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = engine(&dir);
    engine.capture(image_clip(&solid(40, 30, RED)));
    let image = image_at(&engine, 0);

    assert!(engine.pin(0));
    assert!(engine.pin(0), "pinning it twice is harmless");
    engine.clear_history();

    assert_eq!(names(engine.pinned()), ["40x30"]);
    assert!(engine.image_png(&image).is_some());
    assert!(engine.image_thumbnail_png(&image).is_some());

    assert!(engine.unpin(0));

    assert!(engine.image_png(&image).is_none());
}

#[test]
fn unpinning_an_image_still_in_history_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = engine(&dir);
    engine.capture(image_clip(&solid(40, 30, RED)));
    let image = image_at(&engine, 0);
    engine.pin(0);

    engine.unpin(0);

    assert!(engine.image_png(&image).is_some());
}

#[test]
fn deleting_an_image_clip_deletes_its_image() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = engine(&dir);
    engine.capture(image_clip(&solid(40, 30, RED)));
    let image = image_at(&engine, 0);

    engine.delete(0);

    assert!(engine.image_png(&image).is_none());
}

#[test]
fn a_pinned_image_cannot_be_rewritten_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = engine(&dir);
    engine.capture(image_clip(&solid(40, 30, RED)));
    engine.pin(0);

    assert!(!engine.update_pinned(0, "text".into()));

    assert_eq!(names(engine.pinned()), ["40x30"]);
}

#[test]
fn history_images_over_the_budget_evict_the_oldest_images_but_no_text() {
    let dir = tempfile::tempdir().unwrap();
    let one_image = captured(&solid(40, 30, RED)).info().byte_len;
    let config = EngineConfig {
        // Room for two of these images, not three.
        history_image_budget: one_image * 2 + one_image / 2,
        ..Default::default()
    };
    let mut engine = engine_in(&dir, InMemoryImageStore::default(), config);

    engine.capture(image_clip(&solid(40, 30, RED)));
    let oldest = image_at(&engine, 0);
    engine.capture(text_clip("text is never counted"));
    engine.capture(image_clip(&solid(41, 30, GREEN)));
    engine.capture(image_clip(&solid(42, 30, BLUE)));

    assert_eq!(
        history(&engine),
        ["text is never counted", "41x30", "42x30"]
    );
    assert!(engine.image_png(&oldest).is_none());
}

#[test]
fn images_left_behind_by_a_crash_are_cleaned_up_on_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let images_dir = dir.path().join("images");
    {
        let mut engine = engine_in(
            &dir,
            DirImageStore::open(&images_dir),
            EngineConfig::default(),
        );
        engine.capture(image_clip(&solid(40, 30, RED)));
    }
    // Saved, but the Clip never reached History.
    let mut orphans = DirImageStore::open(&images_dir);
    orphans.put(&captured(&solid(40, 30, BLUE)));
    assert_eq!(orphans.digests().len(), 2);

    let engine = engine_in(
        &dir,
        DirImageStore::open(&images_dir),
        EngineConfig::default(),
    );

    let kept = image_at(&engine, 0);
    assert_eq!(DirImageStore::open(&images_dir).digests(), [kept.digest]);
}

#[test]
fn image_clips_and_their_files_persist_across_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let images_dir = dir.path().join("images");
    let picture = solid(40, 30, RED);
    {
        let mut engine = engine_in(
            &dir,
            DirImageStore::open(&images_dir),
            EngineConfig::default(),
        );
        engine.capture(image_clip(&picture));
        engine.pin(0);
    }

    let engine = engine_in(
        &dir,
        DirImageStore::open(&images_dir),
        EngineConfig::default(),
    );

    assert_eq!(history(&engine), ["40x30"]);
    assert_eq!(names(engine.pinned()), ["40x30"]);
    let png = engine.image_png(&image_at(&engine, 0)).unwrap();
    assert_eq!(Rgba::from_png(&png).unwrap(), picture);
}

#[test]
fn an_image_over_the_pixel_limit_is_not_captured() {
    let width = 8000;
    let height = (MAX_IMAGE_PIXELS / width as u64) as u32 + 1;
    let too_large = Rgba::new(width, height, vec![0; (width * height * 4) as usize]).unwrap();

    assert!(CapturedImage::from_rgba(&too_large).is_none());
}

#[test]
fn pixels_that_do_not_match_the_size_are_rejected() {
    assert!(Rgba::new(2, 2, vec![0; 15]).is_none());
    assert!(Rgba::new(0, 2, vec![]).is_none());
    assert!(CapturedImage::from_png(b"not a png").is_none());
}

#[test]
fn a_thumbnail_fits_the_menu_and_keeps_the_picture_s_proportions() {
    let wide = captured(&solid(1920, 1080, RED));
    let tall = captured(&solid(100, 1000, RED));
    let small = captured(&solid(20, 10, RED));

    let size = |image: &CapturedImage| {
        let thumbnail = Rgba::from_png(image.thumbnail_png()).unwrap();
        (thumbnail.width, thumbnail.height)
    };
    assert_eq!(size(&wide), (113, THUMBNAIL_MAX_HEIGHT));
    assert_eq!(size(&tall), (6, THUMBNAIL_MAX_HEIGHT));
    assert_eq!(size(&small), (20, 10), "a small picture isn't enlarged");
    let banner = captured(&solid(2000, 100, RED));
    assert_eq!(size(&banner), (THUMBNAIL_MAX_WIDTH, 9));
}

#[test]
fn a_thumbnail_averages_the_pixels_it_covers() {
    // Left half black, right half white, shrunk to a single row of pixels.
    let mut pixels = Vec::new();
    for _ in 0..128 {
        for x in 0..128 {
            let v = if x < 64 { 0 } else { 255 };
            pixels.extend_from_slice(&[v, v, v, 255]);
        }
    }
    let picture = Rgba::new(128, 128, pixels).unwrap();

    let thumbnail = picture.fit_within(2, 2);

    assert_eq!((thumbnail.width, thumbnail.height), (2, 2));
    assert_eq!(&thumbnail.pixels[0..4], &[0, 0, 0, 255]);
    assert_eq!(&thumbnail.pixels[4..8], &[255, 255, 255, 255]);
}

#[test]
fn transparent_pixels_do_not_darken_a_thumbnail_s_edges() {
    // Opaque red beside fully transparent black.
    let mut pixels = Vec::new();
    for _ in 0..2 {
        pixels.extend_from_slice(&[255, 0, 0, 255, 0, 0, 0, 0]);
    }
    let picture = Rgba::new(2, 2, pixels).unwrap();

    let thumbnail = picture.fit_within(1, 1);

    assert_eq!(thumbnail.pixels, [255, 0, 0, 128]);
}

/// Encodes `data` as a PNG of the given color type, the way another app might.
fn foreign_png(color: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 2, 1);
    encoder.set_color(color);
    encoder.set_depth(depth);
    if color == png::ColorType::Indexed {
        encoder.set_palette(vec![255, 0, 0, 0, 0, 255]);
    }
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
    writer.finish().unwrap();
    out
}

#[test]
fn pngs_of_any_color_type_are_read_as_rgba() {
    use png::{BitDepth, ColorType};
    let red_then_blue = vec![255, 0, 0, 255, 0, 0, 255, 255];
    let black_then_white = vec![0, 0, 0, 255, 255, 255, 255, 255];
    let cases = [
        (
            foreign_png(ColorType::Rgb, BitDepth::Eight, &[255, 0, 0, 0, 0, 255]),
            &red_then_blue,
        ),
        (
            foreign_png(ColorType::Indexed, BitDepth::Eight, &[0, 1]),
            &red_then_blue,
        ),
        (
            foreign_png(ColorType::Grayscale, BitDepth::Eight, &[0, 255]),
            &black_then_white,
        ),
        (
            foreign_png(
                ColorType::GrayscaleAlpha,
                BitDepth::Eight,
                &[0, 255, 255, 255],
            ),
            &black_then_white,
        ),
        (
            foreign_png(
                ColorType::Rgb,
                BitDepth::Sixteen,
                &[255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255],
            ),
            &red_then_blue,
        ),
    ];

    for (png, expected) in cases {
        let image = Rgba::from_png(&png).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(&image.pixels, expected);
    }
}
