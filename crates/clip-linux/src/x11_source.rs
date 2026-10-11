//! Clipboard capture on X11.
//!
//! X11 has no clipboard as such: the app that copied owns the CLIPBOARD
//! selection and hands the data over when asked. The XFIXES extension says
//! when the owner changes, so there's no polling; each change is then read
//! by asking the new owner for what it offers.

use clip_engine::{CapturedImage, ClipboardSource, IncomingClip, IncomingContent};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::res::{self, ConnectionExt as _};
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, Property, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

/// How long the app that copied gets to answer each request. One that never
/// answers (it hung, or exited mid-copy) must not stall capture for good.
const REPLY_TIMEOUT: Duration = Duration::from_secs(1);
/// A copy larger than this is given up on rather than read to the end.
const MAX_BYTES: usize = 4 * clip_engine::MAX_IMAGE_PNG_BYTES;

type Queue = Arc<Mutex<VecDeque<IncomingClip>>>;
type Failed = Box<dyn std::error::Error>;

pub struct X11ClipboardSource {
    queue: Queue,
}

impl X11ClipboardSource {
    /// Starts watching the clipboard of the X server `display` names
    /// (`None`: the one in `$DISPLAY`). Fails when there's no such server, or
    /// it lacks the XFIXES extension.
    pub fn connect(display: Option<&str>) -> Result<Self, String> {
        let listener = Listener::connect(display).map_err(|err| err.to_string())?;
        let queue = Queue::default();
        let worker_queue = Arc::clone(&queue);
        thread::spawn(move || listener.run(worker_queue));
        Ok(Self { queue })
    }
}

impl ClipboardSource for X11ClipboardSource {
    fn next_event(&mut self) -> Option<IncomingClip> {
        self.queue.lock().unwrap().pop_front()
    }
}

struct Atoms {
    clipboard: Atom,
    targets: Atom,
    incr: Atom,
    utf8_string: Atom,
    text_plain_utf8: Atom,
    image_png: Atom,
    /// What KDE's clipboard manager looks for, and password managers set:
    /// present with the value "secret", the copy is not for clipboard tools.
    password_manager_hint: Atom,
    /// The property on our window that the data arrives in.
    transfer: Atom,
}

struct Listener {
    conn: RustConnection,
    window: Window,
    atoms: Atoms,
    /// Clipboard changes that arrived while an earlier one was being read.
    pending: VecDeque<xfixes::SelectionNotifyEvent>,
}

impl Listener {
    fn connect(display: Option<&str>) -> Result<Self, Failed> {
        let (conn, screen) = RustConnection::connect(display)?;
        let atom = |name: &str| -> Result<Atom, Failed> {
            Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
        };
        let atoms = Atoms {
            clipboard: atom("CLIPBOARD")?,
            targets: atom("TARGETS")?,
            incr: atom("INCR")?,
            utf8_string: atom("UTF8_STRING")?,
            text_plain_utf8: atom("text/plain;charset=utf-8")?,
            image_png: atom("image/png")?,
            password_manager_hint: atom("x-kde-passwordManagerHint")?,
            transfer: atom("CLIPDECK_TRANSFER")?,
        };
        // Never shown. It only exists to receive the clipboard's data, which
        // arrives as a property on a window.
        let window = conn.generate_id()?;
        conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            conn.setup().roots[screen].root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        conn.xfixes_query_version(5, 0)?.reply()?;
        conn.xfixes_select_selection_input(
            window,
            atoms.clipboard,
            xfixes::SelectionEventMask::SET_SELECTION_OWNER,
        )?;
        conn.flush()?;
        Ok(Self {
            conn,
            window,
            atoms,
            pending: VecDeque::new(),
        })
    }

    fn run(mut self, queue: Queue) {
        loop {
            let change = match self.pending.pop_front() {
                Some(change) => change,
                None => match self.conn.wait_for_event() {
                    Ok(Event::XfixesSelectionNotify(change)) => change,
                    Ok(_) => continue,
                    // The X server is gone, and the session with it.
                    Err(_) => return,
                },
            };
            // Only the newest of a burst of copies is still there to read.
            if !self.pending.is_empty() || change.owner == x11rb::NONE {
                continue;
            }
            if let Ok(Some(clip)) = self.read(&change) {
                queue.lock().unwrap().push_back(clip);
            }
        }
    }

