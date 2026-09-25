//! UI interaction tests: real pointer and key events against the real
//! window (Slint's headless testing backend), for gestures that unit tests
//! of the view models cannot see — elements rebuilt under the pointer, a
//! scroll view stealing drags, a popup keeping the keyboard.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use i_slint_backend_testing::ElementHandle;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, ModelRc, SharedString, VecModel};

mod ui {
    #![allow(
        // Generated with element debug info (debug builds) it contains
        // `todo!()` in paths the app never takes.
        clippy::todo,
        unreachable_pub,
        clippy::all,
        clippy::pedantic,
        clippy::nursery,
        unused,
        missing_debug_implementations,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )]
    slint::include_modules!();
}
use ui::{EditorState, MainWindow, PreviewText, TextBlockView, TimelineClip};

fn window() -> MainWindow {
    i_slint_backend_testing::init_no_event_loop();
    let w = MainWindow::new().unwrap();
    // The tests find elements by their English labels, whatever the
    // machine's language (needs a created component).
    slint::select_bundled_translation("en").unwrap();
    w.window().set_size(slint::LogicalSize::new(1400.0, 860.0));
    w.show().unwrap();
    w
}

fn only(mut it: impl Iterator<Item = ElementHandle>, what: &str) -> ElementHandle {
    it.next().unwrap_or_else(|| panic!("{what} not found"))
}

fn centre(e: &ElementHandle) -> LogicalPosition {
    let p = e.absolute_position();
    let s = e.size();
    LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0)
}

fn type_text(w: &MainWindow, text: &str) {
    for c in text.chars() {
        let s = SharedString::from(c.to_string());
        w.window()
            .dispatch_event(WindowEvent::KeyPressed { text: s.clone() });
        w.window()
            .dispatch_event(WindowEvent::KeyReleased { text: s });
    }
}

fn press_key(w: &MainWindow, key: Key) {
    let s: SharedString = key.into();
    w.window()
        .dispatch_event(WindowEvent::KeyPressed { text: s.clone() });
    w.window()
        .dispatch_event(WindowEvent::KeyReleased { text: s });
}

/// Twenty wide clips: far more than the strip shows, so it could scroll.
fn many_clips(s: &EditorState) {
    let clips: Vec<TimelineClip> = (0..20)
        .map(|i| TimelineClip {
            index: i,
            id: format!("c{i}").into(),
            title: format!("Clip {i}").into(),
            x: 8.0 + i as f32 * 200.0,
            width: 196.0,
            duration_text: "5.0 s".into(),
            ..Default::default()
        })
        .collect();
    s.set_clips(ModelRc::new(VecModel::from(clips)));
    s.set_clip_count(20);
    s.set_strip_width(4_010.0);
}

#[test]
fn dragging_a_clip_in_a_long_timeline_moves_the_clip_not_the_view() {
    let w = window();
    let s = w.global::<EditorState>();
    many_clips(&s);
    let dragged = Rc::new(RefCell::new(Vec::new()));
    let d = Rc::clone(&dragged);
    s.on_clip_dragged(move |x| d.borrow_mut().push(x));
    let clip = only(
        ElementHandle::find_by_accessible_label(&w, "Clip 1, 5.0 s"),
        "clip",
    );
    let before = clip.absolute_position();
    let from = centre(&clip);
    clip.mock_drag(
        LogicalPosition::new(from.x + 150.0, from.y),
        PointerEventButton::Left,
    );
    let xs = dragged.borrow();
    assert!(xs.len() >= 2, "the clip got the drag: {xs:?}");
    assert!(xs.last().unwrap() - xs.first().unwrap() > 100.0, "{xs:?}");
    assert_eq!(clip.absolute_position(), before, "the strip did not scroll");
}

#[test]
fn dragging_a_text_block_or_its_edge_in_a_long_timeline_reaches_the_text() {
    let w = window();
    let s = w.global::<EditorState>();
    many_clips(&s);
    s.set_text_blocks(ModelRc::new(VecModel::from(vec![TextBlockView {
        index: 0,
        x: 300.0,
        width: 300.0,
        row: 0,
        title: "Summer".into(),
        selected: true,
        focused: false,
    }])));
    let events = Rc::new(RefCell::new(Vec::new()));
    let e = Rc::clone(&events);
    s.on_text_lane_pressed(move |_, _, _, code, _| e.borrow_mut().push(format!("press {code}")));
    let e = Rc::clone(&events);
    s.on_text_lane_dragged(move |_| e.borrow_mut().push("move".to_owned()));
    let block = only(
        ElementHandle::find_by_accessible_label(&w, "Summer"),
        "text block",
    );
    let before = block.absolute_position();
    let from = centre(&block);
    block.mock_drag(
        LogicalPosition::new(from.x + 120.0, from.y),
        PointerEventButton::Left,
    );
    assert_eq!(
        block.absolute_position(),
        before,
        "the strip did not scroll"
    );
    // The right edge (8 px grab area) trims.
    let p = block.absolute_position();
    let edge = LogicalPosition::new(p.x + block.size().width - 3.0, from.y);
    w.window()
        .dispatch_event(WindowEvent::PointerMoved { position: edge });
    w.window().dispatch_event(WindowEvent::PointerPressed {
        position: edge,
        button: PointerEventButton::Left,
    });
    for dx in [20.0, 40.0, 60.0] {
        w.window().dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(edge.x + dx, edge.y),
        });
    }
    w.window().dispatch_event(WindowEvent::PointerReleased {
        position: LogicalPosition::new(edge.x + 60.0, edge.y),
        button: PointerEventButton::Left,
    });
    let ev = events.borrow();
    let presses: Vec<&String> = ev.iter().filter(|e| e.starts_with("press")).collect();
    assert_eq!(presses, ["press 0", "press 2"], "{ev:?}");
    assert!(ev.iter().filter(|e| *e == "move").count() >= 4, "{ev:?}");
}

