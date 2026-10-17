use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND,
};

/// Gives a borderless window Windows 11's rounded corners without the thin
/// border Windows draws along them. Older Windows has neither, and the calls
/// do nothing there.
pub fn round_without_border(handle: isize) {
    let hwnd = HWND(handle as *mut core::ffi::c_void);
    let corners = DWMWCP_ROUND;
    let border = DWMWA_COLOR_NONE;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corners as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&corners) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// Moves and resizes a window in one step, without activating it. Moving
/// and then resizing would show it for a moment at its new place in its old
/// size.
pub fn set_window_bounds(handle: isize, x: i32, y: i32, width: i32, height: i32) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER,
    };
    unsafe {
        let _ = SetWindowPos(
            HWND(handle as *mut core::ffi::c_void),
            None,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
        );
    }
}
