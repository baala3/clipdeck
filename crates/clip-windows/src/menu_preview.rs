//! A larger look at an image Clip: resting on its row in one of Clipdeck's
//! native menus shows the picture beside the menu, where a text Clip's
//! tooltip would be.
//!
//! The thumbnail in the row is too small to tell two screenshots apart, and a
//! tooltip can only hold text. So the picture goes in a small window of our
//! own that never takes focus or clicks. The full image is read and scaled
//! down on another thread, since a big screenshot takes a moment and the menu
//! must stay responsive meanwhile.

use clip_engine::Rgba;
use std::cell::RefCell;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, ShowWindow, UpdateLayeredWindow, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// The largest a preview's picture gets, at 96 DPI.
pub(crate) const MAX_WIDTH: u32 = 480;
pub(crate) const MAX_HEIGHT: u32 = 360;
/// The line around the picture, in pixels.
const BORDER: usize = 1;
/// What the line and the space behind a see-through picture are painted
/// with (red, green, blue), to go with a light or a dark menu.
const LIGHT: ([u8; 3], [u8; 3]) = ([160, 160, 160], [249, 249, 249]);
const DARK: ([u8; 3], [u8; 3]) = ([96, 96, 96], [43, 43, 43]);

type Loader = Arc<dyn Fn(&str) -> Option<Rgba> + Send + Sync>;

#[derive(Default)]
struct State {
    loader: Option<Loader>,
    /// The image asked for last, and its picture once that has arrived.
    key: String,
    loading: Option<Receiver<Option<Rgba>>>,
    loaded: Option<Rgba>,
    window: Option<HWND>,
}

// Menus are built, shown, and torn down on the main thread only.
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Sets where previews get their pictures: `loader` is given what the menu's
/// tips call an image (`MenuTip::Image`) and returns it in full, or `None` if
/// it's gone. It runs on a thread of its own.
pub fn on_image_preview(loader: impl Fn(&str) -> Option<Rgba> + Send + Sync + 'static) {
    STATE.with(|state| state.borrow_mut().loader = Some(Arc::new(loader)));
}

/// Starts getting `key`'s picture ready, scaled to fit `max_width` x
/// `max_height`, unless it's the one asked for last.
pub(crate) fn request(key: &str, max_width: u32, max_height: u32) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.key == key && (state.loaded.is_some() || state.loading.is_some()) {
            return;
        }
        let Some(loader) = state.loader.clone() else {
            return;
        };
        let (sender, receiver) = channel();
        state.key = key.to_string();
        state.loaded = None;
        state.loading = Some(receiver);
        let key = key.to_string();
        std::thread::spawn(move || {
            let picture = loader(&key).map(|image| image.fit_within(max_width, max_height));
            let _ = sender.send(picture);
        });
    });
}

/// Shows `key`'s picture beside `menu`, level with `item_top` and `gap` away
/// from it, if it's ready. Returns whether it's showing now.
pub(crate) unsafe fn show(key: &str, menu: RECT, item_top: i32, gap: i32) -> bool {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.key != key {
            return false;
        }
        if let Some(loading) = &state.loading {
            match loading.try_recv() {
                Ok(picture) => state.loaded = picture,
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => {}
            }
            state.loading = None;
        }
        let (Some(picture), Some(window)) = (state.loaded.clone(), state.window()) else {
            return false;
        };
        let (width, height) = (
            picture.width as usize + 2 * BORDER,
            picture.height as usize + 2 * BORDER,
        );
        let at = crate::menu_tips::place_beside(menu, item_top, (width as i32, height as i32), gap);
        if !paint(window, &picture, at, width, height) {
            return false;
        }
        let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
        true
    })
}

pub(crate) unsafe fn hide() {
    STATE.with(|state| {
        if let Some(window) = state.borrow().window {
            let _ = ShowWindow(window, SW_HIDE);
        }
    });
}

impl State {
    /// The preview window, created the first time one is needed.
    unsafe fn window(&mut self) -> Option<HWND> {
        if self.window.is_none() {
            self.window = CreateWindowExW(
                // Click-through, never focused, and absent from the taskbar.
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Clipdeck image preview"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            )
            .ok();
        }
        self.window
    }
}

/// Draws `picture` with a line around it into `window`, and moves the window
/// to `at`. A layered window takes its whole look from one bitmap, so there's
/// no window procedure to paint it.
unsafe fn paint(window: HWND, picture: &Rgba, at: (i32, i32), width: usize, height: usize) -> bool {
    let (line, behind) = if crate::appearance::menus_are_dark() {
        DARK
    } else {
        LIGHT
    };
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            // Negative: rows run top to bottom.
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let Ok(bitmap) = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
        return false;
    };
    if bits.is_null() {
        let _ = DeleteObject(bitmap);
        return false;
    }
    // Blue, green, red, alpha; every pixel is opaque.
    let out = std::slice::from_raw_parts_mut(bits as *mut [u8; 4], width * height);
    out.fill([line[2], line[1], line[0], 255]);
    let rows = picture.pixels.chunks_exact(picture.width as usize * 4);
    for (row, pixels) in rows.enumerate() {
        let start = (BORDER + row) * width + BORDER;
        let out = &mut out[start..start + picture.width as usize];
        for (out, &[red, green, blue, alpha]) in out.iter_mut().zip(pixels.as_chunks::<4>().0) {
            let over = |channel: u8, behind: u8| {
                let alpha = alpha as u32;
                ((channel as u32 * alpha + behind as u32 * (255 - alpha) + 127) / 255) as u8
            };
            *out = [
                over(blue, behind[2]),
                over(green, behind[1]),
                over(red, behind[0]),
                255,
            ];
        }
    }

    let screen = GetDC(None);
    let memory = CreateCompatibleDC(screen);
    let previous = SelectObject(memory, bitmap);
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
        ..Default::default()
    };
    let updated = UpdateLayeredWindow(
        window,
        screen,
        Some(&POINT { x: at.0, y: at.1 }),
        Some(&SIZE {
            cx: width as i32,
            cy: height as i32,
        }),
        memory,
        Some(&POINT::default()),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );
    SelectObject(memory, previous);
    let _ = DeleteDC(memory);
    ReleaseDC(None, screen);
    let _ = DeleteObject(bitmap);
    updated.is_ok()
}
