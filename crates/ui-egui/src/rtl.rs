//! Mirrored window layout for a right-to-left interface language (Arabic).
//!
//! egui has no global direction, but `ui.horizontal` runs right to left wherever the enclosing
//! layout prefers it (`Layout::prefer_right_to_left`): a right-to-left row, or a column whose
//! items are aligned to the right. So direction is inherited from a root: each panel, window and
//! menu body is wrapped in [`root`], columns are opened with [`vertical`] (egui's `ui.vertical`
//! always resets to left-aligned), and "far side" clusters use [`far_side`]. Widgets that paint
//! their own parts ask [`flows`] where their leading edge is.
//!
//! An area that egui lays out left to right on its own (a `Grid`, a far-side cluster) stays left
//! to right inside, because everything here follows the enclosing layout rather than the language.
//! The document itself is never mirrored: its direction belongs to the document.

use std::sync::Arc;

use egui::epaint::text::TextDirection;
use egui::text::LayoutJob;
use egui::{Align, Align2, Galley, InnerResponse, Layout, Rect, Ui, UiBuilder, pos2};

use craft_ui::direction::{self, Direction};

fn dir(rtl: bool) -> Direction {
    if rtl { Direction::Rtl } else { Direction::Ltr }
}

/// Install the interface direction on `ctx` when the language's has changed: the shared
/// `craft_ui::direction` state, and egui's own chrome (check boxes and radio buttons with the mark
/// at the reading start, submenus, scroll bars, title bars, the font combo) through
/// `Style::right_to_left`.
pub fn sync(ctx: &egui::Context) {
    let want = dir(on());
    if direction::HasDirection::direction(ctx) != want {
        direction::apply(ctx, want);
    }
}

/// Is the interface language written right to left?
pub fn on() -> bool {
    crate::i18n::current().rtl()
}

/// Is `ui` part of the mirrored interface: a right-to-left language, in an area that lays its
/// content out from the right (so rows opened in it run right to left)? Never in a left-to-right
/// language, whose far-side clusters also fill from the right but are not mirrored.
pub fn flows(ui: &Ui) -> bool {
    on() && ui.layout().prefer_right_to_left()
}

/// Run a container's body (a panel, window, popup or menu, which egui starts left-aligned) in the
/// interface's direction. In a left-to-right language this is just `add`.
pub fn root<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !on() || flows(ui) {
        return add(ui);
    }
    ui.scope_builder(UiBuilder::new().layout(Layout::top_down(Align::Max)), add).inner
}

/// `ui.vertical`, aligned to the side rows start from: a column inside a right-to-left area keeps
/// its content on the right and its rows right to left.
pub fn vertical<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    if flows(ui) { ui.scope_builder(UiBuilder::new().layout(Layout::top_down(Align::Max)), add) } else { ui.vertical(add) }
}

/// `ui.horizontal_top`, with its items running from the side reading starts from: a row opened
/// in a right-to-left area is a right-to-left layout of its own, so its first column sits at the
/// right edge.
pub fn horizontal_top<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    if flows(ui) { ui.scope_builder(UiBuilder::new().layout(Layout::right_to_left(Align::Min)), add) } else { ui.horizontal_top(add) }
}

/// A button's text whose `>>` or `<<` points the way things move across the window (Add >>, <<
/// Remove): in a mirrored interface the two swap, because the list items move to is on the other side.
pub fn arrows(text: &str) -> String {
    if on() { text.replace(">>", "\u{1}").replace("<<", ">>").replace('\u{1}', "<<") } else { text.to_string() }
}

/// Text whose `→` points forward, to more of something (More templates →): forward is to the
/// left in a mirrored interface, so the arrow there is `←`. A translation that already turned it
/// round keeps its `←`.
pub fn forward(text: &str) -> String {
    if on() { text.replace('→', "←") } else { text.to_string() }
}

