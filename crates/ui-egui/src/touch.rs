//! Finger gestures on the page, for touch screens (a phone or tablet running the Android build).
//!
//! egui turns a touch into a mouse press, so a finger that drags across the page would select
//! text, and nothing would scroll. People expect the opposite of a finger: it scrolls, and a
//! selection is made with a double tap (then a drag, which selects word by word), a long press (the
//! context menu) or the keyboard. So a single finger that travels more than a few points scrolls the
//! page and, once let go, keeps coasting; the caret and selection the press changed on the way down
//! are put back, so scrolling never moves the caret. A finger that goes down as the second tap of a
//! double tap is left to the mouse handling, which selects by words as it drags. Two fingers are egui's own
//! pinch zoom, and cancel a scroll in progress. A touch that belongs to a tool (a dragged picture
//! or table border, the pen) is never taken over.
//!
//! The state machine is plain data in, scroll distance out, so its tests need no window.

use egui::{Event, Pos2, Rect, TouchId, TouchPhase, Vec2};
use wordcraft_engine::Selection;

/// How far (points) a finger travels before a tap becomes a scroll.
const SLOP: f32 = 8.0;
/// A release slower than this (points per second) doesn't coast.
const MIN_FLING: f32 = 80.0;
/// Coasting slows by this much per second (the speed is multiplied by `exp(-FRICTION * dt)`).
const FRICTION: f32 = 3.5;
/// The fastest coast (points per second), so a stray sample can't throw the page far.
const MAX_FLING: f32 = 6000.0;

/// One finger that is down.
struct Finger {
    id: TouchId,
    start: Pos2,
    last: Pos2,
    /// Moved far enough to be a scroll.
    panning: bool,
    /// Went down as the second tap of a double tap: the mouse handling selects by words.
    selecting: bool,
    /// The selection when the finger went down.
    before: Selection,
}

/// What a frame's touches did.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Step {
    /// The distance to scroll the page this frame, in the sign the canvas's scroll uses (a finger
    /// moving up is a negative y, the same as a wheel turning down).
    pub delta: Vec2,
    /// A finger became a scroll this frame; the selection it changed on the way down is returned.
    pub began: Option<Selection>,
    /// The page is still moving without a finger (coasting): ask for another frame.
    pub coasting: bool,
}

/// The finger scroll in progress.
#[derive(Default)]
pub(crate) struct Pan {
    finger: Option<Finger>,
    /// Points per second the page coasts at after a release.
    fling: Vec2,
    /// The finger lifted this frame after scrolling.
    just_ended: bool,
}

impl Pan {
    /// A finger is scrolling the page (or let go of it this frame): the mouse handling stands down.
    pub fn panning(&self) -> bool {
        self.just_ended || self.finger.as_ref().is_some_and(|f| f.panning)
    }

    /// No finger is down and the page isn't coasting: nothing to do until the next touch.
    pub fn idle(&self) -> bool {
        self.finger.is_none() && self.fling == Vec2::ZERO
    }

    /// Stop everything, e.g. when a tool takes the pointer.
    pub fn cancel(&mut self) {
        self.finger = None;
        self.fling = Vec2::ZERO;
        self.just_ended = false;
    }