#[test]
fn the_wheel_scrolls_a_long_timeline() {
    let w = window();
    let s = w.global::<EditorState>();
    many_clips(&s);
    let clip = only(
        ElementHandle::find_by_accessible_label(&w, "Clip 2, 5.0 s"),
        "clip",
    );
    let before = clip.absolute_position().x;
    clip.scroll(0.0, -120.0);
    let after = clip.absolute_position().x;
    assert!((before - after - 120.0).abs() < 1.0, "{before} → {after}");
}

#[test]
fn a_corner_handle_resizes_even_while_the_box_changes() {
    let w = window();
    let s = w.global::<EditorState>();
    s.set_frame_aspect(16.0 / 9.0);
    s.set_text_selected_count(1);
    let model = Rc::new(VecModel::from(vec![PreviewText {
        index: 0,
        x: 0.3,
        y: 0.4,
        width: 0.4,
        height: 0.1,
        selected: true,
    }]));
    s.set_preview_texts(ModelRc::from(Rc::clone(&model)));
    let events = Rc::new(RefCell::new(Vec::new()));
    let e = Rc::clone(&events);
    s.on_preview_text_pressed(move |i, code, _, _| {
        e.borrow_mut().push(format!("press {i} {code}"))
    });
    // Like the app: every move changes the box, which rebuilt the handles.
    let (e, m) = (Rc::clone(&events), Rc::clone(&model));
    s.on_preview_text_dragged(move |nx, _| {
        e.borrow_mut().push("move".into());
        let mut row = m.row_data(0).unwrap();
        row.width = (nx - row.x).max(0.05);
        m.set_row_data(0, row);
    });
    let handles: Vec<ElementHandle> =
        ElementHandle::find_by_accessible_label(&w, "Resize text").collect();
    assert_eq!(handles.len(), 4, "four corner handles");
    let bottom_right = handles
        .iter()
        .max_by(|a, b| {
            let (pa, pb) = (centre(a), centre(b));
            (pa.x + pa.y).total_cmp(&(pb.x + pb.y))
        })
        .unwrap();
    let from = centre(bottom_right);
    bottom_right.mock_drag(
        LogicalPosition::new(from.x + 80.0, from.y + 40.0),
        PointerEventButton::Left,
    );
    let ev = events.borrow();
    assert_eq!(ev.first().map(String::as_str), Some("press 0 3"), "{ev:?}");
    let moves = ev.iter().filter(|e| *e == "move").count();
    assert!(
        moves >= 3,
        "the handle kept the drag while the box changed: {ev:?}"
    );
}