/// The side a list's rows start from: the right in a right-to-left interface. For a list's own
/// layout (`top_down`, `top_down_justified`), which egui would start at the left.
pub fn start_align() -> Align {
    if on() { Align::RIGHT } else { Align::LEFT }
}

/// Where a text field sets its text and hint: from the right edge in a right-to-left interface
/// (a field takes the side from the layout it is in, and egui starts most layouts at the left).
pub fn field_align() -> Align2 {
    if on() { Align2::RIGHT_TOP } else { Align2::LEFT_TOP }
}

/// A multi-line text field's text laid out for editing, for `TextEdit::layouter` with [`field_align`]:
/// wrapped at `wrap_width`; in a right-to-left interface every paragraph is right to left, whatever letter it
/// starts with (as the text boxes of a right-to-left word processor are), elsewhere each paragraph takes the
/// direction of its first strong letter; and a right-to-left paragraph's rows end at the field's right edge,
/// also when they are short (an empty text is left as narrow as it is: [`field_align`] puts it, and its caret,
/// at the right). Mixed scripts, numbers and punctuation are ordered by the bidi algorithm, and the
/// caret moves as it does in the document. The text is in the field's own colour and body font.
pub fn field_galley(ui: &Ui, text: &str, wrap_width: f32) -> Arc<Galley> {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let line_height = ui.fonts_mut(|f| f.row_height(&font)) + ui.spacing().extra_text_line_spacing;
    // The placeholder colour is painted in the field's text colour.
    let mut job = LayoutJob::simple(text.to_owned(), font, egui::Color32::PLACEHOLDER, wrap_width);
    job.direction = if on() { TextDirection::RightToLeft } else { TextDirection::Auto };
    // Each paragraph at its start: a right-to-left one, and the caret of an empty one, at the right.
    job.halign = Align::Min;
    job.halign_min_is_start = true;
    job.keep_trailing_whitespace = true;
    job.language = ui.language();
    for section in &mut job.sections {
        section.format.line_height = Some(line_height);
    }
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    // Right-to-left rows end at the widest row; as wide as the field, they end at its right edge. An empty text
    // stays as narrow as it is: the field sets it, with its caret, against the side `field_align` names, and
    // while it is empty the field measures it against the hint, not against its whole width, so a galley as
    // wide as the field would put the caret that far past the hint, out of the field.
    if !text.is_empty() && galley.rows.iter().any(|row| row.rtl_paragraph) { as_wide_as(galley, wrap_width) } else { galley }
}

/// Read-only text whose paragraphs keep their own direction (each from its first strong letter, as UAX #9
/// sets a paragraph's direction), wrapped at `width`, each row against the edge its paragraph starts from: a
/// right-to-left paragraph at the right, the others at the left, in either interface. The galley is as wide
/// as `width`, so a selectable `Label` of it keeps every row where it is.
pub fn paragraphs(ui: &Ui, text: &str, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple(text.to_owned(), egui::TextStyle::Body.resolve(ui.style()), egui::Color32::PLACEHOLDER, width);
    job.direction = TextDirection::Auto;
    job.halign = Align::Min;
    job.halign_min_is_start = true;
    job.language = ui.language();
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    as_wide_as(galley, width)
}

/// `galley` with every row against the edge its paragraph starts from within `width`, and as wide as `width`;
/// as it was when `width` is not a usable width or a row is wider than it.
fn as_wide_as(galley: Arc<Galley>, width: f32) -> Arc<Galley> {
    let aligned = craft_ui::text::align_galley_rows(galley.clone(), width, |row| if row.rtl_paragraph { Align::Max } else { Align::Min });
    match aligned {
        craft_ui::text::AlignedGalley::Aligned { galley: wide, paint_offset, .. } if paint_offset == egui::Vec2::ZERO => wide,
        _ => galley,
    }
}

/// The layout for a row's items that belong at its far end (the side rows end on): filled from
/// that end inwards.
pub fn far_side(ui: &Ui, align: Align) -> Layout {
    if flows(ui) { Layout::left_to_right(align) } else { Layout::right_to_left(align) }
}

