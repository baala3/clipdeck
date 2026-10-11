#![allow(dead_code)]

use clip_engine::{CapturedImage, ImageStore, Rgba};
use std::collections::HashMap;

/// An [`ImageStore`] that keeps its images in memory.
#[derive(Default)]
pub struct InMemoryImageStore {
    images: HashMap<String, CapturedImage>,
}

impl ImageStore for InMemoryImageStore {
    fn put(&mut self, image: &CapturedImage) {
        self.images
            .insert(image.info().digest.clone(), image.clone());
    }

    fn png(&self, digest: &str) -> Option<Vec<u8>> {
        self.images.get(digest).map(|image| image.png().to_vec())
    }

    fn thumbnail_png(&self, digest: &str) -> Option<Vec<u8>> {
        self.images
            .get(digest)
            .map(|image| image.thumbnail_png().to_vec())
    }

    fn remove(&mut self, digest: &str) {
        self.images.remove(digest);
    }

    fn digests(&self) -> Vec<String> {
        self.images.keys().cloned().collect()
    }
}

/// A `width` x `height` image of one opaque color.
pub fn solid(width: u32, height: u32, color: [u8; 3]) -> Rgba {
    let pixel = [color[0], color[1], color[2], 255];
    Rgba::new(width, height, pixel.repeat((width * height) as usize)).unwrap()
}

pub fn captured(image: &Rgba) -> CapturedImage {
    CapturedImage::from_rgba(image).unwrap()
}