    /// Take this frame's events. `area` is where the page is drawn (a touch elsewhere is not ours),
    /// `second_tap` says whether a touch at a point is the second tap of a double tap, `tool_busy`
    /// that a tool owns the pointer, `selection` the selection now (before this frame's presses
    /// are handled), and `dt` the frame time in seconds.
    pub fn frame(
        &mut self,
        events: &[Event],
        area: Rect,
        second_tap: impl Fn(Pos2) -> bool,
        tool_busy: bool,
        selection: &Selection,
        dt: f32,
    ) -> Step {
        let mut step = Step::default();
        self.just_ended = false;
        let mut moved = Vec2::ZERO;
        for event in events {
            let Event::Touch { id, phase, pos, .. } = event else { continue };
            match phase {
                TouchPhase::Start => {
                    self.fling = Vec2::ZERO;
                    if self.finger.is_some() {
                        // A second finger: pinch and two finger pan are egui's.
                        self.finger = None;
                    } else if area.contains(*pos) {
                        self.finger =
                            Some(Finger { id: *id, start: *pos, last: *pos, panning: false, selecting: second_tap(*pos), before: selection.clone() });
                    }
                }
                TouchPhase::Move => {
                    let Some(f) = self.finger.as_mut().filter(|f| f.id == *id) else { continue };
                    if !f.panning && !f.selecting && !tool_busy && f.start.distance(*pos) > SLOP {
                        f.panning = true;
                        step.began = Some(f.before.clone());
                        // The distance already travelled counts, so the page doesn't lag the finger.
                        f.last = f.start;
                    }
                    if f.panning {
                        moved += *pos - f.last;
                    }
                    f.last = *pos;
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    if self.finger.as_ref().is_some_and(|f| f.id == *id) {
                        if self.finger.as_ref().is_some_and(|f| f.panning) {
                            self.just_ended = true;
                        }
                        self.finger = None;
                    }
                }
            }
        }
        step.delta = moved;
        if dt > 0.0 && dt.is_finite() {
            if moved != Vec2::ZERO || self.finger.as_ref().is_some_and(|f| f.panning) {
                // Follow the finger's speed: an average of the last frames, so one jittery sample
                // doesn't decide how far the page coasts.
                let now = (moved / dt).clamp(Vec2::splat(-MAX_FLING), Vec2::splat(MAX_FLING));
                self.fling = self.fling * 0.4 + now * 0.6;
            } else if self.finger.is_none() && self.fling != Vec2::ZERO {
                if self.fling.length() < MIN_FLING {
                    self.fling = Vec2::ZERO;
                } else {
                    step.delta += self.fling * dt;
                    self.fling *= (-FRICTION * dt).exp();
                    step.coasting = true;
                }
            }
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, TouchDeviceId, pos2, vec2};

    fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
        Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos, force: None }
    }

    fn area() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))
    }

    fn sel() -> Selection {
        Selection::caret(wordcraft_doc::Pos::body(0, 3))
    }

    fn run(pan: &mut Pan, events: &[Event]) -> Step {
        pan.frame(events, area(), |_| false, false, &sel(), 1.0 / 60.0)
    }

    #[test]
    fn a_finger_that_travels_scrolls_and_gives_back_the_selection() {
        let mut pan = Pan::default();
        let s = run(&mut pan, &[touch(1, TouchPhase::Start, pos2(300.0, 400.0))]);
        assert_eq!(s, Step::default());
        assert!(!pan.panning());
        // Inside the slop nothing happens: it is still a tap.
        let s = run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, 395.0))]);
        assert_eq!((s.delta, s.began.is_none(), pan.panning()), (Vec2::ZERO, true, false));
        // Past it, the whole distance so far is scrolled at once, and the old selection returned.
        let s = run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, 380.0))]);
        assert_eq!(s.delta, vec2(0.0, -20.0));
        assert_eq!(s.began, Some(sel()));
        assert!(pan.panning());
        let s = run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, 350.0))]);
        assert_eq!((s.delta, s.began), (vec2(0.0, -30.0), None));
        // Letting go ends the scroll, but the mouse handling still stands down for that frame.
        run(&mut pan, &[touch(1, TouchPhase::End, pos2(300.0, 350.0))]);
        assert!(pan.panning());
        run(&mut pan, &[]);
        assert!(!pan.panning());
    }

    #[test]
    fn a_fast_flick_coasts_and_slows_to_a_stop() {
        let mut pan = Pan::default();
        run(&mut pan, &[touch(1, TouchPhase::Start, pos2(300.0, 500.0))]);
        let mut y = 500.0;
        for _ in 0..6 {
            y -= 30.0;
            run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, y))]);
        }
        run(&mut pan, &[touch(1, TouchPhase::End, pos2(300.0, y))]);
        let mut total = 0.0;
        let mut frames = 0;
        loop {
            let s = run(&mut pan, &[]);
            if !s.coasting {
                break;
            }
            assert!(s.delta.y < 0.0, "keeps going the way the finger went");
            total += s.delta.y;
            frames += 1;
            assert!(frames < 600, "coasting must end");
        }
        assert!(total < -100.0, "a flick carries on for a while: {total}");
        assert_eq!(run(&mut pan, &[]).delta, Vec2::ZERO);
    }

    #[test]
    fn a_slow_release_does_not_coast_and_a_new_touch_stops_the_coast() {
        let mut pan = Pan::default();
        run(&mut pan, &[touch(1, TouchPhase::Start, pos2(300.0, 500.0))]);
        run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, 480.0))]);
        // The finger rests before lifting.
        for _ in 0..10 {
            run(&mut pan, &[]);
        }
        run(&mut pan, &[touch(1, TouchPhase::End, pos2(300.0, 480.0))]);
        assert!(!run(&mut pan, &[]).coasting);
        // A flick, then a new finger goes down while the page is still moving.
        let mut pan = Pan::default();
        run(&mut pan, &[touch(1, TouchPhase::Start, pos2(300.0, 500.0))]);
        for i in 1..=5 {
            run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, 500.0 - 40.0 * i as f32))]);
        }
        run(&mut pan, &[touch(1, TouchPhase::End, pos2(300.0, 300.0))]);
        assert!(run(&mut pan, &[]).coasting);
        let s = run(&mut pan, &[touch(2, TouchPhase::Start, pos2(100.0, 100.0))]);
        assert_eq!((s.delta, s.coasting), (Vec2::ZERO, false));
    }

    #[test]
    fn the_second_tap_of_a_double_tap_is_left_to_the_mouse_handling() {
        let mut pan = Pan::default();
        let s = pan.frame(&[touch(1, TouchPhase::Start, pos2(300.0, 400.0))], area(), |_| true, false, &sel(), 1.0 / 60.0);
        assert_eq!(s, Step::default());
        let s = pan.frame(&[touch(1, TouchPhase::Move, pos2(300.0, 300.0))], area(), |_| true, false, &sel(), 1.0 / 60.0);
        assert_eq!((s.delta, s.began, pan.panning()), (Vec2::ZERO, None, false));
    }

    #[test]
    fn a_tool_that_owns_the_pointer_keeps_the_finger() {
        let mut pan = Pan::default();
        pan.frame(&[touch(1, TouchPhase::Start, pos2(300.0, 400.0))], area(), |_| false, true, &sel(), 1.0 / 60.0);
        let s = pan.frame(&[touch(1, TouchPhase::Move, pos2(300.0, 300.0))], area(), |_| false, true, &sel(), 1.0 / 60.0);
        assert_eq!((s.delta, pan.panning()), (Vec2::ZERO, false));
    }

    #[test]
    fn two_fingers_cancel_a_scroll_and_touches_outside_the_page_are_not_ours() {
        let mut pan = Pan::default();
        run(&mut pan, &[touch(1, TouchPhase::Start, pos2(300.0, 400.0))]);
        run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, 350.0))]);
        assert!(pan.panning());
        run(&mut pan, &[touch(2, TouchPhase::Start, pos2(500.0, 400.0))]);
        assert!(!pan.panning());
        let s = run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, 200.0)), touch(2, TouchPhase::Move, pos2(500.0, 200.0))]);
        assert_eq!(s.delta, Vec2::ZERO, "pinch and two finger pan belong to egui");
        let mut pan = Pan::default();
        run(&mut pan, &[touch(1, TouchPhase::Start, pos2(900.0, 400.0))]);
        let s = run(&mut pan, &[touch(1, TouchPhase::Move, pos2(900.0, 300.0))]);
        assert_eq!((s.delta, pan.panning()), (Vec2::ZERO, false), "the ribbon and panes keep their own touches");
        // Mouse events and junk (a zero frame time, a NaN position) are harmless.
        let mut pan = Pan::default();
        let junk = [
            Event::PointerMoved(pos2(1.0, 1.0)),
            Event::PointerButton { pos: pos2(1.0, 1.0), button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        ];
        assert_eq!(pan.frame(&junk, area(), |_| false, false, &sel(), 0.0), Step::default());
        pan.frame(
            &[touch(1, TouchPhase::Start, pos2(1.0, 1.0)), touch(1, TouchPhase::Move, pos2(f32::NAN, f32::NAN))],
            area(),
            |_| false,
            false,
            &sel(),
            f32::NAN,
        );
    }
}