    /// Reads the copy as text if it has any, and as an image otherwise. A
    /// copy with both (spreadsheet cells, for one) is meant as text.
    fn read(
        &mut self,
        change: &xfixes::SelectionNotifyEvent,
    ) -> Result<Option<IncomingClip>, Failed> {
        let Some(targets) = self.fetch(self.atoms.targets, change.timestamp)? else {
            return Ok(None);
        };
        let targets: Vec<Atom> = targets
            .as_chunks::<4>()
            .0
            .iter()
            .map(|atom| Atom::from_ne_bytes(*atom))
            .collect();
        let offers = |target: Atom| targets.contains(&target);

        let concealed = offers(self.atoms.password_manager_hint)
            && self
                .fetch(self.atoms.password_manager_hint, change.timestamp)?
                .is_some_and(|hint| hint == b"secret");
        let text_target = [self.atoms.utf8_string, self.atoms.text_plain_utf8]
            .into_iter()
            .find(|target| offers(*target));
        let content = if let Some(target) = text_target {
            self.fetch(target, change.timestamp)?
                .map(|text| IncomingContent::Text(String::from_utf8_lossy(&text).into_owned()))
        } else if offers(self.atoms.image_png) {
            self.fetch(self.atoms.image_png, change.timestamp)?
                .and_then(|png| CapturedImage::from_png(&png))
                .map(IncomingContent::Image)
        } else {
            None
        };
        Ok(content.map(|content| IncomingClip {
            content,
            source_app: self.app_name(change.owner),
            concealed,
        }))
    }

    /// Asks the clipboard's owner for the copy in the form `target`. `None`
    /// when the owner doesn't have that form, or doesn't answer in time.
    fn fetch(&mut self, target: Atom, time: u32) -> Result<Option<Vec<u8>>, Failed> {
        let transfer = self.atoms.transfer;
        self.conn.delete_property(self.window, transfer)?;
        self.conn
            .convert_selection(self.window, self.atoms.clipboard, target, transfer, time)?;
        self.conn.flush()?;
        let answered = self.wait_for(|event| match event {
            Event::SelectionNotify(answer) => Some(answer.property != x11rb::NONE),
            _ => None,
        })?;
        if answered != Some(true) {
            return Ok(None);
        }
        let reply = self
            .conn
            .get_property(true, self.window, transfer, AtomEnum::ANY, 0, u32::MAX)?
            .reply()?;
        if reply.type_ != self.atoms.incr {
            return Ok(Some(reply.value));
        }
        // Too large for one property: deleting it (just done) tells the
        // owner to send the data piece by piece, ending with an empty piece.
        let mut data = Vec::new();
        loop {
            let piece_arrived = self.wait_for(|event| match event {
                Event::PropertyNotify(change)
                    if change.atom == transfer && change.state == Property::NEW_VALUE =>
                {
                    Some(())
                }
                _ => None,
            })?;
            if piece_arrived.is_none() {
                return Ok(None);
            }
            let piece = self
                .conn
                .get_property(true, self.window, transfer, AtomEnum::ANY, 0, u32::MAX)?
                .reply()?
                .value;
            if piece.is_empty() {
                return Ok(Some(data));
            }
            data.extend(piece);
            if data.len() > MAX_BYTES {
                return Ok(None);
            }
        }
    }

    /// Waits for the event `wanted` picks out. Clipboard changes that arrive
    /// meanwhile are kept for later; anything else is dropped.
    fn wait_for<T>(&mut self, wanted: impl Fn(&Event) -> Option<T>) -> Result<Option<T>, Failed> {
        let deadline = Instant::now() + REPLY_TIMEOUT;
        while Instant::now() < deadline {
            match self.conn.poll_for_event()? {
                Some(Event::XfixesSelectionNotify(change)) => self.pending.push_back(change),
                Some(event) => {
                    if let Some(found) = wanted(&event) {
                        return Ok(Some(found));
                    }
                }
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
        Ok(None)
    }

    /// The name of the program that owns `window` (e.g. "keepassxc"), for
    /// matching against the Exclusion list. The X server knows which process
    /// each window belongs to; where it won't say, the window's class is the
    /// next best thing.
    fn app_name(&self, window: Window) -> Option<String> {
        self.process_name(window)
            .or_else(|| self.class_name(window))
    }

    fn process_name(&self, window: Window) -> Option<String> {
        let spec = res::ClientIdSpec {
            client: window,
            mask: res::ClientIdMask::LOCAL_CLIENT_PID,
        };
        let reply = self.conn.res_query_client_ids(&[spec]).ok()?.reply().ok()?;
        let pid = *reply.ids.first()?.value.first()?;
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
        exe.and_then(|exe| Some(exe.file_name()?.to_string_lossy().into_owned()))
            .or_else(|| {
                let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
                Some(comm.trim_end().to_string())
            })
    }

    /// WM_CLASS holds two strings, the instance and the class; the class
    /// ("Firefox", "KeePassXC") names the app.
    fn class_name(&self, window: Window) -> Option<String> {
        let reply = self
            .conn
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        let class = reply.value.split(|byte| *byte == 0).nth(1)?;
        (!class.is_empty()).then(|| String::from_utf8_lossy(class).into_owned())
    }
}