/// A row layout in the direction of `ui`, for `allocate_ui_with_layout` and `with_layout`.
pub fn row_layout(ui: &Ui, align: Align) -> Layout {
    if flows(ui) { Layout::right_to_left(align) } else { Layout::left_to_right(align) }
}

/// The x coordinate `inset` points in from the leading edge of `rect` (left, or right when `rtl`).
pub fn lead_x(rtl: bool, rect: Rect, inset: f32) -> f32 {
    let d = dir(rtl);
    direction::start_x(&d, rect) + direction::forward(&d) * inset
}

/// The x coordinate `inset` points in from the trailing edge of `rect`.
pub fn trail_x(rtl: bool, rect: Rect, inset: f32) -> f32 {
    lead_x(!rtl, rect, inset)
}

/// Text anchored at its leading edge and vertically centred: left-centre, or right-centre when `rtl`.
pub fn lead_center(rtl: bool) -> Align2 {
    direction::align2(&dir(rtl), Align2::LEFT_CENTER)
}

/// Text anchored at its leading edge and top.
pub fn lead_top(rtl: bool) -> Align2 {
    direction::align2(&dir(rtl), Align2::LEFT_TOP)
}

/// Where to put something `width` wide so that it starts `inset` in from the leading edge of
/// `rect`: its left x.
pub fn lead_left(rtl: bool, rect: Rect, inset: f32, width: f32) -> f32 {
    if rtl { rect.max.x - inset - width } else { rect.min.x + inset }
}

/// The leading top corner of `rect`.
pub fn lead_corner(rtl: bool, rect: Rect) -> egui::Pos2 {
    pos2(lead_x(rtl, rect, 0.0), rect.min.y)
}

/// A side panel on the side reading starts from (left; right in a right-to-left interface).
pub fn start_panel(id: &'static str) -> egui::Panel {
    direction::start_panel(&dir(on()), id)
}

/// A side panel on the side reading ends on (right; left in a right-to-left interface).
pub fn end_panel(id: &'static str) -> egui::Panel {
    direction::end_panel(&dir(on()), id)
}

/// `egui::Popup::menu`, with its items aligned to the side reading starts from.
pub fn menu(response: &egui::Response) -> egui::Popup<'_> {
    let popup = egui::Popup::menu(response);
    if on() { popup.layout(menu_layout()) } else { popup }
}

/// A right-to-left menu's layout: items against the right edge. Not justified, because egui
/// starts a full-width button's text at its left.
fn menu_layout() -> Layout {
    Layout::top_down(Align::Max)
}

/// A menu item: `text`, with its keyboard shortcut at the far side. `min_width` keeps a menu's
/// items one width. A screen reader reads the label first, as the row reads, then the shortcut
/// (AGENTS.md rule 21): a mirrored row draws the shortcut first, which egui would read first.
pub fn menu_item<'a>(ui: &Ui, text: &'a str, shortcut: &'a str, min_width: f32) -> impl egui::Widget + use<'a> {
    let button = if flows(ui) {
        egui::Button::new(egui::RichText::new(shortcut).weak()).right_text(text)
    } else {
        egui::Button::new(text).shortcut_text(shortcut)
    };
    let button = button.min_size(egui::vec2(min_width, 0.0));
    move |ui: &mut Ui| {
        let row = ui.add(button);
        let name = if shortcut.is_empty() { text.to_string() } else { format!("{text} {shortcut}") };
        let enabled = ui.is_enabled();
        row.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &name));
        row
    }
}

/// A drop-down list: [`craft_ui::ComboBox`], whose menu lays its rows out from the left (egui's
/// menu layout), with the rows starting from the side reading starts from: in a right-to-left
/// interface each row's text is at the right, the row as wide as the list, and a row of several
/// items runs right to left. Elsewhere it is exactly `craft_ui::ComboBox`.
#[must_use = "You should call .show_ui"]
pub struct ComboBox<'a>(craft_ui::ComboBox<'a>);

