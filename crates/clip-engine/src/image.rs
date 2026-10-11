//! Image Clips. A copied image is kept as a PNG that Clipdeck encodes itself,
//! with a small thumbnail for the menus. Two copies are the same Clip when
//! their pixels match, whatever format the clipboard handed them over in, so
//! an image is identified by a digest of its pixels (see ADR-0009).

use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Larger images aren't captured. This is above an 8K screenshot (33
/// megapixels), and bounds the memory a single copy can take to decode.
pub const MAX_IMAGE_PIXELS: u64 = 40_000_000;
/// An image whose PNG comes out larger than this isn't captured.
pub const MAX_IMAGE_PNG_BYTES: usize = 20 * 1024 * 1024;
/// A thumbnail fits in this box, in pixels. Wide and short, like a menu row;
/// large enough to stay sharp on a high-DPI screen.
pub const THUMBNAIL_MAX_WIDTH: u32 = 192;
pub const THUMBNAIL_MAX_HEIGHT: u32 = 64;

/// What History and the Pinned list hold for an image Clip. The PNG itself
/// lives in an [`ImageStore`], looked up by `digest`.
#[derive(Debug, Clone, Eq)]
pub struct ImageInfo {
    /// Hex SHA-256 of the image's size and RGBA pixels.
    pub digest: String,
    pub width: u32,
    pub height: u32,
    /// Size of the stored PNG, for keeping History's images within a budget.
    pub byte_len: u64,
}

/// The digest alone decides equality: the same pixels encoded by a different
/// version of the PNG encoder are still the same Clip.
impl PartialEq for ImageInfo {
    fn eq(&self, other: &Self) -> bool {
        self.digest == other.digest
    }
}

/// Decoded pixels: 8-bit RGBA, rows top to bottom, alpha not premultiplied.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    /// `None` unless `pixels` is exactly `width * height` RGBA pixels.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        let expected = (width as u64) * (height as u64) * 4;
        (width > 0 && height > 0 && pixels.len() as u64 == expected).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Reads a PNG of any color type or bit depth. Only the first frame of an
    /// animated PNG is read.
    pub fn from_png(png: &[u8]) -> Option<Self> {
        let mut decoder = png::Decoder::new(Cursor::new(png));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().ok()?;
        let (width, height) = reader.info().size();
        if (width as u64) * (height as u64) > MAX_IMAGE_PIXELS {
            return None;
        }
        let mut buffer = vec![0; reader.output_buffer_size()?];
        let frame = reader.next_frame(&mut buffer).ok()?;
        buffer.truncate(frame.buffer_size());
        let pixels = match frame.color_type {
            png::ColorType::Rgba => buffer,
            png::ColorType::Rgb => buffer
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|&[r, g, b]| [r, g, b, 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => buffer
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|&[v, a]| [v, v, v, a])
                .collect(),
            png::ColorType::Grayscale => buffer.iter().flat_map(|&v| [v, v, v, 255]).collect(),
            // `normalize_to_color8` expands a palette into RGB or RGBA.
            png::ColorType::Indexed => return None,
        };
        Self::new(width, height, pixels)
    }

    pub fn to_png(&self) -> Option<Vec<u8>> {
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&self.pixels).ok()?;
        writer.finish().ok()?;
        Some(png)
    }

    /// A copy scaled down to fit in `max_width` x `max_height`, keeping its
    /// proportions. An image that already fits is returned as it is, never
    /// enlarged.
    pub fn fit_within(&self, max_width: u32, max_height: u32) -> Rgba {
        let (max_width, max_height) = (max_width.max(1) as u64, max_height.max(1) as u64);
        let (width, height) = (self.width as u64, self.height as u64);
        if width <= max_width && height <= max_height {
            return self.clone();
        }
        // The tighter of the two limits sets the scale.
        let (new_width, new_height) = if width * max_height > height * max_width {
            (max_width, (height * max_width / width).max(1))
        } else {
            ((width * max_height / height).max(1), max_height)
        };
        self.downscale(new_width as u32, new_height as u32)
    }

    /// Averages the block of source pixels behind each new pixel. Colors are
    /// weighted by alpha, so a transparent pixel's color doesn't bleed into
    /// its neighbours.
    fn downscale(&self, new_width: u32, new_height: u32) -> Rgba {
        let (src_width, src_height) = (self.width as usize, self.height as usize);
        let (new_width, new_height) = (new_width as usize, new_height as usize);
        let mut pixels = Vec::with_capacity(new_width * new_height * 4);
        for y in 0..new_height {
            let top = y * src_height / new_height;
            let bottom = ((y + 1) * src_height / new_height).max(top + 1);
            for x in 0..new_width {
                let left = x * src_width / new_width;
                let right = ((x + 1) * src_width / new_width).max(left + 1);
                let (mut red, mut green, mut blue, mut alpha) = (0u64, 0u64, 0u64, 0u64);
                for row in top..bottom {
                    let start = (row * src_width + left) * 4;
                    let end = (row * src_width + right) * 4;
                    for &[r, g, b, a] in self.pixels[start..end].as_chunks::<4>().0 {
                        let a = a as u64;
                        red += r as u64 * a;
                        green += g as u64 * a;
                        blue += b as u64 * a;
                        alpha += a;
                    }
                }
                let count = ((bottom - top) * (right - left)) as u64;
                if alpha == 0 {
                    pixels.extend_from_slice(&[0, 0, 0, 0]);
                } else {
                    let channel = |sum: u64| ((sum + alpha / 2) / alpha) as u8;
                    pixels.extend_from_slice(&[
                        channel(red),
                        channel(green),
                        channel(blue),
                        ((alpha + count / 2) / count) as u8,
                    ]);
                }
            }
        }
        Rgba {
            width: new_width as u32,
            height: new_height as u32,
            pixels,
        }
    }

    fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.width.to_le_bytes());
        hasher.update(self.height.to_le_bytes());
        hasher.update(&self.pixels);
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// An image a [`crate::ClipboardSource`] just read, ready to be captured: its
/// PNG and thumbnail are already encoded, so capturing it is quick.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedImage {
    info: ImageInfo,
    png: Vec<u8>,
    thumbnail_png: Vec<u8>,
}

