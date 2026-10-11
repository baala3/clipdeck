#![cfg(target_os = "windows")]

use clip_engine::{CapturedImage, ClipboardSource, IncomingContent, Rgba};
use clip_windows::WindowsClipboardSource;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, HWND_MESSAGE};

/// Every test here drives the one real OS clipboard, so they must not overlap.
static CLIPBOARD: Mutex<()> = Mutex::new(());

/// Writes through a message-only window owned by this test process, the way a
/// real app does. With a NULL owner, `EmptyClipboard` leaves the clipboard
/// ownerless and `SetClipboardData` is not reliable.
fn set_real_clipboard_text(text: &str) {
    set_real_clipboard(text, &[]);
}

/// Like `set_real_clipboard_text`, plus extra named clipboard formats in the
/// same write, each holding a DWORD. Password managers add such formats to
/// tell clipboard tools to leave a copy alone.
fn set_real_clipboard(text: &str, markers: &[(windows::core::PCWSTR, u32)]) {
    let mut formats = vec![(CF_UNICODETEXT.0 as u32, utf16_bytes(text))];
    for (name, value) in markers {
        let format = unsafe { RegisterClipboardFormatW(*name) };
        formats.push((format, value.to_le_bytes().to_vec()));
    }
    set_real_clipboard_formats(&formats);
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn png_format() -> u32 {
    unsafe { RegisterClipboardFormatW(windows::core::w!("PNG")) }
}

/// Replaces the clipboard with the given formats, each holding the given bytes.
fn set_real_clipboard_formats(formats: &[(u32, Vec<u8>)]) {
    unsafe {
        let owner = CreateWindowExW(
            Default::default(),
            windows::core::w!("STATIC"),
            windows::core::w!(""),
            Default::default(),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            None,
            None,
        )
        .expect("CreateWindowExW failed");
        OpenClipboard(owner).expect("OpenClipboard failed");
        EmptyClipboard().expect("EmptyClipboard failed");

        for (format, bytes) in formats {
            let hglobal = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).expect("GlobalAlloc failed");
            let ptr = GlobalLock(hglobal) as *mut u8;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            let _ = GlobalUnlock(hglobal);
            SetClipboardData(*format, windows::Win32::Foundation::HANDLE(hglobal.0))
                .expect("SetClipboardData failed");
        }
        let _ = CloseClipboard();
    }
}

#[test]
fn next_event_is_none_when_nothing_has_happened_yet() {
    let _clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let mut source = WindowsClipboardSource::new();
    assert!(source.next_event().is_none());
}

#[test]
fn capturing_a_real_clipboard_change_produces_a_matching_event() {
    let _clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let mut source = WindowsClipboardSource::new();
    // Give the background listener thread time to register before we change the clipboard.
    std::thread::sleep(Duration::from_millis(200));

    set_real_clipboard_text("clipdeck windows listener round trip");

    let captured = wait_for_event(&mut source);
    assert_eq!(
        captured.content,
        IncomingContent::Text("clipdeck windows listener round trip".into())
    );
}