impl<'a> ComboBox<'a> {
    /// A combo box without a label.
    pub fn from_id_salt(id_salt: impl egui::AsIdSalt) -> Self {
        Self(craft_ui::ComboBox::from_id_salt(id_salt))
    }

    /// The button's minimum width.
    pub fn width(self, width: f32) -> Self {
        Self(self.0.width(width))
    }

    /// The maximum height of the menu's contents.
    pub fn height(self, height: f32) -> Self {
        Self(self.0.height(height))
    }

    /// What the button shows as the selected value.
    pub fn selected_text(self, selected_text: impl egui::IntoAtoms<'a>) -> Self {
        Self(self.0.selected_text(selected_text))
    }

    /// Shows the button, and `menu_contents` in the menu while it is open, laid out from the
    /// reading start ([`list_rows`]).
    pub fn show_ui<R>(self, ui: &mut Ui, menu_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<Option<R>> {
        self.0.show_ui(ui, |ui| list_rows(ui, menu_contents))
    }
}

/// A list's rows in a menu, from the side reading starts from: in a right-to-left interface every
/// row is as wide as the list with its text at the right. Elsewhere this is just `add`.
pub fn list_rows<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !on() || flows(ui) {
        return add(ui);
    }
    ui.with_layout(Layout::top_down_justified(Align::Max), add).inner
}

/// `Response::context_menu`, with its items aligned to the side reading starts from.
pub fn context_menu(response: &egui::Response, add: impl FnOnce(&mut Ui)) {
    let popup = egui::Popup::context_menu(response);
    let popup = if on() { popup.layout(menu_layout()) } else { popup };
    popup.show(add);
}

/// Run `add` in a left-to-right island inside a right-to-left area, for content egui can only
/// lay out from the left (a `Grid`, a horizontally scrolling strip). The island is set against the
/// leading (right) edge using the width it had last frame. Outside a right-to-left area this is
/// just `add`.
pub fn island<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !flows(ui) {
        return add(ui);
    }
    // The island gets all the room there is, so its content takes its natural width; it starts
    // where content of last frame's width would end at the leading edge. A change of width shows
    // one frame late, so that frame asks for another.
    let key = ui.next_auto_id().with("rtl_island_width");
    let known = ui.data(|d| d.get_temp::<f32>(key));
    let room = ui.available_rect_before_wrap();
    let available = if room.width().is_finite() { room.width().max(0.0) } else { 0.0 };
    let width = known.unwrap_or(available);
    let left = room.max.x - width;
    if !left.is_finite() {
        return add(ui);
    }
    let max_rect = Rect::from_min_max(pos2(left, room.min.y), pos2(left + available.max(width), room.max.y));
    let shown = ui.scope_builder(UiBuilder::new().max_rect(max_rect).layout(Layout::top_down(Align::Min)), add);
    let now = shown.response.rect.width();
    if now.is_finite() && known.is_none_or(|w| (w - now).abs() > 0.5) {
        ui.data_mut(|d| d.insert_temp(key, now));
        ui.ctx().request_repaint();
    }
    shown.inner
}

/// `grid.show(ui, add)`. egui's `Grid` cannot be laid out from the right (it places cells from a
/// left edge that a right-to-left parent does not have, and says so with a debug assertion), so
/// inside a right-to-left area it is drawn in an [`island`]. Its columns keep their order.
pub fn grid<R>(ui: &mut Ui, grid: egui::Grid, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    island(ui, |ui| grid.show(ui, add))
}

/// A form of rows and cells. Where the interface is mirrored (`flows`), a row's cells run from the
/// right edge, the first cell at the right, and every column is as wide as its widest cell, so the
/// labels sit at the reading start with their fields after them, to their left. Elsewhere it is
/// exactly an `egui::Grid` with the same id, columns and spacing. A `Grid` itself cannot be laid
/// out from the right, which is why [`grid`] keeps its columns in order.
pub struct Table {
    id: &'static str,
    columns: usize,
    spacing: Option<egui::Vec2>,
    striped: bool,
    top: bool,
    min_row_height: Option<f32>,
}

