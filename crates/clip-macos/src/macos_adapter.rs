// UNVERIFIED: this file cannot be compiled or run anywhere in this project's
// development environment (Linux/WSL2 - there is no Apple SDK reachable, even
// through WSL's Windows-host interop, unlike the Windows adapter). It is
// written from objc2-app-kit's documented API surface plus the Apple/
// nspasteboard.org research recorded in docs/research/tech-stack.md, but it
// has never been built or exercised. It must be compiled and tested on real
// macOS hardware before being trusted - do not treat this as verified.
//
// Per Apple's own docs (see docs/research/tech-stack.md, topic 3),
// NSPasteboard has no push/event API, so this polls `changeCount` on a
// background thread instead of the event-driven approach used on Windows
// (ADR-0001). Cocoa calls made off the main thread need their own
// autorelease pool (objc2's own docs say so explicitly), hence the
// `autoreleasepool` wrapping each poll iteration below.

use clip_engine::{CapturedImage, ClipboardSource, IncomingClip, IncomingContent};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSWorkspace};
use objc2_foundation::{ns_string, NSData, NSDictionary};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const CONCEALED_TYPE: &str = "org.nspasteboard.ConcealedType";
const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";
const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub struct MacClipboardSource {
    queue: Arc<Mutex<VecDeque<IncomingClip>>>,
}

impl MacClipboardSource {
    pub fn new() -> Self {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let worker_queue = Arc::clone(&queue);
        thread::spawn(move || poll_loop(worker_queue));
        Self { queue }
    }
}

impl Default for MacClipboardSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardSource for MacClipboardSource {
    fn next_event(&mut self) -> Option<IncomingClip> {
        self.queue.lock().unwrap().pop_front()
    }
}

fn poll_loop(queue: Arc<Mutex<VecDeque<IncomingClip>>>) {
    let pasteboard = unsafe { NSPasteboard::generalPasteboard() };
    let mut last_change_count = unsafe { pasteboard.changeCount() };

    loop {
        thread::sleep(POLL_INTERVAL);

        autoreleasepool(|_| unsafe {
            let current = pasteboard.changeCount();
            if current == last_change_count {
                return;
            }
            last_change_count = current;

            let concealed = is_concealed(&pasteboard);
            let source_app = frontmost_app_name();
            let Some(content) = read_content(&pasteboard) else {
                return;
            };

            queue.lock().unwrap().push_back(IncomingClip {
                content,
                source_app,
                concealed,
            });
        });
    }
}

/// Reads the copy as text if it has any, and as an image otherwise. A copy
/// with both (spreadsheet cells, for one) is meant as text.
unsafe fn read_content(pasteboard: &NSPasteboard) -> Option<IncomingContent> {
    if let Some(text) = read_plain_text(pasteboard) {
        return Some(IncomingContent::Text(text));
    }
    read_image(pasteboard).map(IncomingContent::Image)
}

/// Screenshots and most apps copy a PNG. The rest copy a TIFF, which AppKit
/// converts.
unsafe fn read_image(pasteboard: &NSPasteboard) -> Option<CapturedImage> {
    if let Some(png) = pasteboard.dataForType(ns_string!("public.png")) {
        return CapturedImage::from_png(png.bytes());
    }
    let tiff = pasteboard.dataForType(ns_string!("public.tiff"))?;
    let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let png = bitmap
        .representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())?;
    CapturedImage::from_png(png.bytes())
}

/// Puts an image Clip's PNG on the clipboard. Returns whether it worked.
///
/// The PNG goes on as it is rather than as an `NSImage`: read back by
/// [`read_image`] it's the very same picture, so Clipdeck's own copy moves the
/// Clip to the top of History instead of adding a near-identical one.
pub fn copy_png(png: &[u8]) -> bool {
    autoreleasepool(|_| unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        pasteboard.setData_forType(Some(&NSData::with_bytes(png)), ns_string!("public.png"))
    })
}

unsafe fn read_plain_text(pasteboard: &NSPasteboard) -> Option<String> {
    let plain_text_type = ns_string!("public.utf8-plain-text");
    pasteboard
        .stringForType(plain_text_type)
        .map(|s| s.to_string())
}

unsafe fn is_concealed(pasteboard: &NSPasteboard) -> bool {
    let Some(types) = pasteboard.types() else {
        return false;
    };
    types.iter().any(|t| {
        let s = t.to_string();
        s == CONCEALED_TYPE || s == TRANSIENT_TYPE
    })
}

/// NSPasteboard doesn't record which app wrote to it, so approximate the
/// source app with the frontmost app at the time the change is noticed. This
/// can be wrong if the user switches apps within one poll interval.
unsafe fn frontmost_app_name() -> Option<String> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()?
        .localizedName()
        .map(|name| name.to_string())
}