impl CapturedImage {
    /// `None` when the image is over the size limits.
    pub fn from_rgba(image: &Rgba) -> Option<Self> {
        if (image.width as u64) * (image.height as u64) > MAX_IMAGE_PIXELS {
            return None;
        }
        let png = image.to_png()?;
        if png.len() > MAX_IMAGE_PNG_BYTES {
            return None;
        }
        let thumbnail_png = image
            .fit_within(THUMBNAIL_MAX_WIDTH, THUMBNAIL_MAX_HEIGHT)
            .to_png()?;
        Some(Self {
            info: ImageInfo {
                digest: image.digest(),
                width: image.width,
                height: image.height,
                byte_len: png.len() as u64,
            },
            png,
            thumbnail_png,
        })
    }

    /// `None` when `png` can't be read, or the image is over the size limits.
    pub fn from_png(png: &[u8]) -> Option<Self> {
        Self::from_rgba(&Rgba::from_png(png)?)
    }

    pub fn info(&self) -> &ImageInfo {
        &self.info
    }

    pub fn png(&self) -> &[u8] {
        &self.png
    }

    pub fn thumbnail_png(&self) -> &[u8] {
        &self.thumbnail_png
    }
}

/// Where the PNGs behind image Clips are kept, shared by History and the
/// Pinned list.
pub trait ImageStore {
    fn put(&mut self, image: &CapturedImage);
    fn png(&self, digest: &str) -> Option<Vec<u8>>;
    fn thumbnail_png(&self, digest: &str) -> Option<Vec<u8>>;
    fn remove(&mut self, digest: &str);
    fn digests(&self) -> Vec<String>;
}

const PNG_SUFFIX: &str = ".png";
const THUMBNAIL_SUFFIX: &str = ".thumb.png";

/// Keeps each image as `<digest>.png` and `<digest>.thumb.png` in one folder.
pub struct DirImageStore {
    dir: PathBuf,
}

impl DirImageStore {
    pub fn open(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref().to_path_buf();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            eprintln!("couldn't create {}: {err}", dir.display());
        }
        Self { dir }
    }

    fn path(&self, digest: &str, suffix: &str) -> PathBuf {
        self.dir.join(format!("{digest}{suffix}"))
    }
}

impl ImageStore for DirImageStore {
    fn put(&mut self, image: &CapturedImage) {
        let digest = &image.info().digest;
        for (suffix, bytes) in [
            (PNG_SUFFIX, image.png()),
            (THUMBNAIL_SUFFIX, image.thumbnail_png()),
        ] {
            if let Err(err) = std::fs::write(self.path(digest, suffix), bytes) {
                eprintln!("couldn't save an image Clip: {err}");
            }
        }
    }

    fn png(&self, digest: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(digest, PNG_SUFFIX)).ok()
    }

    fn thumbnail_png(&self, digest: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(digest, THUMBNAIL_SUFFIX)).ok()
    }

    fn remove(&mut self, digest: &str) {
        for suffix in [PNG_SUFFIX, THUMBNAIL_SUFFIX] {
            let _ = std::fs::remove_file(self.path(digest, suffix));
        }
    }

    /// Every digest with a file here, including one whose other file is
    /// missing, so that a half-written image can still be cleaned up.
    fn digests(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let digests: HashSet<String> = entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter_map(|name| {
                name.strip_suffix(THUMBNAIL_SUFFIX)
                    .or_else(|| name.strip_suffix(PNG_SUFFIX))
                    .map(str::to_string)
            })
            .collect();
        digests.into_iter().collect()
    }
}
