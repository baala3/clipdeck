use clip_engine::{ClipContent, ClipboardSource, IncomingClip};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::thread;

use windows::core::{w, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, GetClipboardOwner, OpenClipboard,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW,
    GetWindowLongPtrW, GetWindowThreadProcessId, KillTimer, RegisterClassW, SetTimer,
    SetWindowLongPtrW, TranslateMessage, GWLP_USERDATA, HWND_MESSAGE, MSG, WM_CLIPBOARDUPDATE,
    WM_TIMER, WNDCLASSW,
};

/// How long after a clipboard change to wait before reading it. The OLE
/// clipboard (.NET, WPF, PowerShell's Set-Clipboard) announces a copy before
/// it's finished, then reopens the clipboard to render the data. Reading
/// straight away holds the clipboard open while waiting on that very app to
/// render, so its reopen fails and the user sees their copy fail. Each new
/// change restarts the wait, so a burst of writes is read once, when it's done.
const READ_DELAY_MS: u32 = 100;
const READ_TIMER_ID: usize = 1;

type Queue = Mutex<VecDeque<IncomingClip>>;

pub struct WindowsClipboardSource {
    queue_ptr: *mut Queue,
}

unsafe impl Send for WindowsClipboardSource {}

impl WindowsClipboardSource {
    pub fn new() -> Self {
        let queue_ptr: *mut Queue = Box::into_raw(Box::new(Mutex::new(VecDeque::new())));
        let queue_addr = queue_ptr as usize;

        thread::spawn(move || unsafe {
            run_listener_message_loop(queue_addr as *mut Queue);
        });

        Self { queue_ptr }
    }
}

impl Default for WindowsClipboardSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardSource for WindowsClipboardSource {
    fn next_event(&mut self) -> Option<IncomingClip> {
        let queue = unsafe { &*self.queue_ptr };
        queue.lock().unwrap().pop_front()
    }
}

unsafe fn run_listener_message_loop(queue_ptr: *mut Queue) {
    let hinstance = GetModuleHandleW(None).expect("GetModuleHandleW failed");
    let class_name = w!("ClipdeckClipboardListener");

    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance.into(),
        lpszClassName: class_name,
        ..Default::default()
    };
    RegisterClassW(&wc);

    let hwnd = CreateWindowExW(
        Default::default(),
        class_name,
        w!(""),
        Default::default(),
        0,
        0,
        0,
        0,
        HWND_MESSAGE,
        None,
        hinstance,
        None,
    )
    .expect("CreateWindowExW failed");

    SetWindowLongPtrW(hwnd, GWLP_USERDATA, queue_ptr as isize);
    let _ = AddClipboardFormatListener(hwnd);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).into() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        SetTimer(hwnd, READ_TIMER_ID, READ_DELAY_MS, None);
        return LRESULT(0);
    }
    if msg == WM_TIMER && wparam.0 == READ_TIMER_ID {
        let _ = KillTimer(hwnd, READ_TIMER_ID);
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Queue;
        if !ptr.is_null() {
            if let Some(clip) = read_clipboard_text(hwnd) {
                let queue = &*ptr;
                queue.lock().unwrap().push_back(clip);
            }
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Another process (the app that just copied, or another clipboard tool reacting
/// to the same change) may still hold the clipboard open, in which case
/// `OpenClipboard` fails. Retry briefly rather than silently dropping the Clip.
///
/// Opens with the listener's own window rather than NULL: a NULL open is not
/// tied to a window, so a second listener in the same process could close the
/// clipboard out from under this one mid-read.
unsafe fn open_clipboard_with_retry(listener: HWND) -> bool {
    for _ in 0..20 {
        if OpenClipboard(listener).is_ok() {
            return true;
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

unsafe fn read_clipboard_text(listener: HWND) -> Option<IncomingClip> {
    if !open_clipboard_with_retry(listener) {
        return None;
    }

    let text = (|| -> Option<String> {
        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
        let ptr = GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0)) as *const u16;
        if ptr.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        let text = String::from_utf16_lossy(slice);
        let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
        Some(text)
    })();

    let _ = CloseClipboard();

    text.map(|text| IncomingClip {
        content: ClipContent::Text(text),
        source_app: source_app_name(),
        concealed: false,
    })
}

/// The executable name (e.g. "KeePass.exe") of the app that just wrote to the
/// clipboard, for matching against the exclusion list. Uses the clipboard
/// owner window, falling back to the foreground window for apps that write
/// without an owner.
unsafe fn source_app_name() -> Option<String> {
    let hwnd = GetClipboardOwner().unwrap_or_else(|_| GetForegroundWindow());
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return None;
    }
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let queried = QueryFullProcessImageNameW(
        process,
        PROCESS_NAME_WIN32,
        PWSTR(buf.as_mut_ptr()),
        &mut len,
    );
    let _ = CloseHandle(process);
    queried.ok()?;
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit('\\').next().map(str::to_string)
}
