//! Thumbnails for the image Clips in Clipdeck's native menus.
//!
//! Tauri can put an icon on a menu item, but on Windows it squeezes it into
//! 16x16 pixels, too small to tell one screenshot from another. A Win32 menu
//! item takes a bitmap of any size though, and makes its row as tall as the
//! bitmap, so the thumbnails are set here, straight on the native menu.

use crate::MenuSlot;
use clip_engine::Rgba;
use std::cell::RefCell;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    CreateDIBSection, DeleteObject, MonitorFromPoint, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSubMenu, SetMenuItemInfoW, HMENU, MENUITEMINFOW, MIIM_BITMAP,
};

/// The size of every thumbnail's bitmap, at 96 DPI. A menu lines its items'
/// text up after the widest bitmap, so they're all made the same size, with
/// the thumbnail centered: a column of pictures instead of a ragged edge.
const WIDTH: u32 = 48;
const HEIGHT: u32 = 24;

/// The thumbnail for one row of a menu.
pub struct MenuImage {
    /// The row's position in the menu.
    pub row: usize,
    /// For a row inside a folder: its position in the submenu at `row`.
    pub folder_row: Option<usize>,
    pub image: Rgba,
}

/// A menu doesn't own its items' bitmaps, so they're kept here and deleted
/// once the menu is gone.
#[derive(Default)]
struct Bitmaps {
    current: Vec<isize>,
    /// The bitmaps of the menu that held this slot before. The tray menu can
    /// be replaced while it's open, so they outlive it by one round.
    retired: Vec<isize>,
}

impl Bitmaps {
    fn replace(&mut self, bitmaps: Vec<isize>) {
        for bitmap in self.retired.drain(..) {
            unsafe {
                let _ = DeleteObject(HBITMAP(bitmap as *mut _));
            }
        }
        self.retired = std::mem::replace(&mut self.current, bitmaps);
    }
}

// Menus are built, shown, and torn down on the main thread only.
thread_local! {
    static BITMAPS: RefCell<[Bitmaps; 2]> = RefCell::new(Default::default());
}

/// Puts `images` on the rows of `hmenu`, replacing whatever menu held `slot`
/// before. Call it on the thread that shows the menu.
pub fn set_menu_images(slot: MenuSlot, hmenu: isize, images: Vec<MenuImage>) {
    let scale = unsafe { cursor_monitor_dpi() };
    let (width, height) = (WIDTH * scale / 96, HEIGHT * scale / 96);
    let mut bitmaps = Vec::new();
    for item in images {
        let thumbnail = item.image.fit_within(width, height);
        let Some(bitmap) = (unsafe { centered_bitmap(&thumbnail, width, height) }) else {
            continue;
        };
        bitmaps.push(bitmap.0 as isize);
        unsafe {
            let (menu, position) = match item.folder_row {
                Some(folder_row) => (
                    GetSubMenu(HMENU(hmenu as *mut _), item.row as i32),
                    folder_row,
                ),
                None => (HMENU(hmenu as *mut _), item.row),
            };
            let info = MENUITEMINFOW {
                cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_BITMAP,
                hbmpItem: bitmap,
                ..Default::default()
            };
            let _ = SetMenuItemInfoW(menu, position as u32, true, &info);
        }
    }
    BITMAPS.with(|slots| slots.borrow_mut()[slot as usize].replace(bitmaps));
}

/// Forgets the menu in `slot`, once it's gone.
pub fn clear_menu_images(slot: MenuSlot) {
    BITMAPS.with(|slots| slots.borrow_mut()[slot as usize].replace(Vec::new()));
}

/// The DPI of the monitor the mouse is on, which is where a menu opens.
unsafe fn cursor_monitor_dpi() -> u32 {
    let mut cursor = POINT::default();
    let _ = GetCursorPos(&mut cursor);
    let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    let (mut dpi, mut dpi_y) = (96, 96);
    let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y);
    dpi.max(96)
}

/// A `width` x `height` bitmap with `image`, which must fit, in its middle
/// and nothing around it. It's in the form a menu draws with transparency: 32
/// bits per pixel, as blue, green, red, alpha, with the colors already
/// multiplied by the alpha.
unsafe fn centered_bitmap(image: &Rgba, width: u32, height: u32) -> Option<HBITMAP> {
    if image.width > width || image.height > height {
        return None;
    }
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
    let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
    if bits.is_null() {
        return None;
    }
    // A new bitmap is all zeroes, which is fully transparent.
    let (width, height) = (width as usize, height as usize);
    let out = std::slice::from_raw_parts_mut(bits as *mut [u8; 4], width * height);
    let (left, top) = (
        (width - image.width as usize) / 2,
        (height - image.height as usize) / 2,
    );
    let rows = image.pixels.chunks_exact(image.width as usize * 4);
    for (row, pixels) in rows.enumerate() {
        let start = (top + row) * width + left;
        let out = &mut out[start..start + image.width as usize];
        for (out, &[red, green, blue, alpha]) in out.iter_mut().zip(pixels.as_chunks::<4>().0) {
            let premultiply = |channel: u8| ((channel as u32 * alpha as u32 + 127) / 255) as u8;
            *out = [
                premultiply(blue),
                premultiply(green),
                premultiply(red),
                alpha,
            ];
        }
    }
    Some(bitmap)
}
