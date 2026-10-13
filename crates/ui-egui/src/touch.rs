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
//! The press a finger makes is held back until it is known what the finger is doing ([`Gate`]):
//! if it travels it is a scroll and the page never saw a press (so a swipe that starts on a
//! picture, a table border or a coasting page changes nothing); if it lifts without travelling it
//! is a tap, and the page gets a press and a release together; if it rests for a moment it is a
//! press and hold, and the page gets the press then (so a long press opens the context menu and a
//! held picture can be dragged).
//!
//! The state machines are plain data in, events or scroll distance out, so their tests need no
//! window.

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
/// The longest frame (seconds) a finger or a coast counts for.
const MAX_DT: f32 = 0.05;

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
    /// A finger became a scroll (or a second finger landed) this frame; the selection the press
    /// changed on the way down is returned.
    pub began: Option<Selection>,
    /// The page is still moving without a finger (coasting): ask for another frame.
    pub coasting: bool,
}

/// The finger scroll in progress.
#[derive(Default)]
pub(crate) struct Pan {
    finger: Option<Finger>,
    /// Every finger that is down (a second or third one is never tracked as a scroll).
    down: Vec<TouchId>,
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

    /// The page has run into the start or the end of the document: the coast is over.
    pub fn hit_edge(&mut self) {
        if self.finger.is_none() {
            self.fling = Vec2::ZERO;
        }
    }

    /// The page is coasting on its own (no finger is down).
    pub fn coasting(&self) -> bool {
        self.finger.is_none() && self.fling != Vec2::ZERO
    }

    /// No finger is down and the page isn't coasting: nothing to do until the next touch.
    pub fn idle(&self) -> bool {
        self.finger.is_none() && self.fling == Vec2::ZERO
    }

    /// No finger is down any more (egui says so): forget the ones tracked, whose lift was lost
    /// (the app was paused with a finger down). Without this a lost lift would make every later
    /// finger look like a second one.
    pub fn forget_fingers(&mut self) {
        self.finger = None;
        self.down.clear();
    }

    /// Stop everything, e.g. when a tool takes the pointer.
    pub fn cancel(&mut self) {
        self.finger = None;
        self.down.clear();
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
                // A position that isn't a number can't be followed (and would poison the sums below).
                TouchPhase::Start | TouchPhase::Move if !pos.is_finite() => continue,
                TouchPhase::Start => {
                    self.fling = Vec2::ZERO;
                    // An id that is already down never lifted (the app was paused): it starts over.
                    if self.down.contains(id) {
                        self.down.retain(|d| d != id);
                        if self.finger.as_ref().is_some_and(|f| f.id == *id) {
                            self.finger = None;
                        }
                    }
                    let others = !self.down.is_empty();
                    self.down.push(*id);
                    if others {
                        // A second finger: pinch and two finger scroll are egui's. The press of the
                        // first finger (a caret placed, a selection begun) is undone, as for a scroll.
                        // Nor is a third finger a scroll of its own while two are down.
                        if let Some(first) = self.finger.take()
                            && !first.panning
                            && !first.selecting
                        {
                            step.began = Some(first.before);
                        }
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
                    self.down.retain(|d| d != id);
                    if self.finger.as_ref().is_some_and(|f| f.id == *id) {
                        if self.finger.as_ref().is_some_and(|f| f.panning) {
                            self.just_ended = true;
                        }
                        self.finger = None;
                    }
                }
            }
        }
        if !moved.is_finite() {
            moved = Vec2::ZERO;
        }
        step.delta = moved;
        if dt > 0.0 && dt.is_finite() {
            // A long frame (a slow redraw, a paused app) must not throw the page: time passes at
            // most this fast.
            let dt = dt.min(MAX_DT);
            if moved != Vec2::ZERO || self.finger.as_ref().is_some_and(|f| f.panning) {
                // Follow the finger's speed: an average of the last frames, so one jittery sample
                // doesn't decide how far the page coasts.
                let now = (moved / dt).clamp(Vec2::splat(-MAX_FLING), Vec2::splat(MAX_FLING));
                self.fling = self.fling * 0.4 + now * 0.6;
            } else if self.finger.is_none() && self.fling != Vec2::ZERO {
                if self.fling.length() < MIN_FLING {
                    self.fling = Vec2::ZERO;
                } else {
                    // The distance covered while the speed decays: the integral, not a step.
                    step.delta += self.fling * ((1.0 - (-FRICTION * dt).exp()) / FRICTION);
                    self.fling *= (-FRICTION * dt).exp();
                    step.coasting = true;
                }
            }
        }
        step
    }
}

