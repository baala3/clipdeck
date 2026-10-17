use clip_engine::menu::{place_cascade, place_popup, PopupPlacement, ScreenRect};

/// A 1920 x 1040 work area: a 1080p screen above a 40px taskbar.
const AREA: ScreenRect = ScreenRect {
    x: 0.0,
    y: 0.0,
    width: 1920.0,
    height: 1040.0,
};
/// How wide a cascade (a folder, More, a preview) may be.
const PANEL: f64 = 320.0;

#[test]
fn the_menu_opens_with_its_corner_at_the_cursor() {
    assert_eq!(
        place_popup((400.0, 300.0), (300.0, 500.0), PANEL, AREA),
        PopupPlacement {
            x: 400.0,
            y: 300.0,
            panel_on_left: false
        }
    );
}

#[test]
fn near_the_right_edge_it_opens_to_the_left_of_the_cursor() {
    let placed = place_popup((1800.0, 300.0), (300.0, 500.0), PANEL, AREA);

    assert_eq!(placed.x, 1500.0);
}

#[test]
fn near_the_bottom_it_moves_up_to_stay_on_screen() {
    let placed = place_popup((400.0, 900.0), (300.0, 500.0), PANEL, AREA);

    assert_eq!(placed.y, 540.0);
}

#[test]
fn a_menu_taller_than_the_screen_starts_at_its_top() {
    let placed = place_popup((400.0, 900.0), (300.0, 1500.0), PANEL, AREA);

    assert_eq!(placed.y, 0.0);
}

#[test]
fn cascades_open_on_the_left_when_the_right_has_no_room_for_them() {
    assert!(!place_popup((400.0, 300.0), (300.0, 500.0), PANEL, AREA).panel_on_left);
    assert!(place_popup((1400.0, 300.0), (300.0, 500.0), PANEL, AREA).panel_on_left);
}

#[test]
fn the_panel_stays_on_the_right_when_neither_side_has_room() {
    let narrow = ScreenRect {
        width: 500.0,
        ..AREA
    };

    assert!(!place_popup((10.0, 10.0), (300.0, 500.0), PANEL, narrow).panel_on_left);
}

#[test]
fn work_areas_need_not_start_at_zero() {
    // A second screen to the left of the main one, with its taskbar on top.
    let left_screen = ScreenRect {
        x: -1280.0,
        y: 40.0,
        width: 1280.0,
        height: 984.0,
    };

    let placed = place_popup((-100.0, 10.0), (300.0, 500.0), PANEL, left_screen);

    assert_eq!(
        placed,
        PopupPlacement {
            x: -400.0,
            y: 40.0,
            panel_on_left: true
        }
    );
}

const MENU: ScreenRect = ScreenRect {
    x: 400.0,
    y: 300.0,
    width: 300.0,
    height: 500.0,
};

#[test]
fn a_cascade_opens_beside_the_menu_level_with_its_row() {
    assert_eq!(
        place_cascade(MENU, 360.0, (320.0, 200.0), false, AREA),
        (700.0, 360.0)
    );
}

#[test]
fn on_the_left_it_ends_where_the_menu_starts() {
    assert_eq!(
        place_cascade(MENU, 360.0, (320.0, 200.0), true, AREA),
        (80.0, 360.0)
    );
}

#[test]
fn a_cascade_near_the_bottom_moves_up_to_stay_on_screen() {
    assert_eq!(
        place_cascade(MENU, 950.0, (320.0, 200.0), false, AREA),
        (700.0, 840.0)
    );
}

#[test]
fn a_cascade_taller_than_the_screen_starts_at_its_top() {
    assert_eq!(
        place_cascade(MENU, 950.0, (320.0, 1500.0), false, AREA),
        (700.0, 0.0)
    );
}
