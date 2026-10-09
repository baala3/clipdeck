#![cfg(target_os = "windows")]

use clip_engine::{ClipContent, ClipboardSource};
use clip_windows::WindowsClipboardSource;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, HWND_MESSAGE};

/// Every test here drives the one real OS clipboard, so they must not overlap.
static CLIPBOARD: Mutex<()> = Mutex::new(());

/// Writes through a message-only window owned by this test process, the way a
/// real app does. With a NULL owner, `EmptyClipboard` leaves the clipboard
/// ownerless and `SetClipboardData` is not reliable.
fn set_real_clipboard_text(text: &str) {
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

        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_len = wide.len() * std::mem::size_of::<u16>();

        let hglobal = GlobalAlloc(GMEM_MOVEABLE, byte_len).expect("GlobalAlloc failed");
        let ptr = GlobalLock(hglobal) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(hglobal);

        SetClipboardData(
            CF_UNICODETEXT.0 as u32,
            windows::Win32::Foundation::HANDLE(hglobal.0),
        )
        .expect("SetClipboardData failed");
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
        ClipContent::Text("clipdeck windows listener round trip".into())
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
    while Instant::now() < deadline && last != Some(ClipContent::Text("ole copy 5".into())) {
        if let Some(event) = source.next_event() {
            last = Some(event.content);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(last, Some(ClipContent::Text("ole copy 5".into())));
}