impl Table {
    pub fn new(id: &'static str) -> Self {
        Self { id, columns: 2, spacing: None, striped: false, top: false, min_row_height: None }
    }

    /// The number of columns; 0 leaves an `egui::Grid` without a fixed count.
    pub fn columns(mut self, columns: usize) -> Self {
        self.columns = columns;
        self
    }

    /// Cells align to the top of their row (they are centred in it otherwise, as a mirrored form
    /// wants for a label and its field; tall cells such as a column of widgets want the top).
    pub fn top(mut self) -> Self {
        self.top = true;
        self
    }

    pub fn min_row_height(mut self, height: f32) -> Self {
        self.min_row_height = Some(height);
        self
    }

    pub fn spacing(mut self, spacing: egui::Vec2) -> Self {
        self.spacing = Some(spacing);
        self
    }

    /// Shade every other row (only where the table is an `egui::Grid`: a mirrored table has no stripes).
    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }

    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Rows) -> R) -> R {
        if !flows(ui) {
            let spec = egui::Grid::new(self.id);
            let spec = if self.columns > 0 { spec.num_columns(self.columns) } else { spec };
            let spec = if self.striped { spec.striped(true) } else { spec };
            let spec = match self.min_row_height {
                Some(height) => spec.min_row_height(height),
                None => spec,
            };
            let spec = match self.spacing {
                Some(spacing) => spec.spacing(spacing),
                None => spec,
            };
            return grid(ui, spec, |ui| add(&mut Rows { ui, mirror: None })).inner;
        }
        let key = ui.scope_id().with(("rtl_table_widths", self.id));
        let prev = ui.data(|d| d.get_temp::<Vec<f32>>(key)).unwrap_or_default();
        let spacing = self.spacing.unwrap_or(ui.spacing().item_spacing);
        let gap = spacing.x;
        // Rows stack top to bottom from the reading edge, whatever way the area around runs (a ribbon
        // group lays its content out in a row).
        let shown = ui.scope_builder(UiBuilder::new().layout(Layout::top_down(Align::Max)), |ui| {
            ui.spacing_mut().item_spacing.y = spacing.y;
            let mirror = Mirror { prev: prev.clone(), cur: Vec::new(), gap, top: self.top, min_row: self.min_row_height };
            let mut rows = Rows { ui, mirror: Some(mirror) };
            let out = add(&mut rows);
            (out, rows.mirror.map(|m| m.cur).unwrap_or_default())
        });
        let (out, cur) = shown.inner;
        let same = prev.len() == cur.len() && prev.iter().zip(&cur).all(|(a, b)| (a - b).abs() <= 0.5);
        if !same {
            ui.data_mut(|d| d.insert_temp(key, cur));
            ui.ctx().request_repaint();
        }
        out
    }
}

struct Mirror {
    /// The width each column had last frame.
    prev: Vec<f32>,
    /// The width each column has now (the widest of its cells).
    cur: Vec<f32>,
    gap: f32,
    top: bool,
    min_row: Option<f32>,
}

/// The rows of a [`Table`].
pub struct Rows<'a> {
    ui: &'a mut Ui,
    mirror: Option<Mirror>,
}

impl Rows<'_> {
    /// One row: add its cells in reading order with [`Cells::cell`].
    pub fn row<R>(&mut self, add: impl FnOnce(&mut Cells) -> R) -> R {
        match &mut self.mirror {
            None => {
                let out = add(&mut Cells { ui: self.ui, mirror: None, col: 0, pending: 0.0 });
                self.ui.end_row();
                out
            }
            Some(mirror) => {
                let (top, min_row) = (mirror.top, mirror.min_row);
                let body = |ui: &mut Ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if let Some(height) = min_row {
                        ui.set_min_height(height);
                    }
                    add(&mut Cells { ui, mirror: Some(mirror), col: 0, pending: 0.0 })
                };
                if top { self.ui.horizontal_top(body).inner } else { self.ui.horizontal(body).inner }
            }
        }
    }
}