#[test]
fn the_font_picker_filters_picks_closes_and_keeps_the_app_usable() {
    let w = window();
    let s = w.global::<EditorState>();
    s.set_text_selected_count(1);
    s.set_text_font("Inter Variable".into());
    let all: Vec<SharedString> = ["Arial", "Caveat", "Inter Variable", "Montserrat"]
        .iter()
        .map(|f| (*f).into())
        .collect();
    let searches = Rc::new(RefCell::new(Vec::<String>::new()));
    let (sr, all_c, ww) = (Rc::clone(&searches), all.clone(), w.as_weak());
    s.on_text_font_search(move |q| {
        let st = ww.upgrade().unwrap();
        let st = st.global::<EditorState>();
        sr.borrow_mut().push(q.to_string());
        let q = q.to_lowercase();
        let rows: Vec<SharedString> = all_c
            .iter()
            .filter(|f| f.to_lowercase().contains(&q))
            .cloned()
            .collect();
        st.set_font_matches(ModelRc::new(VecModel::from(rows)));
    });
    let picked = Rc::new(RefCell::new(Vec::<String>::new()));
    let (p, ww) = (Rc::clone(&picked), w.as_weak());
    s.on_text_font_changed(move |name| {
        p.borrow_mut().push(name.to_string());
        ww.upgrade()
            .unwrap()
            .global::<EditorState>()
            .set_text_font(name);
    });
    let picker = only(
        ElementHandle::find_by_accessible_label(&w, "Font").filter(|e| {
            e.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::Combobox)
        }),
        "font picker",
    );
    assert_eq!(picker.accessible_expanded(), Some(false));
    picker.mock_single_click(PointerEventButton::Left);
    assert_eq!(
        picker.accessible_expanded(),
        Some(true),
        "a click opens the list"
    );
    type_text(&w, "mon");
    assert_eq!(
        searches.borrow().last().map(String::as_str),
        Some("mon"),
        "typing reaches the field"
    );
    press_key(&w, Key::Return);
    assert_eq!(picked.borrow().as_slice(), ["Montserrat"]);
    assert_eq!(
        picker.accessible_expanded(),
        Some(false),
        "picking closes the list"
    );
    // Nothing reopens by itself, and the field still takes typing.
    press_key(&w, Key::Escape);
    assert_eq!(picker.accessible_expanded(), Some(false));
    // A click opens the list again with the text selected; typing replaces it.
    picker.mock_single_click(PointerEventButton::Left);
    assert_eq!(picker.accessible_expanded(), Some(true));
    type_text(&w, "cav");
    assert_eq!(searches.borrow().last().map(String::as_str), Some("cav"));
    assert_eq!(picker.accessible_expanded(), Some(true));
    // Conditional and repeated elements exist once laid out.
    let _ = picker.size();
    for f in ElementHandle::find_by_element_type_name(&w, "Flickable") {
        let _ = f.size();
    }
    let row = only(
        ElementHandle::find_by_accessible_label(&w, "Caveat"),
        "Caveat row",
    );
    row.mock_single_click(PointerEventButton::Left);
    assert_eq!(picked.borrow().last().map(String::as_str), Some("Caveat"));
    assert_eq!(
        picker.accessible_expanded(),
        Some(false),
        "a click on a row picks and closes"
    );
    // The rest of the app still responds: Escape clears, a clip click arrives.
    many_clips(&s);
    let clicked = Rc::new(RefCell::new(0));
    let c = Rc::clone(&clicked);
    s.on_clip_pressed(move |_, _, _| *c.borrow_mut() += 1);
    only(
        ElementHandle::find_by_accessible_label(&w, "Clip 0, 5.0 s"),
        "clip",
    )
    .mock_single_click(PointerEventButton::Left);
    assert_eq!(*clicked.borrow(), 1, "clicks reach the timeline again");
}

#[test]
fn the_export_dialog_discloses_advanced_and_gates_hdr() {
    let w = window();
    let s = w.global::<EditorState>();
    let refreshes = Rc::new(RefCell::new(0));
    let r = refreshes.clone();
    s.on_export_refresh(move || *r.borrow_mut() += 1);
    s.set_export_hdr_availability(1);
    s.set_export_open(true);

    // Collapsed: no codec picker yet.
    assert!(
        ElementHandle::find_by_accessible_label(&w, "Codec")
            .next()
            .is_none()
    );
    assert!(*refreshes.borrow() >= 1, "opening refreshes the summary");
    let advanced = only(
        ElementHandle::find_by_accessible_label(&w, "Advanced"),
        "Advanced disclosure",
    );
    advanced.mock_single_click(PointerEventButton::Left);
    assert!(s.get_export_advanced_open());
    only(
        ElementHandle::find_by_accessible_label(&w, "Codec"),
        "codec picker",
    );

    // No HDR sources: clicking the switch does nothing.
    let hdr = only(
        ElementHandle::find_by_accessible_label(&w, "HDR (for iPhone and YouTube HDR)"),
        "HDR switch",
    );
    hdr.mock_single_click(PointerEventButton::Left);
    assert!(!s.get_export_hdr());
    s.set_export_hdr_availability(0);
    hdr.mock_single_click(PointerEventButton::Left);
    assert!(s.get_export_hdr());

    // A running export: Esc closes the dialog but does not cancel.
    let cancelled = Rc::new(RefCell::new(false));
    let c = cancelled.clone();
    s.on_export_cancel(move || *c.borrow_mut() = true);
    s.set_export_status(1);
    press_key(&w, Key::Escape);
    assert!(!s.get_export_open());
    assert!(!*cancelled.borrow());

    // Right after success, Enter closes instead of exporting again.
    let started = Rc::new(RefCell::new(false));
    let st = started.clone();
    s.on_export_start(move || *st.borrow_mut() = true);
    s.set_export_status(2);
    s.set_export_open(true);
    only(
        ElementHandle::find_by_accessible_label(&w, "Advanced"),
        "dialog open",
    );
    press_key(&w, Key::Return);
    assert!(!s.get_export_open(), "Enter closes");
    assert!(!*started.borrow(), "no second export");
}