/// How long (seconds) a finger rests on the page before it is a press and hold: the page gets its
/// press then, and egui's long touch (`max_click_duration`) opens the context menu a little later.
pub(crate) const HOLD: f64 = 0.2;

/// A finger that has been down this long (seconds) with no touch event at all is taken as lost (its
/// lift never came, say because the app was paused): the gate forgets it.
const STALE_TOUCH: f64 = 10.0;

/// What the gate needs to know about the page and the finger.
pub(crate) struct Env<'a> {
    /// Where the page is drawn: a touch elsewhere (the ribbon, a status bar button) is the
    /// interface's, which needs its presses at once.
    pub area: Option<Rect>,
    /// A tool owns the pointer (a pen, the eraser, the lasso, Draw Table): it draws from the first
    /// point, so it needs the press at once.
    pub tool: bool,
    /// The page was coasting as the finger landed: the touch is meant to stop it, and does no more.
    pub coasting: bool,
    /// Whether a touch at this point is the second tap of a double tap, which selects a word as it
    /// goes down (and then by words as it drags).
    pub second_tap: &'a dyn Fn(Pos2) -> bool,
    /// Whether a dialog or a menu drawn over the page is under this point.
    pub covered: &'a dyn Fn(Pos2) -> bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Mode {
    /// No finger is being held back.
    #[default]
    Free,
    /// One finger on the page, not yet a scroll, a tap or a hold: its mouse presses are held.
    Undecided { id: TouchId, start: Pos2, since: f64, stopper: bool },
    /// A scroll or a pinch: the page gets no press from these fingers.
    Scrolling,
    /// The page has its press (a hold, a tool or the interface): mouse events go through.
    Pressed,
    /// The rest of this batch's mouse presses belong to a touch that was settled (and dropped or
    /// replaced by a tap of the gate's own); `own_gone` says the gate sends the pointer's departure.
    Releasing { own_gone: bool },
}

/// Holds back the mouse presses egui-winit makes for the first finger until the gesture is known
/// (see the module's docs). Works on a frame's raw events, before egui sees them. The pointer's
/// moves always go through (egui starts a pinch only when the pointer has a position); only the
/// press is held.
#[derive(Default)]
pub(crate) struct Gate {
    mode: Mode,
    /// Every finger that is down.
    down: Vec<TouchId>,
    /// When a touch event last came, and where.
    last_touch: f64,
    last_pos: Pos2,
    /// Events for the next frame: a tap's release comes one frame after its press, as a real tap's
    /// does, so what reacts to the press in its own frame (a drag begun on a picture or a table, a
    /// selection area) sees it end.
    queued: Vec<Event>,
}

impl Gate {
    /// Let go of everything (the view changed under the finger).
    pub fn cancel(&mut self) {
        // A queued release still goes out: its press did.
        let queued = std::mem::take(&mut self.queued);
        *self = Gate { queued, ..Gate::default() };
    }

