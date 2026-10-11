//! A private X server for each test, and a stand-in for an app that copies.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux, EventMask,
    PropMode, Property, SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
    SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// A headless X server of the test's own, so nothing touches the clipboard
/// of the session the tests are run from. Stops when dropped.
pub struct XServer {
    process: Child,
    pub display: String,
}

impl XServer {
    pub fn start() -> Self {
        // X servers normally listen on a socket in /tmp/.X11-unix. Where that
        // folder can't be written to (WSL mounts it read-only), fall back to
        // TCP on this machine.
        let sockets = std::path::Path::new("/tmp/.X11-unix");
        let probe = sockets.join(format!("clipdeck-probe-{}", std::process::id()));
        let unix_sockets = std::fs::create_dir_all(sockets).is_ok()
            && std::fs::write(&probe, b"").is_ok()
            && std::fs::remove_file(&probe).is_ok();
        let (transport, host): (&[&str], &str) = if unix_sockets {
            (&["-nolisten", "tcp"], "")
        } else {
            (
                &["-nolisten", "unix", "-nolisten", "local", "-listen", "tcp"],
                "127.0.0.1",
            )
        };
        let mut process = Command::new("Xvfb")
            // Picks a free display number and prints it.
            .args(["-displayfd", "1", "-screen", "0", "640x480x24"])
            .args(transport)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("these tests need Xvfb installed (apt install xvfb)");
        let mut number = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut number)
            .expect("Xvfb didn't report its display");
        Self {
            process,
            display: format!("{host}:{}", number.trim()),
        }
    }

    pub fn connect(&self) -> (RustConnection, Window) {
        let (conn, screen) = RustConnection::connect(Some(&self.display)).unwrap();
        let root = conn.setup().roots[screen].root;
        (conn, root)
    }
}

impl Drop for XServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

pub fn atom(conn: &RustConnection, name: &str) -> Atom {
    conn.intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom
}

pub fn window(conn: &RustConnection, root: Window, events: EventMask) -> Window {
    let window = conn.generate_id().unwrap();
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        window,
        root,
        0,
        0,
        10,
        10,
        0,
        WindowClass::INPUT_OUTPUT,
        x11rb::COPY_FROM_PARENT,
        &CreateWindowAux::new().event_mask(events),
    )
    .unwrap();
    window
}

/// How the stand-in app hands its copy over.
#[derive(Clone, Copy)]
pub enum Transfer {
    /// In one piece, as small copies are.
    Whole,
    /// Piece by piece (the INCR protocol), `0` bytes at a time, as large ones are.
    Pieces(usize),
    /// Never: the app took the clipboard and then hung.
    Never,
}

/// Copies `formats` (target name and bytes) the way an app does: it takes
/// the clipboard and then answers whoever asks for the data. Returns once
/// the clipboard is taken; the app keeps answering for the rest of the test.
pub fn copy(server: &XServer, formats: &[(&str, &[u8])], transfer: Transfer) {
    let formats: Vec<(String, Vec<u8>)> = formats
        .iter()
        .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
        .collect();
    let (conn, root) = server.connect();
    let (taken, wait_until_taken) = mpsc::channel();
    thread::spawn(move || {
        let formats: Vec<(Atom, Vec<u8>)> = formats
            .into_iter()
            .map(|(name, bytes)| (atom(&conn, &name), bytes))
            .collect();
        let owner = window(&conn, root, EventMask::NO_EVENT);
        conn.change_property8(
            PropMode::REPLACE,
            owner,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"copier\0Copier\0",
        )
        .unwrap();
        let clipboard = atom(&conn, "CLIPBOARD");
        conn.set_selection_owner(owner, clipboard, x11rb::CURRENT_TIME)
            .unwrap();
        conn.sync().unwrap();
        taken.send(()).unwrap();
        while let Ok(event) = conn.wait_for_event() {
            match event {
                Event::SelectionRequest(request) => answer(&conn, &request, &formats, transfer),
                // Another copy replaced this one.
                Event::SelectionClear(_) => return,
                _ => {}
            }
        }
    });
    wait_until_taken.recv().unwrap();
}

fn answer(
    conn: &RustConnection,
    request: &SelectionRequestEvent,
    formats: &[(Atom, Vec<u8>)],
    transfer: Transfer,
) {
    if matches!(transfer, Transfer::Never) {
        return;
    }
    let mut property = request.property;
    if request.target == atom(conn, "TARGETS") {
        let targets: Vec<Atom> = formats.iter().map(|(target, _)| *target).collect();
        conn.change_property32(
            PropMode::REPLACE,
            request.requestor,
            property,
            AtomEnum::ATOM,
            &targets,
        )
        .unwrap();
    } else if let Some((target, bytes)) = formats.iter().find(|(t, _)| *t == request.target) {
        match transfer {
            Transfer::Pieces(size) => {
                send_in_pieces(conn, request, *target, bytes, size);
                return;
            }
            _ => {
                conn.change_property8(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    *target,
                    bytes,
                )
                .unwrap();
            }
        }
    } else {
        // "I don't have it in that form."
        property = x11rb::NONE;
    }
    notify(conn, request, property);
}

fn notify(conn: &RustConnection, request: &SelectionRequestEvent, property: Atom) {
    let event = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        sequence: 0,
        time: request.time,
        requestor: request.requestor,
        selection: request.selection,
        target: request.target,
        property,
    };
    conn.send_event(false, request.requestor, EventMask::NO_EVENT, event)
        .unwrap();
    conn.flush().unwrap();
}

/// The sending side of INCR: announce the size, then put the next piece in
/// the property each time the receiver deletes the last, ending with an
/// empty one.
fn send_in_pieces(
    conn: &RustConnection,
    request: &SelectionRequestEvent,
    target: Atom,
    bytes: &[u8],
    size: usize,
) {
    let (requestor, property) = (request.requestor, request.property);
    conn.change_window_attributes(
        requestor,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )
    .unwrap();
    conn.change_property32(
        PropMode::REPLACE,
        requestor,
        property,
        atom(conn, "INCR"),
        &[bytes.len() as u32],
    )
    .unwrap();
    notify(conn, request, property);
    let mut pieces = bytes.chunks(size).chain(std::iter::once(&[][..]));
    while let Ok(event) = conn.wait_for_event() {
        let Event::PropertyNotify(change) = event else {
            continue;
        };
        if change.window != requestor || change.atom != property || change.state != Property::DELETE
        {
            continue;
        }
        let Some(piece) = pieces.next() else {
            return;
        };
        conn.change_property8(PropMode::REPLACE, requestor, property, target, piece)
            .unwrap();
        conn.flush().unwrap();
    }
}
