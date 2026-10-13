//! The page with a finger on it, frame by frame as the Android host drives it: egui-winit's
//! touches and the mouse events they stand in for go through [`WordApp::gate_touches`] first, then
//! a frame. Pins what a person sees: a scroll scrolls (and edits nothing it passes over), a tap
//! places the caret, a stop is not a tap, a hold selects the word and opens the menu.

use egui::{Event, Pos2, TouchPhase, pos2, vec2};
use serde_json::json;
use wordcraft_engine::Session;

use crate::{Services, WordApp};

struct Rig {
    ctx: egui::Context,
    a: WordApp,
    t: f64,
    screen: egui::Rect,
    commands: Vec<egui::ViewportCommand>,
}

impl Rig {
    fn new(w: f32, h: f32) -> Rig {
        let ctx = egui::Context::default();
        // What the Android app sets (`apps/wordcraft-android/src/app.rs`).
        ctx.options_mut(|o| {
            o.input_options.max_click_dist = 12.0;
            o.input_options.max_double_click_delay = 0.5;
            o.input_options.max_click_duration = 0.3;
        });
        Rig {
            ctx,
            a: WordApp::new(Session::new(wordcraft_doc::Document::new()), Services::default()),
            t: 0.0,
            screen: egui::Rect::from_min_size(Pos2::ZERO, vec2(w, h)),
            commands: Vec::new(),
        }
    }

    fn frame_dt(&mut self, events: Vec<Event>, dt: f64) {
        self.t += dt;
        let mut raw = egui::RawInput { events, time: Some(self.t), screen_rect: Some(self.screen), ..Default::default() };
        self.a.raw_input_hook(&mut raw);
        // As the Android shell does: the gate's wake is requested inside the pass, in `logic`.
        let wake = self.a.gate_touches(&self.ctx, &mut raw);
        let a = &mut self.a;
        let out = self.ctx.run_ui(raw, |ui| {
            if let Some(wake) = wake {
                ui.ctx().request_repaint_after(wake);
            }
            a.logic(ui.ctx());
            a.ui(ui);
        });
        for v in out.viewport_output.values() {
            self.commands.extend(v.commands.iter().cloned());
        }
        out.drop_without_applying_deltas();
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.frame_dt(events, 1.0 / 60.0);
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(Vec::new());
        }
    }

    fn long_doc(&mut self) {
        self.a.run("text.insert", json!({"text": "A line of text for the finger to scroll past. ".repeat(900)})).unwrap();
        self.a.run("caret.set", json!({"pos": wordcraft_doc::Pos::body(0, 100)})).unwrap();
        self.frames(4);
    }

    fn canvas(&self) -> egui::Rect {
        self.a.canvas.canvas_rect.unwrap()
    }

    fn offset(&self) -> f32 {
        self.a.canvas.scroll_offset.y
    }

    /// A finger goes down, moves by `step` per frame `n` times, and lifts.
    fn swipe(&mut self, from: Pos2, step: egui::Vec2, n: usize) -> Pos2 {
        self.frame(down(1, from));
        let mut at = from;
        for _ in 0..n {
            at += step;
            self.frame(moved(1, at));
        }
        self.frame(up(1, at));
        at
    }

    /// What the gate lets through of `events` now (a frame is not run).
    fn probe(&mut self, events: Vec<Event>) -> Vec<Event> {
        self.t += 1.0 / 60.0;
        let mut raw = egui::RawInput { events, time: Some(self.t), screen_rect: Some(self.screen), ..Default::default() };
        let _ = self.a.gate_touches(&self.ctx, &mut raw);
        raw.events
    }

    fn tap(&mut self, at: Pos2) {
        self.frame(down(1, at));
        self.frame(up(1, at));
        self.frames(2);
    }
}

fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
    Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(id), phase, pos, force: None }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
}

/// What egui-winit sends for the first finger going down, moving and lifting.
fn down(id: u64, pos: Pos2) -> Vec<Event> {
    vec![touch(id, TouchPhase::Start, pos), Event::PointerMoved(pos), button(pos, true)]
}

fn moved(id: u64, pos: Pos2) -> Vec<Event> {
    vec![touch(id, TouchPhase::Move, pos), Event::PointerMoved(pos)]
}

fn up(id: u64, pos: Pos2) -> Vec<Event> {
    vec![touch(id, TouchPhase::End, pos), button(pos, false), Event::PointerGone]
}