    /// Filter a frame's events: the touches stay, the mouse presses a finger stands in for come when
    /// the finger's meaning is clear. `now` is the frame's time in seconds. Returns how long until a
    /// frame is wanted: a finger that rests becomes a press and hold, a tap's release is queued.
    pub fn filter(&mut self, events: &mut Vec<Event>, now: f64, env: &Env) -> Option<f64> {
        let button = |pos, pressed| Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        if !self.down.is_empty() && now - self.last_touch > STALE_TOUCH {
            self.cancel();
        }
        let input = std::mem::take(events);
        let mut out = std::mem::take(&mut self.queued);
        out.reserve(input.len() + 3);
        for e in input {
            match &e {
                // The window lost focus: a finger that was down won't report its lift. The press the
                // page has is released, or what it started (a drag) would never end.
                Event::WindowFocused(false) => {
                    if self.mode == Mode::Pressed {
                        out.push(button(self.last_pos, false));
                    }
                    self.cancel();
                    out.push(e);
                }
                Event::Touch { id, phase, pos, .. } => {
                    self.last_touch = now;
                    if pos.is_finite() {
                        self.last_pos = *pos;
                    }
                    match phase {
                        TouchPhase::Start => {
                            // A tap of this batch has its release before the next finger lands.
                            out.append(&mut self.queued);
                            if !self.down.contains(id) {
                                self.down.push(*id);
                            }
                            if self.down.len() > 1 {
                                // A second finger: a pinch or a two finger scroll. A press that was
                                // held back is never sent; one already sent stays (its release comes).
                                if matches!(self.mode, Mode::Undecided { .. }) {
                                    self.mode = Mode::Scrolling;
                                }
                            } else if pos.is_finite()
                                && !env.tool
                                && env.area.is_some_and(|a| a.contains(*pos))
                                && !(env.covered)(*pos)
                                && !(env.second_tap)(*pos)
                            {
                                self.mode = Mode::Undecided { id: *id, start: *pos, since: now, stopper: env.coasting };
                            } else {
                                self.mode = Mode::Pressed;
                            }
                        }
                        TouchPhase::Move => {
                            if let Mode::Undecided { id: held, start, .. } = &self.mode
                                && held == id
                                && pos.is_finite()
                                && start.distance(*pos) > SLOP
                            {
                                self.mode = Mode::Scrolling;
                            }
                        }
                        TouchPhase::End | TouchPhase::Cancel => {
                            self.down.retain(|d| d != id);
                            match std::mem::take(&mut self.mode) {
                                Mode::Undecided { id: held, start, stopper, .. } if held == *id => {
                                    out.push(e.clone());
                                    if *phase == TouchPhase::End && !stopper {
                                        // The finger lifted without travelling: a tap. The press now, the
                                        // release (and the pointer leaving) with the next frame.
                                        // (An earlier tap of this same batch has its release before this press.)
                                        out.append(&mut self.queued);
                                        out.extend([Event::PointerMoved(start), button(start, true)]);
                                        self.queued = vec![button(start, false), Event::PointerGone];
                                        self.mode = Mode::Releasing { own_gone: true };
                                    } else {
                                        // Only stopped a coast, or cancelled: no tap.
                                        self.mode = Mode::Releasing { own_gone: false };
                                    }
                                    continue;
                                }
                                Mode::Scrolling if self.down.is_empty() => self.mode = Mode::Releasing { own_gone: false },
                                Mode::Pressed if self.down.is_empty() => {
                                    // A cancelled touch comes with no release (egui-winit sends only the
                                    // pointer's departure): the press the page has is released here.
                                    if *phase == TouchPhase::Cancel {
                                        out.extend([e.clone(), button(self.last_pos, false)]);
                                        self.mode = Mode::Free;
                                        continue;
                                    }
                                    self.mode = Mode::Free;
                                }
                                other => self.mode = other,
                            }
                        }
                    }
                    out.push(e);
                }
                // A press that is held back, or belongs to a touch that was settled without one.
                Event::PointerButton { .. } if matches!(self.mode, Mode::Undecided { .. } | Mode::Scrolling | Mode::Releasing { .. }) => {}
                // The pointer leaving after a tap the gate sends itself.
                Event::PointerGone if matches!(self.mode, Mode::Releasing { own_gone: true }) => {}
                _ => out.push(e),
            }
        }
        let mut wake = if self.queued.is_empty() { None } else { Some(0.0) };
        if let Mode::Undecided { start, since, .. } = self.mode {
            if now - since >= HOLD {
                out.extend([Event::PointerMoved(start), button(start, true)]);
                self.mode = Mode::Pressed;
            } else {
                wake = Some((since + HOLD - now).max(0.0));
            }
        }
        if matches!(self.mode, Mode::Releasing { .. }) {
            self.mode = if self.down.is_empty() { Mode::Free } else { Mode::Scrolling };
        }
        *events = out;
        wake
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
        // A second finger landing on a finger that has not scrolled gives back the selection too.
        let mut tap = Pan::default();
        run(&mut tap, &[touch(1, TouchPhase::Start, pos2(300.0, 400.0))]);
        assert_eq!(run(&mut tap, &[touch(2, TouchPhase::Start, pos2(500.0, 400.0))]).began, Some(sel()));
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

    #[test]
    fn a_position_that_is_not_a_number_is_ignored() {
        let mut pan = Pan::default();
        run(&mut pan, &[touch(1, TouchPhase::Start, pos2(300.0, 400.0))]);
        run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, 350.0))]);
        assert!(pan.panning());
        for bad in [pos2(f32::INFINITY, 340.0), pos2(f32::NAN, f32::NAN), pos2(300.0, f32::NEG_INFINITY)] {
            let s = run(&mut pan, &[touch(1, TouchPhase::Move, bad), touch(1, TouchPhase::Move, pos2(300.0, 340.0))]);
            assert!(s.delta.is_finite(), "{bad:?}: {:?}", s.delta);
        }
        // A start at a bad position never begins a finger; the finger that is down stays ours.
        let s = run(&mut pan, &[touch(2, TouchPhase::Start, pos2(f32::NAN, 10.0))]);
        assert_eq!(s, Step::default());
        assert!(pan.panning());
        run(&mut pan, &[touch(1, TouchPhase::End, pos2(300.0, 340.0))]);
        // The coast that follows is a number, and ends.
        for _ in 0..2_000 {
            let s = run(&mut pan, &[]);
            assert!(s.delta.is_finite());
            if !s.coasting {
                break;
            }
        }
        assert!(pan.idle());
    }

    // ---- the gate ----

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
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

    struct World {
        tool: bool,
        coasting: bool,
        second_tap: bool,
        covered: bool,
    }

    const CALM: World = World { tool: false, coasting: false, second_tap: false, covered: false };

    fn gate(g: &mut Gate, mut events: Vec<Event>, now: f64, w: &World) -> (Vec<Event>, Option<f64>) {
        let second_tap = |_: Pos2| w.second_tap;
        let covered = |_: Pos2| w.covered;
        let env = Env { area: Some(area()), tool: w.tool, coasting: w.coasting, second_tap: &second_tap, covered: &covered };
        let wake = g.filter(&mut events, now, &env);
        (events, wake)
    }

    /// The mouse presses and releases among `events` (the pointer's moves go through as they are).
    fn mouse_events(events: &[Event]) -> Vec<&Event> {
        events.iter().filter(|e| matches!(e, Event::PointerButton { .. })).collect()
    }

    #[test]
    fn a_tap_reaches_the_page_as_a_press_and_then_a_release_when_the_finger_lifts() {
        let mut g = Gate::default();
        let p = pos2(300.0, 400.0);
        let (out, wake) = gate(&mut g, down(1, p), 0.0, &CALM);
        assert_eq!(out, vec![touch(1, TouchPhase::Start, p), Event::PointerMoved(p)], "the pointer arrives, the press is held back");
        assert_eq!(wake, Some(HOLD), "a frame is wanted when the finger would become a hold");
        // Jitter within the slop changes nothing.
        let (out, _) = gate(&mut g, moved(1, pos2(303.0, 402.0)), 0.05, &CALM);
        assert_eq!(out, moved(1, pos2(303.0, 402.0)));
        let (out, wake) = gate(&mut g, up(1, pos2(303.0, 402.0)), 0.1, &CALM);
        assert_eq!(
            out,
            vec![touch(1, TouchPhase::End, pos2(303.0, 402.0)), Event::PointerMoved(p), button(p, true)],
            "the page gets the press where the finger landed"
        );
        assert_eq!(wake, Some(0.0), "and a frame is wanted for the release");
        let (out, wake) = gate(&mut g, Vec::new(), 0.12, &CALM);
        assert_eq!(out, vec![button(p, false), Event::PointerGone], "the release and the pointer's departure come with the next frame");
        assert_eq!(wake, None);
        // Everything is back to normal for the next touch.
        let (out, _) = gate(&mut g, down(2, p), 1.0, &CALM);
        assert_eq!(mouse_events(&out).len(), 0);
    }

    #[test]
    fn a_scroll_never_reaches_the_page_as_a_press() {
        let mut g = Gate::default();
        let p = pos2(300.0, 400.0);
        gate(&mut g, down(1, p), 0.0, &CALM);
        let (out, _) = gate(&mut g, moved(1, pos2(300.0, 380.0)), 0.02, &CALM);
        assert_eq!(out, moved(1, pos2(300.0, 380.0)), "past the slop: a scroll, the pointer follows and no press comes");
        let (out, wake) = gate(&mut g, moved(1, pos2(300.0, 300.0)), 0.05, &CALM);
        assert_eq!((mouse_events(&out).len(), wake), (0, None));
        // A scroll that rests for longer than a hold is still a scroll.
        let (out, _) = gate(&mut g, Vec::new(), 1.0, &CALM);
        assert!(out.is_empty());
        let (out, _) = gate(&mut g, up(1, pos2(300.0, 300.0)), 1.1, &CALM);
        assert_eq!(out, vec![touch(1, TouchPhase::End, pos2(300.0, 300.0)), Event::PointerGone], "no release for a press the page never got");
    }

    #[test]
    fn a_finger_that_rests_gives_the_page_its_press_as_a_hold() {
        let mut g = Gate::default();
        let p = pos2(300.0, 400.0);
        gate(&mut g, down(1, p), 0.0, &CALM);
        let (out, wake) = gate(&mut g, Vec::new(), HOLD - 0.05, &CALM);
        assert_eq!((out.len(), wake.is_some()), (0, true), "not yet");
        let (out, wake) = gate(&mut g, Vec::new(), HOLD + 0.01, &CALM);
        assert_eq!(out, vec![Event::PointerMoved(p), button(p, true)]);
        assert_eq!(wake, None);
        // From now on the finger is the page's: moves and the release go through.
        let (out, _) = gate(&mut g, moved(1, pos2(305.0, 400.0)), 0.3, &CALM);
        assert_eq!(out, moved(1, pos2(305.0, 400.0)));
        let (out, _) = gate(&mut g, up(1, pos2(305.0, 400.0)), 0.6, &CALM);
        assert_eq!(out, up(1, pos2(305.0, 400.0)), "the press the page got is released");
    }

    #[test]
    fn a_second_finger_makes_it_a_pinch_and_no_press() {
        let mut g = Gate::default();
        let (a, b) = (pos2(300.0, 400.0), pos2(500.0, 400.0));
        // Two fingers in the same frame (the first's mouse events come with it).
        let mut both = down(1, a);
        both.push(touch(2, TouchPhase::Start, b));
        let (out, _) = gate(&mut g, both, 0.0, &CALM);
        assert_eq!(mouse_events(&out).len(), 0);
        let (out, _) = gate(&mut g, up(1, a), 0.1, &CALM);
        assert_eq!(out, vec![touch(1, TouchPhase::End, a), Event::PointerGone]);
        let (out, _) = gate(&mut g, vec![touch(2, TouchPhase::End, b)], 0.1, &CALM);
        assert_eq!(out.len(), 1);
        // The next finger is a fresh start.
        let (out, wake) = gate(&mut g, down(3, a), 1.0, &CALM);
        assert_eq!((mouse_events(&out).len(), wake.is_some()), (0, true));
        // A second finger after a hold: the press the page got is still released.
        let mut g = Gate::default();
        gate(&mut g, down(1, a), 0.0, &CALM);
        gate(&mut g, Vec::new(), 0.3, &CALM);
        gate(&mut g, vec![touch(2, TouchPhase::Start, b)], 0.35, &CALM);
        let (out, _) = gate(&mut g, up(1, a), 0.4, &CALM);
        assert!(out.contains(&button(a, false)));
    }

    #[test]
    fn touches_that_are_not_the_pages_or_need_the_press_at_once_pass_through() {
        let p = pos2(300.0, 400.0);
        for (name, w) in [
            ("a tool", World { tool: true, ..CALM }),
            ("a second tap", World { second_tap: true, ..CALM }),
            ("a dialog over the page", World { covered: true, ..CALM }),
        ] {
            let mut g = Gate::default();
            let (out, wake) = gate(&mut g, down(1, p), 0.0, &w);
            assert_eq!(out, down(1, p), "{name}");
            assert_eq!(wake, None, "{name}");
            let (out, _) = gate(&mut g, up(1, p), 0.1, &w);
            assert_eq!(out, up(1, p), "{name}");
        }
        // The ribbon, the status bar: outside the page.
        let mut g = Gate::default();
        let outside = pos2(300.0, 700.0);
        let (out, _) = gate(&mut g, down(1, outside), 0.0, &CALM);
        assert_eq!(out, down(1, outside));
        // A position that is not a number is not held either.
        let mut g = Gate::default();
        let bad = pos2(f32::NAN, 5.0);
        let (out, _) = gate(&mut g, vec![touch(1, TouchPhase::Start, bad)], 0.0, &CALM);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_touch_that_stops_a_coast_does_nothing_more() {
        let mut g = Gate::default();
        let p = pos2(300.0, 400.0);
        let w = World { coasting: true, ..CALM };
        gate(&mut g, down(1, p), 0.0, &w);
        let (out, _) = gate(&mut g, up(1, p), 0.05, &w);
        assert_eq!(out, vec![touch(1, TouchPhase::End, p), Event::PointerGone], "no tap: it only stopped the page");
        // A finger that lands on a coasting page and scrolls is a scroll as usual.
        let mut g = Gate::default();
        gate(&mut g, down(1, p), 0.0, &w);
        let (out, _) = gate(&mut g, moved(1, pos2(300.0, 350.0)), 0.05, &w);
        assert_eq!(mouse_events(&out).len(), 0);
    }

    #[test]
    fn a_cancelled_touch_is_not_a_tap() {
        let mut g = Gate::default();
        let p = pos2(300.0, 400.0);
        gate(&mut g, down(1, p), 0.0, &CALM);
        let (out, _) = gate(&mut g, vec![touch(1, TouchPhase::Cancel, p), Event::PointerGone], 0.05, &CALM);
        assert_eq!(out, vec![touch(1, TouchPhase::Cancel, p), Event::PointerGone]);
    }

    #[test]
    fn a_third_finger_is_not_a_scroll_of_its_own() {
        let mut pan = Pan::default();
        let (a, b, c) = (pos2(100.0, 100.0), pos2(200.0, 100.0), pos2(300.0, 100.0));
        run(&mut pan, &[touch(1, TouchPhase::Start, a), touch(2, TouchPhase::Start, b)]);
        run(&mut pan, &[touch(1, TouchPhase::End, a)]);
        // Fingers 2 and 3 are down: finger 3 doesn't scroll on its own on top of the two finger scroll.
        run(&mut pan, &[touch(3, TouchPhase::Start, c)]);
        let s = run(&mut pan, &[touch(3, TouchPhase::Move, pos2(300.0, 20.0))]);
        assert_eq!(s.delta, Vec2::ZERO);
        assert!(!pan.panning());
        // All up: the next finger scrolls again.
        run(&mut pan, &[touch(2, TouchPhase::End, b), touch(3, TouchPhase::End, c)]);
        run(&mut pan, &[touch(4, TouchPhase::Start, a)]);
        let s = run(&mut pan, &[touch(4, TouchPhase::Move, pos2(100.0, 50.0))]);
        assert_eq!(s.delta, vec2(0.0, -50.0));
    }

    #[test]
    fn a_long_frame_does_not_throw_the_page() {
        let mut pan = Pan::default();
        run(&mut pan, &[touch(1, TouchPhase::Start, pos2(300.0, 400.0))]);
        for y in [380.0, 350.0, 320.0, 290.0] {
            run(&mut pan, &[touch(1, TouchPhase::Move, pos2(300.0, y))]);
        }
        run(&mut pan, &[touch(1, TouchPhase::End, pos2(300.0, 290.0))]);
        // A five second frame (the app was paused) moves the page about as far as 50 ms would.
        let long = pan.frame(&[], area(), |_| false, false, &sel(), 5.0);
        let mut pan2 = Pan::default();
        run(&mut pan2, &[touch(1, TouchPhase::Start, pos2(300.0, 400.0))]);
        for y in [380.0, 350.0, 320.0, 290.0] {
            run(&mut pan2, &[touch(1, TouchPhase::Move, pos2(300.0, y))]);
        }
        run(&mut pan2, &[touch(1, TouchPhase::End, pos2(300.0, 290.0))]);
        let short = pan2.frame(&[], area(), |_| false, false, &sel(), MAX_DT);
        assert_eq!(long.delta, short.delta);
        assert!(long.delta.length() < 400.0, "{:?}", long.delta);
    }

    #[test]
    fn a_lost_lift_does_not_turn_every_later_finger_into_a_second_one() {
        let p = pos2(300.0, 400.0);
        // The gate: a finger went down, the app was paused, no lift came; much later a finger taps.
        let mut g = Gate::default();
        gate(&mut g, down(1, p), 0.0, &CALM);
        gate(&mut g, Vec::new(), 0.5, &CALM);
        let (out, _) = gate(&mut g, down(2, p), 30.0, &CALM);
        assert_eq!(mouse_events(&out).len(), 0, "held back as a fresh touch");
        let (out, _) = gate(&mut g, up(2, p), 30.05, &CALM);
        assert!(out.contains(&button(p, true)), "and delivered as a tap");
        let (out, _) = gate(&mut g, Vec::new(), 30.06, &CALM);
        assert!(out.contains(&button(p, false)));
        // Losing the focus forgets fingers at once.
        let mut g = Gate::default();
        gate(&mut g, down(1, p), 0.0, &CALM);
        gate(&mut g, vec![Event::WindowFocused(false)], 0.1, &CALM);
        let (out, _) = gate(&mut g, down(2, p), 0.2, &CALM);
        assert_eq!(mouse_events(&out).len(), 0);
        let (out, _) = gate(&mut g, up(2, p), 0.25, &CALM);
        assert!(out.contains(&button(p, true)));
        // The scroll tracking: forgotten when egui reports no fingers.
        let mut pan = Pan::default();
        run(&mut pan, &[touch(1, TouchPhase::Start, p)]);
        pan.forget_fingers();
        run(&mut pan, &[touch(2, TouchPhase::Start, p)]);
        let s = run(&mut pan, &[touch(2, TouchPhase::Move, pos2(300.0, 350.0))]);
        assert_eq!(s.delta, vec2(0.0, -50.0));
    }

    #[test]
    fn a_cancelled_or_abandoned_press_is_released() {
        let p = pos2(300.0, 400.0);
        // A hold gave the page its press; Android then cancels the touch (egui-winit sends only the
        // pointer's departure).
        let mut g = Gate::default();
        gate(&mut g, down(1, p), 0.0, &CALM);
        let (out, _) = gate(&mut g, Vec::new(), 0.3, &CALM);
        assert!(out.contains(&button(p, true)));
        let (out, _) = gate(&mut g, vec![touch(1, TouchPhase::Cancel, p), Event::PointerGone], 0.4, &CALM);
        assert_eq!(out, vec![touch(1, TouchPhase::Cancel, p), button(p, false), Event::PointerGone], "the press is released");
        // The window loses focus with a finger down on the page's press.
        let mut g = Gate::default();
        gate(&mut g, down(1, p), 0.0, &CALM);
        gate(&mut g, Vec::new(), 0.3, &CALM);
        let (out, _) = gate(&mut g, vec![Event::WindowFocused(false)], 0.4, &CALM);
        assert_eq!(out, vec![button(p, false), Event::WindowFocused(false)]);
        // A finger the gate held back has no press to release.
        let mut g = Gate::default();
        gate(&mut g, down(1, p), 0.0, &CALM);
        let (out, _) = gate(&mut g, vec![Event::WindowFocused(false)], 0.1, &CALM);
        assert_eq!(out, vec![Event::WindowFocused(false)]);
    }

    #[test]
    fn two_taps_in_one_batch_each_keep_their_own_release() {
        let (a, b) = (pos2(100.0, 100.0), pos2(500.0, 300.0));
        let mut g = Gate::default();
        let mut batch = down(1, a);
        batch.extend(up(1, a));
        batch.extend(down(2, b));
        batch.extend(up(2, b));
        let (out, _) = gate(&mut g, batch, 0.0, &CALM);
        let buttons: Vec<&Event> = out.iter().filter(|e| matches!(e, Event::PointerButton { .. })).collect();
        assert_eq!(
            buttons,
            vec![&button(a, true), &button(a, false), &button(b, true)],
            "press, release, press, in that order and at their own places"
        );
        // The second tap's release comes with the next frame.
        let (out, _) = gate(&mut g, Vec::new(), 0.016, &CALM);
        assert_eq!(out, vec![button(b, false), Event::PointerGone]);
    }
}
