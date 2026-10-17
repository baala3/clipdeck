#![cfg(target_os = "linux")]

mod common;

use clip_engine::menu::HeldModifiers;
use clip_engine::{ClipboardSource, IncomingClip, IncomingContent, Rgba};
use clip_linux::{X11ClipboardSource, X11Keys};
use common::{copy, window, Transfer, XServer};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::KEY_PRESS_EVENT;
use x11rb::protocol::xproto::{ConnectionExt as _, EventMask, InputFocus, KeyButMask};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

fn watch(server: &XServer) -> X11ClipboardSource {
    X11ClipboardSource::connect(Some(&server.display)).unwrap()
}

fn wait_for_clip(source: &mut X11ClipboardSource) -> IncomingClip {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(clip) = source.next_event() {
            return clip;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("expected a Clip within 5 seconds");
}

fn text(text: &str) -> IncomingContent {
    IncomingContent::Text(text.into())
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

fn expect_image(content: IncomingContent) -> Rgba {
    match content {
        IncomingContent::Image(image) => Rgba::from_png(image.png()).unwrap(),
        IncomingContent::Text(text) => panic!("expected an image, got the text {text:?}"),
    }
}

#[test]
fn copied_text_is_captured_with_the_name_of_the_program_that_copied_it() {
    let server = XServer::start();
    let mut source = watch(&server);

    copy(
        &server,
        &[("UTF8_STRING", "héllo from X11".as_bytes())],
        Transfer::Whole,
    );

    let clip = wait_for_clip(&mut source);
    assert_eq!(clip.content, text("héllo from X11"));
    assert!(!clip.concealed);
    // The copy came from this test's own process, e.g. "x11-0123abcd". An X
    // server reached over TCP doesn't know its clients' processes, and then
    // the copying window's class stands in.
    let app = clip.source_app.expect("expected the source app");
    assert!(app.starts_with("x11-") || app == "Copier", "got {app:?}");
}

#[test]
fn each_new_copy_is_captured() {
    let server = XServer::start();
    let mut source = watch(&server);

    for copied in ["one", "two", "three"] {
        copy(
            &server,
            &[("UTF8_STRING", copied.as_bytes())],
            Transfer::Whole,
        );
        assert_eq!(wait_for_clip(&mut source).content, text(copied));
    }
}

#[test]
fn a_copied_png_is_captured_as_an_image_with_its_transparency() {
    let server = XServer::start();
    let mut source = watch(&server);
    let picture = four_pixels();

    copy(
        &server,
        &[("image/png", &picture.to_png().unwrap())],
        Transfer::Whole,
    );

    assert_eq!(expect_image(wait_for_clip(&mut source).content), picture);
}

#[test]
fn a_copy_with_both_text_and_an_image_is_captured_as_text() {
    let server = XServer::start();
    let mut source = watch(&server);

    copy(
        &server,
        &[
            ("image/png", &four_pixels().to_png().unwrap()),
            ("text/plain;charset=utf-8", b"a spreadsheet cell"),
        ],
        Transfer::Whole,
    );

    assert_eq!(
        wait_for_clip(&mut source).content,
        text("a spreadsheet cell")
    );
}

#[test]
fn a_copy_a_password_manager_marks_as_secret_is_reported_concealed() {
    let server = XServer::start();
    let mut source = watch(&server);

    copy(
        &server,
        &[
            ("UTF8_STRING", b"hunter2"),
            ("x-kde-passwordManagerHint", b"secret"),
        ],
        Transfer::Whole,
    );
    let secret = wait_for_clip(&mut source);
    copy(
        &server,
        &[
            ("UTF8_STRING", b"not a password"),
            ("x-kde-passwordManagerHint", b"public"),
        ],
        Transfer::Whole,
    );
    let public = wait_for_clip(&mut source);

    assert!(secret.concealed);
    assert!(!public.concealed);
}

#[test]
fn a_large_copy_sent_piece_by_piece_arrives_whole() {
    let server = XServer::start();
    let mut source = watch(&server);
    // Noise, so the PNG doesn't compress to nothing: about a megabyte.
    let mut seed = 1u32;
    let pixels: Vec<u8> = (0..512 * 512 * 4)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 24) as u8
        })
        .collect();
    let picture = Rgba::new(512, 512, pixels).unwrap();
    let png = picture.to_png().unwrap();
    assert!(png.len() > 500_000);

    copy(&server, &[("image/png", &png)], Transfer::Pieces(64 * 1024));

    assert_eq!(expect_image(wait_for_clip(&mut source).content), picture);
}

#[test]
fn an_app_that_never_hands_its_copy_over_does_not_stop_later_captures() {
    let server = XServer::start();
    let mut source = watch(&server);

    copy(&server, &[("UTF8_STRING", b"never sent")], Transfer::Never);
    std::thread::sleep(Duration::from_millis(100));
    copy(
        &server,
        &[("UTF8_STRING", b"after the hang")],
        Transfer::Whole,
    );

    assert_eq!(wait_for_clip(&mut source).content, text("after the hang"));
}