fn screen_of(r: &mut Rig, x: f32, y: f32) -> Pos2 {
    crate::canvas::page_to_screen(&mut r.a, 0, x, y).unwrap()
}

#[test]
fn a_tap_places_the_caret_and_a_swipe_scrolls_without_moving_it() {
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    let caret = r.a.session.sel.clone();
    let start = r.offset();
    r.swipe(r.canvas().center(), vec2(0.0, -40.0), 5);
    assert!(r.offset() > start + 150.0, "the page followed the finger: {start} to {}", r.offset());
    assert_eq!(r.a.session.sel, caret, "scrolling doesn't move the caret or select text");
    r.frames(120);
    // A tap places the caret where it landed, and the page stays.
    let rest = r.offset();
    r.tap(r.canvas().center());
    assert_eq!(r.offset(), rest, "a tap doesn't scroll");
    assert_ne!(r.a.session.sel, caret, "a tap puts the caret where it landed");
    assert!(r.a.session.sel.is_collapsed());
    assert!(r.commands.iter().any(|c| matches!(c, egui::ViewportCommand::IMEAllowed(true))), "a tap asks for the keyboard");
}

#[test]
fn a_tap_that_stops_a_coast_neither_moves_the_caret_nor_raises_the_keyboard() {
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    let caret = r.a.session.sel.clone();
    r.swipe(r.canvas().center(), vec2(0.0, -40.0), 6);
    r.frames(2);
    assert!(!r.a.canvas.pan.idle(), "the page is coasting");
    r.commands.clear();
    let stop = r.canvas().center() + vec2(30.0, -80.0);
    r.frame(down(2, stop));
    r.frame(up(2, stop));
    r.frames(3);
    assert!(r.a.canvas.pan.idle(), "the touch stopped the page");
    assert_eq!(r.a.session.sel, caret, "and did nothing else");
    assert!(!r.commands.iter().any(|c| matches!(c, egui::ViewportCommand::IMEAllowed(_))));
}