/// The cells of one row of a [`Table`].
pub struct Cells<'a> {
    ui: &'a mut Ui,
    mirror: Option<&'a mut Mirror>,
    col: usize,
    /// The space still to leave before the next cell: what its column is wider than the last cell.
    pending: f32,
}

impl Cells<'_> {
    /// The next cell of the row (its widgets, in the order they read).
    pub fn cell<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> R {
        let Some(mirror) = self.mirror.as_deref_mut() else {
            return add(self.ui);
        };
        if self.pending > 0.0 {
            self.ui.add_space(self.pending);
        }
        let shown = self.ui.scope(add);
        let width = shown.response.rect.width();
        let width = if width.is_finite() { width.max(0.0) } else { 0.0 };
        if mirror.cur.len() <= self.col {
            mirror.cur.resize(self.col + 1, 0.0);
        }
        if let Some(w) = mirror.cur.get_mut(self.col) {
            *w = w.max(width);
        }
        let column = mirror.prev.get(self.col).copied().unwrap_or(0.0);
        self.pending = (column - width).max(0.0) + mirror.gap;
        self.col += 1;
        shown.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, fixture_lang, set_current};

    /// Where a two-cell row puts its label and its field, as the centres of their rectangles.
    fn row_centres() -> (f32, f32) {
        let ctx = egui::Context::default();
        let mut centres = (0.0, 0.0);
        // The second frame has the column widths the first measured.
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                root(ui, |ui| {
                    Table::new("t").columns(2).show(ui, |t| {
                        t.row(|r| {
                            let label = r.cell(|ui| ui.label("Label:"));
                            let field = r.cell(|ui| ui.add(egui::TextEdit::singleline(&mut String::new()).desired_width(80.0)));
                            centres = (label.rect.center().x, field.rect.center().x);
                        });
                    });
                });
            });
            out.textures_delta.clear();
        }
        centres
    }

    /// `text` laid out as a 300-point wide field's text in the current interface.
    fn field(text: &str) -> Arc<Galley> {
        let ctx = egui::Context::default();
        let mut galley = None;
        ctx.run_ui(egui::RawInput::default(), |ui| galley = Some(field_galley(ui, text, 300.0))).drop_without_applying_deltas();
        galley.unwrap()
    }

    /// The left and right screen edges of the letters of `row`.
    fn ink(galley: &Galley, row: usize) -> (f32, f32) {
        let placed = &galley.rows[row];
        let letters = placed.glyphs.iter().filter(|g| !g.chr.is_whitespace());
        letters.fold((f32::MAX, f32::MIN), |(l, r), g| (l.min(placed.pos.x + g.pos.x), r.max(placed.pos.x + g.pos.x + g.advance_width)))
    }

    /// A field in a right-to-left interface lays out every paragraph right to left against its right edge, a
    /// line that starts with an English word included; elsewhere each paragraph starts at its own side. (Where an
    /// empty field's caret is drawn depends on the field around it: the AI pane's tests check it on the drawn
    /// pane.)
    #[test]
    fn a_field_starts_each_paragraph_at_its_side() {
        set_current(fixture_lang("he", "\tHome\tבית\n"));
        let (short, mixed) = (field("שלום"), field("Word היא"));
        set_current(Lang::EN);
        assert!(300.0 - ink(&short, 0).1 < 1.0, "a short line at the right edge: {:?}", ink(&short, 0));
        let x = |c: char| mixed.rows[0].glyphs.iter().find(|g| g.chr == c).map_or(f32::NAN, |g| mixed.rows[0].pos.x + g.pos.x);
        assert!(x('W') > x('ה'), "right to left, the English word first: {} {}", x('W'), x('ה'));
        let both = field("Hello\nשלום");
        assert!(ink(&both, 0).0 < 1.0 && 300.0 - ink(&both, 1).1 < 1.0, "{:?} {:?}", ink(&both, 0), ink(&both, 1));
    }

    /// AGENTS.md rule 21: a menu row is named for a screen reader by its label first, then its
    /// shortcut, in a mirrored interface too, where the shortcut is drawn first (it was named
    /// `Ctrl+B Bold` there).
    #[test]
    fn a_menu_item_is_named_label_first_in_either_direction() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::{NodeT as _, Queryable};
        let hebrew = fixture_lang("he", "\tHome\tבית\n");
        for mirrored in [false, true] {
            let drawn = std::rc::Rc::new(std::cell::Cell::new(false));
            let seen = drawn.clone();
            let h = egui_kittest::Harness::new_ui(move |ui| {
                set_current(if mirrored { hebrew } else { Lang::EN });
                root(ui, |ui| {
                    seen.set(flows(ui));
                    ui.add(menu_item(ui, "Bold", "Ctrl+B", 120.0));
                });
                set_current(Lang::EN);
            });
            assert_eq!(drawn.get(), mirrored, "drawn mirrored: {mirrored}");
            let name = h.get_by_role(Role::Button).accesskit_node().label().unwrap_or_default();
            assert_eq!(name, "Bold Ctrl+B", "mirrored {mirrored}: the label first");
        }
    }

    /// Where the text `text` was painted in `shapes`.
    fn painted(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<Rect> {
        fn find(shape: &egui::Shape, text: &str) -> Option<Rect> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == text => Some(Rect::from_min_size(t.pos, t.galley.size())),
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, text)),
                _ => None,
            }
        }
        shapes.iter().find_map(|c| find(&c.shape, text))
    }

    /// A drop-down list's rows start from the reading side: in a right-to-left interface a row's
    /// text is at the right of the list (craft_ui's menu put it at the left), left to right at the
    /// left.
    #[test]
    fn a_drop_down_list_starts_its_rows_at_the_reading_side() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let hebrew = fixture_lang("he", "\tHome\tבית\n");
        for mirrored in [false, true] {
            let mut h = egui_kittest::Harness::new_ui(move |ui| {
                set_current(if mirrored { hebrew } else { Lang::EN });
                sync(ui.ctx());
                root(ui, |ui| {
                    let label = ui.label("Size");
                    let combo = ComboBox::from_id_salt("rows").selected_text("One").width(240.0).show_ui(ui, |ui| {
                        let _ = ui.selectable_label(true, "One");
                        let _ = ui.selectable_label(false, "Second");
                    });
                    combo.response.labelled_by(label.id);
                });
                set_current(Lang::EN);
            });
            h.get_by_role(Role::ComboBox).click();
            h.run();
            let row = h.get_by_label("Second").rect();
            let text = painted(&h.output().shapes, "Second").expect("the row's text is painted");
            let (lead, trail) = (text.min.x - row.min.x, row.max.x - text.max.x);
            assert!(row.width() > text.width() + 40.0, "a row as wide as the list: {row:?} {text:?}");
            if mirrored {
                assert!(trail < lead, "mirrored: the text at the row's right: {row:?} {text:?}");
            } else {
                assert!(lead < trail, "left to right: the text at the row's left: {row:?} {text:?}");
            }
        }
    }

    /// A form reads from the reading start: the label of a row first, its field after it, so in a
    /// right-to-left interface the label is at the right and the field to its left.
    #[test]
    fn a_form_row_puts_the_label_at_the_reading_start() {
        set_current(Lang::EN);
        let (label, field) = row_centres();
        assert!(label < field, "left to right: label {label}, field {field}");
        set_current(fixture_lang("ar", "\tHome\tالرئيسية\n"));
        let (label, field) = row_centres();
        set_current(Lang::EN);
        assert!(label > field, "right to left: label {label}, field {field}");
    }
}