fn wait_for_event(source: &mut WindowsClipboardSource) -> clip_engine::IncomingClip {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(event) = source.next_event() {
            return event;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("expected a clipboard event within 5 seconds");
}

#[test]
fn a_clipboard_change_reports_the_exe_name_of_the_app_that_made_it() {
    let _clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let mut source = WindowsClipboardSource::new();
    std::thread::sleep(Duration::from_millis(200));

    // set_real_clipboard_text writes through a window owned by this test
    // process, so the source app is the test binary, e.g. "listener-0123abcd.exe".
    set_real_clipboard_text("clipdeck owner check");

    let event = wait_for_event(&mut source);
    let app = event
        .source_app
        .expect("expected the source app to be reported");
    assert!(
        app.starts_with("listener-") && app.ends_with(".exe"),
        "got {app:?}"
    );
}

#[test]
fn apps_copying_through_the_ole_clipboard_are_not_locked_out_while_listening() {
    let _clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let mut source = WindowsClipboardSource::new();
    std::thread::sleep(Duration::from_millis(200));

    // PowerShell's Set-Clipboard copies through the OLE clipboard, like .NET
    // and WPF apps do, and reports an error when it can't finish the copy.
    let failures = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-STA",
            "-Command",
            "$failed = 0; 1..5 | % { try { Set-Clipboard \"ole copy $_\" -ErrorAction Stop } \
             catch { $failed++ }; Start-Sleep -Milliseconds 300 }; exit $failed",
        ])
        .status()
        .expect("couldn't run powershell.exe")
        .code();

    assert_eq!(failures, Some(0), "copies that failed out of 5");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last = None;
    while Instant::now() < deadline && last != Some(IncomingContent::Text("ole copy 5".into())) {
        if let Some(event) = source.next_event() {
            last = Some(event.content);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(last, Some(IncomingContent::Text("ole copy 5".into())));
}

#[test]
fn copies_an_app_marks_as_not_for_clipboard_tools_are_reported_concealed() {
    use windows::core::w;
    let _clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let mut source = WindowsClipboardSource::new();
    std::thread::sleep(Duration::from_millis(200));

    // What KeePass, 1Password and Bitwarden write alongside a password.
    for (text, marker) in [
        (
            "secret one",
            (w!("ExcludeClipboardContentFromMonitorProcessing"), 0),
        ),
        ("secret two", (w!("CanIncludeInClipboardHistory"), 0)),
        ("secret three", (w!("Clipboard Viewer Ignore"), 0)),
    ] {
        set_real_clipboard(text, &[marker]);
        let event = wait_for_event(&mut source);
        assert_eq!(event.content, IncomingContent::Text(text.into()));
        assert!(event.concealed, "{text} should be concealed");
    }

    // An app that says history is fine is captured as usual.
    set_real_clipboard("not secret", &[(w!("CanIncludeInClipboardHistory"), 1)]);
    let event = wait_for_event(&mut source);
    assert_eq!(event.content, IncomingContent::Text("not secret".into()));
    assert!(!event.concealed);
}

/// Red and a half-transparent blue on top, green and white below.
fn four_pixels() -> Rgba {
    Rgba::new(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 0, 255, 128, //
            0, 255, 0, 255, 255, 255, 255, 255,
        ],
    )
    .unwrap()
}

fn expect_image(content: IncomingContent) -> CapturedImage {
    match content {
        IncomingContent::Image(image) => image,
        IncomingContent::Text(text) => panic!("expected an image, got the text {text:?}"),
    }
}

#[test]
fn a_copied_png_is_captured_as_an_image_with_its_transparency() {
    let _clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let mut source = WindowsClipboardSource::new();
    std::thread::sleep(Duration::from_millis(200));
    let picture = four_pixels();

    set_real_clipboard_formats(&[(png_format(), picture.to_png().unwrap())]);

    let image = expect_image(wait_for_event(&mut source).content);
    assert_eq!(Rgba::from_png(image.png()).unwrap(), picture);
}

#[test]
fn a_copied_bitmap_is_captured_as_an_opaque_image_the_right_way_up() {
    let _clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let mut source = WindowsClipboardSource::new();
    std::thread::sleep(Duration::from_millis(200));
    // A 2x2 device-independent bitmap, 32 bits per pixel: a 40-byte header,
    // then the rows bottom to top, each pixel as blue, green, red, unused.
    let mut dib = Vec::new();
    dib.extend(40u32.to_le_bytes());
    dib.extend(2i32.to_le_bytes());
    dib.extend(2i32.to_le_bytes());
    dib.extend(1u16.to_le_bytes());
    dib.extend(32u16.to_le_bytes());
    dib.extend([0u8; 24]);
    dib.extend([0, 255, 0, 0, 255, 255, 255, 0]);
    dib.extend([0, 0, 255, 0, 255, 0, 0, 0]);

    set_real_clipboard_formats(&[(CF_DIB.0 as u32, dib)]);

    let image = expect_image(wait_for_event(&mut source).content);
    let mut opaque = four_pixels();
    opaque.pixels[7] = 255;
    assert_eq!(Rgba::from_png(image.png()).unwrap(), opaque);
}

#[test]
fn a_copy_with_both_text_and_an_image_is_captured_as_text() {
    let _clipboard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let mut source = WindowsClipboardSource::new();
    std::thread::sleep(Duration::from_millis(200));

    set_real_clipboard_formats(&[
        (png_format(), four_pixels().to_png().unwrap()),
        (CF_UNICODETEXT.0 as u32, utf16_bytes("a spreadsheet cell")),
    ]);

    let event = wait_for_event(&mut source);
    assert_eq!(
        event.content,
        IncomingContent::Text("a spreadsheet cell".into())
    );
}