#[test]
fn a_swipe_that_starts_on_a_picture_scrolls_and_leaves_it_alone() {
    let mut r = Rig::new(1280.0, 900.0);
    r.a.run("text.insert", json!({"text": "Before the picture. ".repeat(40)})).unwrap();
    let img = image::RgbaImage::from_fn(200, 100, |_, _| image::Rgba([200, 30, 30, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    let data = wordcraft_engine::cmd::insert::base64_encode(&bytes);
    r.a.session.run("insert.picture", &json!({"data": data, "width": 300})).unwrap();
    r.a.run("select.collapse", json!({"end": true})).unwrap();
    r.a.run("text.insert", json!({"text": " After the picture. ".repeat(60)})).unwrap();
    r.a.run("caret.docStart", json!({})).unwrap();
    r.frames(8);
    let rect = r.a.session.layout().find_object(0, |_| true).map(|o| o.rect).expect("the picture is laid out");
    let center = screen_of(&mut r, rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let (depth, text) = (r.a.session.undo_depth(), r.a.session.doc.plain_text(wordcraft_doc::StoryRef::Body));
    let start = r.offset();
    r.swipe(center, vec2(0.0, -30.0), 8);
    r.frames(4);
    assert!(r.offset() > start + 100.0, "the page scrolled");
    assert_eq!(r.a.session.undo_depth(), depth, "nothing was edited: {:?}", r.a.session.undo_label());
    assert_eq!(r.a.session.doc.plain_text(wordcraft_doc::StoryRef::Body), text);
    let after = r.a.session.layout().find_object(0, |_| true).map(|o| o.rect).unwrap();
    assert_eq!((after.x, after.y), (rect.x, rect.y), "the picture stayed where it was");
}

fn table_rig() -> Rig {
    let mut r = Rig::new(1440.0, 900.0);
    r.a.run("insert.table", json!({"rows": 2, "cols": 3})).unwrap();
    r.a.run("text.insert", json!({"text": "Product"})).unwrap();
    r.frames(8);
    r
}

fn cell_rect(r: &mut Rig, row: usize, col: usize) -> wordcraft_geom::Rect {
    r.a.session.layout().pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            wordcraft_layout::Placed::Cell { rect, row: rr, cell: cc, .. } if *rr == row && *cc == col => Some(*rect),
            _ => None,
        })
        .expect("the cell is laid out")
}

#[test]
fn a_swipe_that_starts_on_a_table_border_or_handle_scrolls_and_changes_nothing() {
    for (name, dy) in [("down", 60.0_f32), ("up", -60.0), ("short", 15.0)] {
        let mut r = table_rig();
        let a = cell_rect(&mut r, 0, 0);
        let depth = r.a.session.undo_depth();
        let sel = r.a.session.sel.clone();
        // On the row's bottom border, on the column's right border, on the table's move handle.
        let from =
            [screen_of(&mut r, a.x + a.w / 2.0, a.y + a.h), screen_of(&mut r, a.right(), a.y + a.h / 2.0), screen_of(&mut r, a.x - 6.0, a.y - 6.0)];
        for at in from {
            r.swipe(at, vec2(0.0, dy / 8.0), 8);
            r.frames(5);
            assert_eq!(r.a.session.undo_depth(), depth, "{name}: nothing was edited ({:?})", r.a.session.undo_label());
            assert_eq!(r.a.session.sel, sel, "{name}: the selection is where it was");
            let now = cell_rect(&mut r, 0, 0);
            assert_eq!((now.w, now.h), (a.w, a.h), "{name}: the cell kept its size");
        }
    }
}

#[test]
fn two_fingers_landing_in_one_frame_keep_the_selection() {
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    r.a.run("select.text", json!({"text": "A line of text"})).unwrap();
    r.frames(2);
    let sel = r.a.session.sel.clone();
    let c = r.canvas().center();
    let mut both = down(1, c);
    both.push(touch(2, TouchPhase::Start, c + vec2(120.0, 0.0)));
    r.frame(both);
    for i in 1..=4 {
        let d = i as f32 * 10.0;
        r.frame(vec![
            touch(1, TouchPhase::Move, c - vec2(d, 0.0)),
            Event::PointerMoved(c - vec2(d, 0.0)),
            touch(2, TouchPhase::Move, c + vec2(120.0 + d, 0.0)),
        ]);
    }
    r.frame(vec![touch(2, TouchPhase::End, c + vec2(160.0, 0.0))]);
    r.frame(up(1, c - vec2(40.0, 0.0)));
    r.frames(3);
    assert_eq!(r.a.session.sel, sel, "a pinch leaves the selection alone");
}

#[test]
fn a_swipe_from_near_the_bottom_edge_follows_the_finger_every_frame() {
    for from_bottom in [5.0_f32, 25.0, 45.0, 70.0] {
        let mut r = Rig::new(1280.0, 600.0);
        r.long_doc();
        let c = r.canvas();
        let from = pos2(c.center().x, c.max.y - from_bottom);
        r.frame(down(1, from));
        let mut at = from;
        let mut moves = Vec::new();
        for _ in 0..8 {
            at.y -= 12.0;
            let before = r.offset();
            r.frame(moved(1, at));
            moves.push(r.offset() - before);
        }
        r.frame(up(1, at));
        // From the first frame past the slop on, the page keeps up with the finger and never goes backwards.
        assert!(moves.iter().skip(1).all(|m| *m >= 0.0), "{from_bottom}: {moves:?}");
        assert!(
            (r.offset() - (from.y - at.y)).abs() < 40.0 || r.offset() > 0.0,
            "{from_bottom}: the page moved {} for {}",
            r.offset(),
            from.y - at.y
        );
    }
}

#[test]
fn a_press_and_hold_selects_the_word_and_opens_the_context_menu() {
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    let c = r.canvas().center();
    r.frame(down(1, c));
    // Resting: after a moment the page gets the press, a little later the long touch opens the menu.
    for _ in 0..60 {
        r.frame(Vec::new());
    }
    assert!(r.a.canvas.context_menu_open, "the context menu is open");
    let word = r.a.session.selected_text();
    assert!(!word.trim().is_empty() && !word.trim().contains(' '), "the word under the finger is selected: {word:?}");
    r.frame(up(1, c));
}

#[test]
fn a_double_tap_selects_a_word_without_waiting_to_be_held() {
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    let c = r.canvas().center();
    r.tap(c);
    r.frame_dt(down(1, c), 0.12);
    r.frame_dt(up(1, c), 0.05);
    r.frames(3);
    let word = r.a.session.selected_text();
    assert!(!word.trim().is_empty() && !word.trim().contains(' '), "a word is selected: {word:?}");
}

#[test]
fn a_pen_gets_the_press_at_once() {
    for (mode, tool) in [("draw.pen", "pen"), ("draw.eraser", "eraser"), ("draw.lasso", "lasso")] {
        let mut r = Rig::new(1280.0, 600.0);
        r.long_doc();
        r.a.run(mode, json!({})).unwrap();
        r.frames(2);
        assert!(crate::ink::pen_active(&r.a), "{tool} is selected");
        let c = r.canvas().center();
        let events = r.probe(down(1, c));
        assert!(events.contains(&button(c, true)), "{tool}: the press goes through at once");
    }
    // With the pen the stroke starts as the finger lands: it is drawn, and nothing scrolls.
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    r.a.run("draw.pen", json!({})).unwrap();
    r.frames(2);
    let (c, start) = (r.canvas().center(), r.offset());
    r.frame(down(1, c));
    r.frame(moved(1, c + vec2(30.0, 0.0)));
    assert!(r.a.canvas.ink.is_some(), "a stroke is being drawn");
    r.frame(up(1, c + vec2(30.0, 0.0)));
    assert_eq!(r.offset(), start, "and the page stayed");
}

/// Two fingers: pinch zoom and two finger scroll are egui's, and need the pointer's position, which
/// the gate lets through.
#[test]
fn a_pinch_zooms_and_two_fingers_scroll_the_page() {
    let mut r = Rig::new(1280.0, 700.0);
    r.long_doc();
    let c = r.canvas().center();
    let (a, b) = (c + vec2(-50.0, 0.0), c + vec2(50.0, 0.0));
    let z0 = r.a.canvas.scale;
    // egui-winit: the first finger makes the mouse events, the second only a touch.
    let mut first = down(1, a);
    first.push(touch(2, TouchPhase::Start, b));
    r.frame(first);
    r.frames(2);
    for i in 1..=10 {
        let d = i as f32 * 12.0;
        r.frame(vec![
            touch(1, TouchPhase::Move, a - vec2(d, 0.0)),
            Event::PointerMoved(a - vec2(d, 0.0)),
            touch(2, TouchPhase::Move, b + vec2(d, 0.0)),
        ]);
    }
    r.frame(vec![touch(2, TouchPhase::End, b + vec2(120.0, 0.0))]);
    r.frame(up(1, a - vec2(120.0, 0.0)));
    r.frames(8);
    assert!(r.a.canvas.scale > z0 * 1.3, "spreading two fingers zooms in: {z0} to {}", r.a.canvas.scale);
    // Two fingers moving together scroll.
    let start = r.offset();
    let (a, b) = (c + vec2(-60.0, 0.0), c + vec2(60.0, 0.0));
    let mut first = down(1, a);
    first.push(touch(2, TouchPhase::Start, b));
    r.frame(first);
    for i in 1..8 {
        let d = vec2(0.0, -25.0 * i as f32);
        r.frame(vec![touch(1, TouchPhase::Move, a + d), Event::PointerMoved(a + d), touch(2, TouchPhase::Move, b + d)]);
    }
    r.frame(vec![touch(1, TouchPhase::End, a), touch(2, TouchPhase::End, b)]);
    r.frames(10);
    assert!(r.offset() > start + 100.0, "two fingers scroll the page: {start} to {}", r.offset());
}

fn picture_rig() -> (Rig, Pos2) {
    let mut r = Rig::new(1280.0, 900.0);
    r.a.run("text.insert", json!({"text": "Before the picture. ".repeat(40)})).unwrap();
    let img = image::RgbaImage::from_fn(200, 100, |_, _| image::Rgba([200, 30, 30, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    let data = wordcraft_engine::cmd::insert::base64_encode(&bytes);
    r.a.session.run("insert.picture", &json!({"data": data, "width": 300})).unwrap();
    r.a.run("select.collapse", json!({"end": true})).unwrap();
    r.a.run("text.insert", json!({"text": " After the picture. ".repeat(60)})).unwrap();
    r.a.run("caret.docStart", json!({})).unwrap();
    r.frames(8);
    let rect = r.a.session.layout().find_object(0, |_| true).map(|o| o.rect).expect("the picture is laid out");
    let center = screen_of(&mut r, rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    (r, center)
}

#[test]
fn a_tap_on_a_picture_selects_it_and_leaves_no_drag_behind() {
    let (mut r, center) = picture_rig();
    let depth = r.a.session.undo_depth();
    r.tap(center);
    assert!(r.a.canvas.obj_drag.is_none(), "the press and the release were two frames, the drag ended");
    let picture = r.a.session.sel.clone();
    assert!(!picture.is_collapsed(), "the picture is selected");
    // The next tap places the caret instead of being eaten (it was, once, by the drag left behind).
    let text = screen_of(&mut r, 90.0, 40.0);
    r.tap(text);
    assert!(r.a.session.sel.is_collapsed() && r.a.session.sel != picture, "a tap on text places the caret");
    // A swipe scrolls at once (it did nothing until another tap before).
    let start = r.offset();
    r.swipe(r.canvas().center() + vec2(100.0, 100.0), vec2(0.0, -30.0), 8);
    r.frames(4);
    assert!(r.offset() > start + 100.0, "the page scrolled: {start} to {}", r.offset());
    assert_eq!(r.a.session.undo_depth(), depth, "nothing was edited");
}

#[test]
fn a_tap_on_a_tables_selection_areas_selects_cells() {
    // Above a column selects it; left of a row selects it.
    let mut r = table_rig();
    let b = cell_rect(&mut r, 0, 1);
    let at = screen_of(&mut r, b.x + b.w / 2.0, b.y - 3.0);
    r.tap(at);
    assert!(r.a.session.cell_range().is_some(), "a tap above a column selects the column");
    let mut r = table_rig();
    let a = cell_rect(&mut r, 0, 0);
    let at = screen_of(&mut r, a.x - 10.0, a.y + a.h / 2.0);
    r.tap(at);
    assert!(r.a.session.cell_range().is_some(), "a tap left of a row selects the row");
}

#[test]
fn a_tap_leaves_no_drag_selection_behind() {
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    r.tap(r.canvas().center());
    assert!(!r.a.canvas.drag_selecting(), "the press and the release were one tap");
    r.a.run("select.text", json!({"text": "A line of text"})).unwrap();
    r.frames(3);
    assert!(crate::mini_toolbar::visible(&r.a), "the mini toolbar shows for the selection");
}

#[test]
fn the_file_tab_gets_its_touches_at_once() {
    let mut r = Rig::new(1280.0, 700.0);
    r.long_doc();
    let inside = r.canvas().center();
    // On the page the press is held back ...
    assert!(!r.probe(down(1, inside)).contains(&button(inside, true)));
    r.probe(up(1, inside));
    r.frames(2);
    // ... on File it is not: the page isn't under the finger.
    r.a.run("ui.backstage", json!({"value": true, "page": "options"})).unwrap();
    r.frames(3);
    let events = r.probe(down(2, inside));
    assert!(events.contains(&button(inside, true)), "the Backstage's own widgets get the press as the finger lands");
}

#[test]
fn a_finger_on_the_scroll_bar_strip_is_the_bars() {
    let mut r = Rig::new(1280.0, 700.0);
    r.long_doc();
    let c = r.canvas();
    let on_bar = pos2(c.max.x - 6.0, c.center().y);
    assert!(r.probe(down(1, on_bar)).contains(&button(on_bar, true)), "the press goes through to the bar at once");
}

#[test]
fn a_long_press_near_the_edge_opens_the_menu_without_scrolling() {
    for from_bottom in [30.0_f32, 40.0, 60.0] {
        let mut r = Rig::new(1280.0, 600.0);
        r.long_doc();
        let c = r.canvas();
        let at = pos2(c.center().x, c.max.y - from_bottom);
        r.frame(down(1, at));
        // The press (at the hold) may bring the caret into view once, as a click there does.
        for _ in 0..30 {
            r.frame(Vec::new());
        }
        let settled = r.offset();
        for _ in 0..40 {
            r.frame(Vec::new());
        }
        assert!(r.a.canvas.context_menu_open, "{from_bottom}: the menu is open");
        assert!(
            (r.offset() - settled).abs() < 1.0,
            "{from_bottom}: the page doesn't go on scrolling under a resting finger ({settled} to {})",
            r.offset()
        );
        assert!(!r.a.canvas.drag_selecting(), "{from_bottom}: no drag select grows from the rest");
    }
}

#[test]
fn a_resting_finger_asks_for_its_hold_frame() {
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    let c = r.canvas().center();
    let mut raw = egui::RawInput { events: down(1, c), time: Some(r.t + 0.016), screen_rect: Some(r.screen), ..Default::default() };
    let wake = r.a.gate_touches(&r.ctx, &mut raw).expect("a frame is wanted for the hold");
    assert!(wake > std::time::Duration::from_millis(100) && wake <= std::time::Duration::from_millis(200), "{wake:?}");
    // Once the hold time has passed the page has its press and nothing more is awaited.
    let mut raw = egui::RawInput { events: Vec::new(), time: Some(r.t + 0.016 + 0.25), screen_rect: Some(r.screen), ..Default::default() };
    assert_eq!(r.a.gate_touches(&r.ctx, &mut raw), None);
    assert!(raw.events.contains(&button(c, true)), "the press is delivered at the hold");
}

#[test]
fn a_pinch_keeps_the_page_under_the_fingers() {
    let mut r = Rig::new(1280.0, 700.0);
    r.long_doc();
    // The caret is under the fingers: a tap puts it there.
    r.tap(r.canvas().center() + vec2(0.0, 60.0));
    let (centre, _) = crate::canvas::caret_screen(&mut r.a).expect("the caret is on screen");
    let (a, b) = (centre + vec2(-50.0, 0.0), centre + vec2(50.0, 0.0));
    let mut first = down(1, a);
    first.push(touch(2, TouchPhase::Start, b));
    r.frame(first);
    r.frames(2);
    let z0 = r.a.canvas.scale;
    for i in 1..=10 {
        let d = i as f32 * 8.0;
        r.frame(vec![
            touch(1, TouchPhase::Move, a - vec2(d, 0.0)),
            Event::PointerMoved(a - vec2(d, 0.0)),
            touch(2, TouchPhase::Move, b + vec2(d, 0.0)),
        ]);
    }
    r.frame(vec![touch(2, TouchPhase::End, b + vec2(80.0, 0.0))]);
    r.frame(up(1, a - vec2(80.0, 0.0)));
    r.frames(8);
    assert!(r.a.canvas.scale > z0 * 1.4, "it zoomed in: {z0} to {}", r.a.canvas.scale);
    let (after, _) = crate::canvas::caret_screen(&mut r.a).expect("the caret is still on screen");
    assert!((after - centre).length() < 60.0, "the point under the fingers stayed under them: {centre:?} to {after:?}");
}

#[test]
fn the_bottom_strip_is_the_pages_unless_a_bar_is_there() {
    let mut r = Rig::new(1280.0, 700.0);
    r.long_doc();
    let c = r.canvas();
    // The page fits the width: no horizontal bar, so the bottom edge is no bar's.
    let low = pos2(c.center().x, c.max.y - 6.0);
    assert!(!r.probe(down(1, low)).contains(&button(low, true)), "held back like any touch on the page");
}

#[test]
fn a_coast_against_the_end_of_the_document_stops() {
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    // At the top, a hard flick downwards has nowhere to go.
    r.swipe(r.canvas().center(), vec2(0.0, 40.0), 6);
    r.frames(3);
    assert!(r.a.canvas.pan.idle(), "no coast against the start of the document");
    let caret = r.a.session.sel.clone();
    r.tap(r.canvas().center() + vec2(50.0, 0.0));
    assert_ne!(r.a.session.sel, caret, "so the next tap places the caret");
}

#[test]
fn a_finger_that_drifts_after_the_menu_opened_keeps_the_words_selection() {
    let mut r = Rig::new(1280.0, 600.0);
    r.long_doc();
    r.a.run("select.text", json!({"text": "A line of text"})).unwrap();
    r.frames(2);
    let c = r.canvas().center() + vec2(0.0, 40.0);
    r.frame(down(1, c));
    for _ in 0..70 {
        r.frame(Vec::new());
    }
    assert!(r.a.canvas.context_menu_open);
    let word = r.a.session.sel.clone();
    let start = r.offset();
    for i in 1..=4 {
        r.frame(moved(1, c + vec2(0.0, -6.0 * i as f32)));
    }
    r.frames(3);
    assert_eq!(r.a.session.sel, word, "the word the menu is for is still selected");
    assert!((r.offset() - start).abs() < 1.0, "and the page stayed under the open menu");
}

#[test]
fn a_cancelled_hold_on_a_picture_leaves_no_drag_behind() {
    let (mut r, center) = picture_rig();
    r.frame(down(1, center));
    for _ in 0..20 {
        r.frame(Vec::new());
    }
    assert!(r.a.canvas.obj_drag.is_some(), "the held picture is being dragged");
    // Android cancels the touch: egui-winit sends the cancel and the pointer's departure only.
    r.frame(vec![touch(1, TouchPhase::Cancel, center), Event::PointerGone]);
    r.frames(3);
    assert!(r.a.canvas.obj_drag.is_none(), "the drag ended with the cancelled touch");
    let start = r.offset();
    r.swipe(r.canvas().center() + vec2(100.0, 100.0), vec2(0.0, -30.0), 8);
    r.frames(4);
    assert!(r.offset() > start + 100.0, "and a swipe scrolls: {start} to {}", r.offset());
}
