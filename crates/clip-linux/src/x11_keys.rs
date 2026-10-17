//! The keyboard on X11: which modifier keys are held, which window has it,
//! and pasting by sending Ctrl+V through the XTEST extension.

use clip_engine::menu::HeldModifiers;
use std::sync::OnceLock;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, InputFocus, KeyButMask, Keycode,
    Window,
};
use x11rb::protocol::xproto::{KEY_PRESS_EVENT, KEY_RELEASE_EVENT};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

const KEYSYM_V: u32 = 0x0076;
const KEYSYM_CONTROL_L: u32 = 0xffe3;

type Failed = Box<dyn std::error::Error>;

/// A connection to an X server, for reading and sending keys.
pub struct X11Keys {
    conn: RustConnection,
    root: Window,
}

impl X11Keys {
    /// `None` for `display` means the server in `$DISPLAY`.
    pub fn connect(display: Option<&str>) -> Result<Self, String> {
        let (conn, screen) = RustConnection::connect(display).map_err(|err| err.to_string())?;
        let root = conn.setup().roots[screen].root;
        Ok(Self { conn, root })
    }

    /// The modifier keys held right now.
    pub fn held_modifiers(&self) -> HeldModifiers {
        let mask = (|| -> Result<u16, Failed> {
            Ok(u16::from(self.conn.query_pointer(self.root)?.reply()?.mask))
        })()
        .unwrap_or(0);
        let held = |modifier: KeyButMask| mask & u16::from(modifier) != 0;
        HeldModifiers {
            command_or_control: held(KeyButMask::CONTROL),
            alt: held(KeyButMask::MOD1),
            shift: held(KeyButMask::SHIFT),
        }
    }

    /// Sends Ctrl+V to whichever window has the keyboard focus, so choosing
    /// a Clip pastes it rather than only copying it. Returns whether the
    /// keys were sent.
    ///
    /// Best-effort, like the other platforms: an app is free to ignore
    /// synthetic keys, and a terminal pastes with Ctrl+Shift+V instead. The
    /// Clip is on the clipboard either way.
    pub fn paste_into_focused_app(&self) -> bool {
        self.send_paste().is_ok()
    }

    fn send_paste(&self) -> Result<(), Failed> {
        let setup = self.conn.setup();
        let (first, last) = (setup.min_keycode, setup.max_keycode);
        let mapping = self
            .conn
            .get_keyboard_mapping(first, last - first + 1)?
            .reply()?;
        // The key that types `keysym` unshifted, on the current layout.
        let key_for = |keysym: u32| -> Result<Keycode, Failed> {
            mapping
                .keysyms
                .chunks(mapping.keysyms_per_keycode as usize)
                .position(|keysyms| keysyms.first() == Some(&keysym))
                .map(|index| first + index as Keycode)
                .ok_or_else(|| "the keyboard layout has no such key".into())
        };
        let (control, v) = (key_for(KEYSYM_CONTROL_L)?, key_for(KEYSYM_V)?);

        // A modifier still held from choosing the item (a number shortcut's,
        // or Shift from the hotkey) would turn Ctrl+V into another shortcut,
        // so the server is told those keys went up first.
        let pressed = self.conn.query_keymap()?.reply()?.keys;
        let is_pressed = |key: Keycode| pressed[key as usize / 8] & (1 << (key % 8)) != 0;
        for key in self.conn.get_modifier_mapping()?.reply()?.keycodes {
            if key != 0 && is_pressed(key) {
                self.fake_key(KEY_RELEASE_EVENT, key)?;
            }
        }
        self.fake_key(KEY_PRESS_EVENT, control)?;
        self.fake_key(KEY_PRESS_EVENT, v)?;
        self.fake_key(KEY_RELEASE_EVENT, v)?;
        self.fake_key(KEY_RELEASE_EVENT, control)?;
        Ok(())
    }

    /// The window the window manager says is active (`_NET_ACTIVE_WINDOW`).
    /// `None` without a window manager, or when it names none.
    pub fn active_window(&self) -> Option<Window> {
        (|| -> Result<Option<Window>, Failed> {
            let atom = self.active_window_atom()?;
            let reply = self
                .conn
                .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 1)?
                .reply()?;
            let window = reply.value32().and_then(|mut values| values.next());
            Ok(window.filter(|&window| window != x11rb::NONE))
        })()
        .ok()
        .flatten()
    }

    /// Gives `window` the keyboard back, e.g. the app that was in front before
    /// the glass menu opened. Asks the window manager the way a taskbar does,
    /// so it also raises the window, and sets the input focus directly too,
    /// for when there's no window manager or it doesn't listen. False if the
    /// window is gone or not shown.
    pub fn activate_window(&self, window: Window) -> bool {
        (|| -> Result<(), Failed> {
            let atom = self.active_window_atom()?;
            // 2: the request comes from a pager or taskbar, which window
            // managers honour without focus-stealing checks.
            let request = ClientMessageEvent::new(32, window, atom, [2, 0, 0, 0, 0]);
            self.conn.send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                request,
            )?;
            self.conn
                .set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)?
                .check()?;
            Ok(())
        })()
        .is_ok()
    }

    fn active_window_atom(&self) -> Result<u32, Failed> {
        Ok(self
            .conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")?
            .reply()?
            .atom)
    }

    /// Checked, so a server without XTEST is an error rather than silence.
    fn fake_key(&self, kind: u8, key: Keycode) -> Result<(), Failed> {
        self.conn
            .xtest_fake_input(kind, key, x11rb::CURRENT_TIME, self.root, 0, 0, 0)?
            .check()?;
        Ok(())
    }
}

/// The session's X server, connected to on first use. `None` when there is
/// none to connect to.
fn session() -> Option<&'static X11Keys> {
    static SESSION: OnceLock<Option<X11Keys>> = OnceLock::new();
    SESSION.get_or_init(|| X11Keys::connect(None).ok()).as_ref()
}

/// See [`X11Keys::held_modifiers`]. Nothing is held if there's no X server.
pub fn held_modifiers() -> HeldModifiers {
    session().map(X11Keys::held_modifiers).unwrap_or_default()
}

/// See [`X11Keys::paste_into_focused_app`].
pub fn paste_into_focused_app() -> bool {
    session().is_some_and(X11Keys::paste_into_focused_app)
}

/// See [`X11Keys::active_window`].
pub fn active_window() -> Option<Window> {
    session().and_then(X11Keys::active_window)
}

/// See [`X11Keys::activate_window`].
pub fn activate_window(window: Window) -> bool {
    session().is_some_and(|keys| keys.activate_window(window))
}
