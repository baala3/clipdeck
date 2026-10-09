#![cfg(target_os = "windows")]

use clip_engine::{ClipContent, ClipboardSource};
use clip_windows::WindowsClipboardSource;
use std::time::{Duration, Instant};

use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

fn set_real_clipboard_text(text: &str) {
    unsafe {
        OpenClipboard(None).expect("OpenClipboard failed");
        EmptyClipboard().expect("EmptyClipboard failed");

        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_len = wide.len() * std::mem::size_of::<u16>();

        let hglobal = GlobalAlloc(GMEM_MOVEABLE, byte_len).expect("GlobalAlloc failed");
        let ptr = GlobalLock(hglobal) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(hglobal);

        SetClipboardData(CF_UNICODETEXT.0 as u32, windows::Win32::Foundation::HANDLE(hglobal.0))
            .expect("SetClipboardData failed");
        let _ = CloseClipboard();
    }
}

#[test]
fn next_event_is_none_when_nothing_has_happened_yet() {
    let mut source = WindowsClipboardSource::new();
    assert!(source.next_event().is_none());
}

#[test]
fn capturing_a_real_clipboard_change_produces_a_matching_event() {
    let mut source = WindowsClipboardSource::new();
    // Give the background listener thread time to register before we change the clipboard.
    std::thread::sleep(Duration::from_millis(200));

    set_real_clipboard_text("clipdeck windows listener round trip");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut captured = None;
    while Instant::now() < deadline {
        if let Some(event) = source.next_event() {
            captured = Some(event);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let captured = captured.expect("expected a clipboard event within 5 seconds");
    assert_eq!(
        captured.content,
        ClipContent::Text("clipdeck windows listener round trip".into())
    );
}
