use clip_engine::{ClipContent, ClipboardSource, IncomingClip};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::thread;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, OpenClipboard,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowLongPtrW,
    RegisterClassW, SetWindowLongPtrW, TranslateMessage, GWLP_USERDATA, HWND_MESSAGE, MSG,
    WM_CLIPBOARDUPDATE, WNDCLASSW,
};

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
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Queue;
        if !ptr.is_null() {
            if let Some(clip) = read_clipboard_text() {
                let queue = &*ptr;
                queue.lock().unwrap().push_back(clip);
            }
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn read_clipboard_text() -> Option<IncomingClip> {
    OpenClipboard(None).ok()?;

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
        source_app: None,
        concealed: false,
    })
}
