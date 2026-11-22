use crate::view::*;

pub struct MacroRow {
    pub stem: String,
    pub title: String,
    pub hotkey: String,
    pub method: &'static str,
    pub broken: bool,
}

#[derive(Default)]
pub struct MacrosUi {
    pub rows: Vec<MacroRow>,
    pub sel: Option<usize>,
    pub scroll: usize,
    pub editor: TextEdit,
    pub dirty: bool,
    pub status: String,
    pub hover: Option<MacrosHit>,
    pub recording: bool,
    pub running: Option<String>,
    pub capture: bool,
    pub armed_delete: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MacrosHit {
    Row(usize),
    Action(usize),
}

pub const MACRO_ACTIONS: [&str; 10] =
    ["Run", "Save", "Record", "Hotkey", "Method", "New", "Delete", "Folder", "Open", "Guide"];

pub const ACT_RUN: usize = 0;
pub const ACT_SAVE: usize = 1;
pub const ACT_RECORD: usize = 2;
pub const ACT_HOTKEY: usize = 3;
pub const ACT_METHOD: usize = 4;
pub const ACT_NEW: usize = 5;
pub const ACT_DELETE: usize = 6;
pub const ACT_FOLDER: usize = 7;
pub const ACT_OPEN: usize = 8;
pub const ACT_GUIDE: usize = 9;

pub const MACRO_FONT: f32 = 12.5;

const LIST_W: f32 = 200.0;
const PANE_GAP: f32 = 12.0;
const ROW_H: f32 = 28.0;
const BUTTON_H: f32 = 26.0;
const GAP: f32 = 4.0;

fn content(pill: Rect, scale: f32) -> Rect {
    let (x, y, w, h) = pill;
    (x + SIDE_PAD * scale, y + LIST_TOP * scale, x + w - SIDE_PAD * scale, y + h - LIST_BOTTOM_PAD * scale)
}

fn list_rect(pill: Rect, scale: f32) -> Rect {
    let c = content(pill, scale);
    (c.0, c.1, c.0 + LIST_W * scale, c.3)
}

fn pane(pill: Rect, scale: f32) -> Rect {
    let c = content(pill, scale);
    (c.0 + (LIST_W + PANE_GAP) * scale, c.1, c.2, c.3)
}

pub fn macro_rows_fit(pill: Rect, scale: f32) -> usize {
    let l = list_rect(pill, scale);
    (((l.3 - l.1) / (ROW_H * scale)).floor() as usize).max(1)
}

fn row_rect(pill: Rect, scale: f32, slot: usize) -> Rect {
    let l = list_rect(pill, scale);
    let top = l.1 + slot as f32 * ROW_H * scale;
    (l.0, top, l.2, top + (ROW_H - 2.0) * scale)
}

fn buttons_top(pill: Rect, scale: f32) -> f32 {
    content(pill, scale).3 - (2.0 * BUTTON_H + GAP) * scale
}

fn action_rect(pill: Rect, scale: f32, i: usize) -> Rect {
    let p = pane(pill, scale);
    let gap = GAP * scale;
    let w = (p.2 - p.0 - 4.0 * gap) / 5.0;
    let l = p.0 + (i % 5) as f32 * (w + gap);
    let t = buttons_top(pill, scale) + (i / 5) as f32 * (BUTTON_H * scale + gap);
    (l, t, l + w, t + BUTTON_H * scale)
}

pub fn macro_field(pill: Rect, scale: f32) -> (Rect, Rect) {
    let p = pane(pill, scale);
    let frame = (p.0, p.1 + 24.0 * scale, p.2, buttons_top(pill, scale) - 24.0 * scale);
    let inner = (frame.0 + 12.0 * scale, frame.1 + 8.0 * scale, frame.2 - 12.0 * scale, frame.3 - 8.0 * scale);
    (frame, inner)
}

pub fn hit_macros(pill: Rect, scale: f32, px: f32, py: f32, ui: &MacrosUi) -> Option<MacrosHit> {
    if let Some(i) = (0..MACRO_ACTIONS.len()).find(|&i| inside(action_rect(pill, scale, i), px, py)) {
        return Some(MacrosHit::Action(i));
    }
    let fit = macro_rows_fit(pill, scale);
    (0..fit)
        .find(|&slot| inside(row_rect(pill, scale, slot), px, py) && ui.scroll + slot < ui.rows.len())
        .map(|slot| MacrosHit::Row(ui.scroll + slot))
}

pub fn over_macro_list(pill: Rect, scale: f32, px: f32, py: f32) -> bool {
    inside(list_rect(pill, scale), px, py)
}

pub(in crate::view) fn paint_macros(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let m = &ui.macros;
    let list = list_rect(pill, scale);
    p.fill_round_rect(list, 12.0 * scale, rgba(1.0, 1.0, 1.0, 0.04))?;
    if m.rows.is_empty() {
        p.text_wrapped(
            "No macros yet. Press New or Record.",
            (list.0 + 12.0 * scale, list.1, list.2 - 12.0 * scale, list.3),
            12.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            rgba(1.0, 1.0, 1.0, 0.5),
        )?;
    }
    for slot in 0..macro_rows_fit(pill, scale) {
        let index = m.scroll + slot;
        let Some(row) = m.rows.get(index) else { break };
        let r = row_rect(pill, scale, slot);
        let selected = m.sel == Some(index);
        let hovered = m.hover == Some(MacrosHit::Row(index));
        if selected || hovered {
            p.fill_round_rect(r, 9.0 * scale, acc(if selected { 0.4 } else { 0.14 }))?;
        }
        let running = m.running.as_deref() == Some(row.stem.as_str());
        let title = format!(
            "{}{}",
            if row.broken {
                "! "
            } else if running {
                "● "
            } else {
                ""
            },
            row.title
        );
        let hot_w = if row.hotkey.is_empty() { 0.0 } else { 86.0 * scale };
        p.text(
            &title,
            (r.0 + 10.0 * scale, r.1, r.2 - hot_w - 6.0 * scale, r.3),
            12.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            if row.broken { rgba(1.0, 0.55, 0.5, 1.0) } else { WHITE },
        )?;
        if hot_w > 0.0 {
            p.text(
                &row.hotkey,
                (r.2 - hot_w - 4.0 * scale, r.1, r.2 - 8.0 * scale, r.3),
                10.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                rgba(1.0, 1.0, 1.0, 0.5),
            )?;
        }
    }
    if m.rows.len() > macro_rows_fit(pill, scale) {
        let rows = macro_rows_fit(pill, scale);
        let max = m.rows.len() - rows;
        let track = list.3 - list.1;
        let bar = track * rows as f32 / m.rows.len() as f32;
        let top = list.1 + (track - bar) * m.scroll.min(max) as f32 / max as f32;
        p.fill_round_rect(
            (list.2 - 4.0 * scale, top, list.2 - 2.0 * scale, top + bar),
            scale,
            rgba(1.0, 1.0, 1.0, 0.3),
        )?;
    }

    let pane = pane(pill, scale);
    if let Some(row) = m.sel.and_then(|i| m.rows.get(i)) {
        let hotkey =
            if row.hotkey.is_empty() { crate::ui::i18n::tr("No hotkey").into_owned() } else { row.hotkey.clone() };
        p.text(
            &format!("{}  ·  {}  ·  {}", row.title, hotkey, crate::ui::i18n::tr(row.method)),
            (pane.0, pane.1, pane.2, pane.1 + 20.0 * scale),
            12.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        )?;
    }
    let (frame, inner) = macro_field(pill, scale);
    paint_editor(
        p,
        frame,
        inner,
        &m.editor,
        ui.editing,
        ui.caret_on,
        "Select a macro or press New",
        MACRO_FONT,
        scale,
    )?;
    if !m.status.is_empty() {
        let top = buttons_top(pill, scale);
        p.text(
            &m.status,
            (pane.0, top - 22.0 * scale, pane.2, top - 4.0 * scale),
            11.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            acc(1.0),
        )?;
    }

    let running_here = m.sel.and_then(|i| m.rows.get(i)).is_some_and(|r| m.running.as_deref() == Some(r.stem.as_str()));
    for i in 0..MACRO_ACTIONS.len() {
        let r = action_rect(pill, scale, i);
        let hovered = m.hover == Some(MacrosHit::Action(i));
        let on = match i {
            ACT_RUN => running_here,
            ACT_RECORD => m.recording,
            ACT_HOTKEY => m.capture,
            ACT_DELETE => m.armed_delete,
            ACT_SAVE => m.dirty,
            _ => false,
        };
        let alpha = match (on, hovered) {
            (true, true) => 0.75,
            (true, false) => 0.6,
            (false, true) => 0.34,
            (false, false) => 0.2,
        };
        let label = match i {
            ACT_RUN if running_here => "Stop",
            ACT_RECORD if m.recording => "Stop recording",
            ACT_HOTKEY if m.capture => "Press keys…",
            ACT_DELETE if m.armed_delete => "Confirm",
            other => MACRO_ACTIONS[other],
        };
        p.fill_round_rect(r, 9.0 * scale, acc(alpha))?;
        p.text(label, r, 11.5 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)?;
    }
    Ok(())
}