/// The key events a focused window receives, as (pressed, keysym, modifiers held).
fn typed_into_a_focused_window(
    server: &XServer,
    act: impl FnOnce(&x11rb::rust_connection::RustConnection, u32),
) -> Vec<(bool, u32, u16)> {
    let (conn, root) = server.connect();
    let target = window(&conn, root, EventMask::KEY_PRESS | EventMask::KEY_RELEASE);
    conn.map_window(target).unwrap();
    conn.sync().unwrap();
    // No window manager here to hand out the focus.
    conn.set_input_focus(InputFocus::POINTER_ROOT, target, x11rb::CURRENT_TIME)
        .unwrap();
    conn.sync().unwrap();

    act(&conn, root);

    conn.sync().unwrap();
    let setup = conn.setup();
    let mapping = conn
        .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)
        .unwrap()
        .reply()
        .unwrap();
    let keysym = |key: u8| {
        mapping.keysyms[(key - setup.min_keycode) as usize * mapping.keysyms_per_keycode as usize]
    };
    let mut typed = Vec::new();
    while let Some(event) = conn.poll_for_event().unwrap() {
        match event {
            Event::KeyPress(key) => typed.push((true, keysym(key.detail), u16::from(key.state))),
            Event::KeyRelease(key) => typed.push((false, keysym(key.detail), u16::from(key.state))),
            _ => {}
        }
    }
    typed
}

const CONTROL_L: u32 = 0xffe3;
const SHIFT_L: u32 = 0xffe1;
const V: u32 = 0x76;

/// The key that types `keysym`, for holding it down by hand.
fn key_for(conn: &x11rb::rust_connection::RustConnection, keysym: u32) -> u8 {
    let setup = conn.setup();
    let mapping = conn
        .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)
        .unwrap()
        .reply()
        .unwrap();
    let index = mapping
        .keysyms
        .chunks(mapping.keysyms_per_keycode as usize)
        .position(|keysyms| keysyms[0] == keysym)
        .unwrap();
    setup.min_keycode + index as u8
}

#[test]
fn pasting_sends_control_v_to_the_focused_window() {
    let server = XServer::start();
    let keys = X11Keys::connect(Some(&server.display)).unwrap();

    let typed = typed_into_a_focused_window(&server, |_, _| {
        assert!(keys.paste_into_focused_app());
    });

    let control = u16::from(KeyButMask::CONTROL);
    assert_eq!(
        typed,
        [
            (true, CONTROL_L, 0),
            (true, V, control),
            (false, V, control),
            (false, CONTROL_L, control),
        ]
    );
}

#[test]
fn a_modifier_still_held_from_choosing_the_clip_is_let_go_before_pasting() {
    let server = XServer::start();
    let keys = X11Keys::connect(Some(&server.display)).unwrap();

    let typed = typed_into_a_focused_window(&server, |conn, root| {
        let shift = key_for(conn, SHIFT_L);
        conn.xtest_fake_input(KEY_PRESS_EVENT, shift, x11rb::CURRENT_TIME, root, 0, 0, 0)
            .unwrap();
        conn.sync().unwrap();
        assert_eq!(
            keys.held_modifiers(),
            HeldModifiers {
                shift: true,
                ..Default::default()
            }
        );
        assert!(keys.paste_into_focused_app());
    });

    let v_pressed = typed
        .iter()
        .find(|(pressed, keysym, _)| *pressed && *keysym == V)
        .expect("V was never pressed");
    assert_eq!(
        v_pressed.2,
        u16::from(KeyButMask::CONTROL),
        "V must arrive with Control alone, not Shift"
    );
}

#[test]
fn no_modifiers_are_held_when_no_keys_are_down() {
    let server = XServer::start();
    let keys = X11Keys::connect(Some(&server.display)).unwrap();

    assert_eq!(keys.held_modifiers(), HeldModifiers::default());
}

#[test]
fn the_active_window_is_the_one_the_window_manager_names() {
    let server = XServer::start();
    let (conn, root) = server.connect();
    let keys = X11Keys::connect(Some(&server.display)).unwrap();
    assert_eq!(keys.active_window(), None, "no window manager, so none yet");

    let editor = window(&conn, root, EventMask::NO_EVENT);
    let active = common::atom(&conn, "_NET_ACTIVE_WINDOW");
    conn.change_property32(
        x11rb::protocol::xproto::PropMode::REPLACE,
        root,
        active,
        x11rb::protocol::xproto::AtomEnum::WINDOW,
        &[editor],
    )
    .unwrap();
    conn.sync().unwrap();

    assert_eq!(keys.active_window(), Some(editor));
}

#[test]
fn activating_a_window_gives_it_the_keyboard_even_without_a_window_manager() {
    let server = XServer::start();
    let (conn, root) = server.connect();
    let keys = X11Keys::connect(Some(&server.display)).unwrap();
    let editor = window(&conn, root, EventMask::NO_EVENT);
    conn.map_window(editor).unwrap();
    conn.sync().unwrap();

    assert!(keys.activate_window(editor));

    assert_eq!(
        conn.get_input_focus().unwrap().reply().unwrap().focus,
        editor
    );
}

#[test]
fn a_window_that_is_gone_cannot_be_activated() {
    let server = XServer::start();
    let (conn, root) = server.connect();
    let keys = X11Keys::connect(Some(&server.display)).unwrap();
    let closed = window(&conn, root, EventMask::NO_EVENT);
    conn.destroy_window(closed).unwrap();
    conn.sync().unwrap();

    assert!(!keys.activate_window(closed));
}
