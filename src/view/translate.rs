use crate::data::settings::TranslateProvider;
use crate::services::translate::{LANGUAGES, language_name};
use crate::view::*;

#[derive(Default)]
pub struct TranslateUi {
    pub input: TextEdit,
    pub result: String,
    pub detected: String,
    pub status: String,
    pub busy: bool,
    pub serial: u64,
    pub res_scroll: f32,
    pub hover: Option<TranslateHit>,
    pub key_saved: bool,
    pub used: Option<TranslateProvider>,
    pub last_source: String,
    pub pasted: bool,
    pub clip_seq: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TranslateHit {
    Provider,
    Lang(usize),
    Action(usize),
    Result,
}

pub const TRANSLATE_ACTIONS: [&str; 5] =
    ["Translate", "Paste clipboard", "Copy result", "Auto translate", "Set key from clipboard"];

pub const KEY_ACTION: usize = 4;

const SIDE_W: f32 = 236.0;
const SIDE_GAP: f32 = 14.0;
const ROW_H: f32 = 26.0;
const GAP: f32 = 4.0;
const LANG_H: f32 = 22.0;
const LANG_COLS: usize = 3;
const LANG_GAP: f32 = 3.0;

fn content(pill: Rect, scale: f32) -> Rect {
    let (x, y, w, h) = pill;
    (x + SIDE_PAD * scale, y + LIST_TOP * scale, x + w - SIDE_PAD * scale, y + h - LIST_BOTTOM_PAD * scale)
}

fn side_left(pill: Rect, scale: f32) -> f32 {
    content(pill, scale).2 - SIDE_W * scale
}

pub fn translate_frames(pill: Rect, scale: f32) -> (Rect, Rect, Rect) {
    let c = content(pill, scale);
    let right = c.2 - (SIDE_W + SIDE_GAP) * scale;
    let mid = (c.1 + c.3) / 2.0;
    let gap = 4.0 * scale;
    let source = (c.0, c.1, right, mid - gap);
    let inner = (source.0 + 14.0 * scale, source.1 + 8.0 * scale, source.2 - 14.0 * scale, source.3 - 8.0 * scale);
    (source, inner, (c.0, mid + gap, right, c.3))
}

pub fn translate_field(pill: Rect, scale: f32) -> (Rect, Rect) {
    let (frame, inner, _) = translate_frames(pill, scale);
    (frame, inner)
}

fn provider_rect(pill: Rect, scale: f32) -> Rect {
    let c = content(pill, scale);
    let left = side_left(pill, scale);
    (left, c.1, c.2, c.1 + ROW_H * scale)
}

fn detected_rect(pill: Rect, scale: f32) -> Rect {
    let p = provider_rect(pill, scale);
    (p.0, p.3 + GAP * scale, p.2, p.3 + (GAP + 16.0) * scale)
}

fn lang_rect(pill: Rect, scale: f32, i: usize) -> Rect {
    let d = detected_rect(pill, scale);
    let top = d.3 + GAP * scale;
    let gap = LANG_GAP * scale;
    let w = (d.2 - d.0 - (LANG_COLS - 1) as f32 * gap) / LANG_COLS as f32;
    let l = d.0 + (i % LANG_COLS) as f32 * (w + gap);
    let t = top + (i / LANG_COLS) as f32 * (LANG_H * scale + gap);
    (l, t, l + w, t + LANG_H * scale)
}

fn action_rect(pill: Rect, scale: f32, i: usize) -> Rect {
    let c = content(pill, scale);
    let left = side_left(pill, scale);
    let gap = GAP * scale;
    let rows = 3.0;
    let top0 = c.3 - rows * ROW_H * scale - (rows - 1.0) * gap;
    let row = |r: usize| top0 + r as f32 * (ROW_H * scale + gap);
    if i == KEY_ACTION {
        return (left, row(2), c.2, row(2) + ROW_H * scale);
    }
    let half = (c.2 - left - gap) / 2.0;
    let l = left + (i % 2) as f32 * (half + gap);
    (l, row(i / 2), l + half, row(i / 2) + ROW_H * scale)
}

fn shows_action(i: usize, cfg: &Settings) -> bool {
    i != KEY_ACTION || cfg.translate.provider.needs_key()
}

pub fn hit_translate(pill: Rect, scale: f32, px: f32, py: f32, cfg: &Settings) -> Option<TranslateHit> {
    if inside(provider_rect(pill, scale), px, py) {
        return Some(TranslateHit::Provider);
    }
    if let Some(i) = (0..LANGUAGES.len()).find(|&i| inside(lang_rect(pill, scale, i), px, py)) {
        return Some(TranslateHit::Lang(i));
    }
    if let Some(i) =
        (0..TRANSLATE_ACTIONS.len()).find(|&i| shows_action(i, cfg) && inside(action_rect(pill, scale, i), px, py))
    {
        return Some(TranslateHit::Action(i));
    }
    inside(translate_frames(pill, scale).2, px, py).then_some(TranslateHit::Result)
}

pub fn over_translate_result(pill: Rect, scale: f32, px: f32, py: f32) -> bool {
    inside(translate_frames(pill, scale).2, px, py)
}

pub(in crate::view) fn paint_translate(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let (frame, inner, result) = translate_frames(pill, scale);
    paint_editor(
        p,
        frame,
        inner,
        &ui.translate.input,
        ui.editing,
        ui.caret_on,
        "Type or paste text to translate",
        NOTE_FONT,
        scale,
    )?;

    p.fill_round_rect(result, 12.0 * scale, rgba(1.0, 1.0, 1.0, 0.08))?;
    let text_area = (result.0 + 14.0 * scale, result.1 + 8.0 * scale, result.2 - 14.0 * scale, result.3 - 26.0 * scale);
    if ui.translate.result.is_empty() {
        let hint = if ui.translate.busy { "Translating…" } else { "The translation appears here" };
        p.text(
            hint,
            (text_area.0, text_area.1, text_area.2, text_area.1 + 24.0 * scale),
            NOTE_FONT * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.4),
        )?;
    } else {
        let max =
            p.text_scrolled(&ui.translate.result, text_area, NOTE_FONT * scale, ui.translate.res_scroll, WHITE)?;
        if max > 0.0 {
            let track_h = text_area.3 - text_area.1;
            let bar_h = (track_h * track_h / (track_h + max)).max(12.0 * scale);
            let top = text_area.1 + (track_h - bar_h) * ui.translate.res_scroll.clamp(0.0, max) / max;
            p.fill_round_rect(
                (result.2 - 7.0 * scale, top, result.2 - 5.0 * scale, top + bar_h),
                scale,
                rgba(1.0, 1.0, 1.0, 0.3),
            )?;
        }
    }
    if !ui.translate.status.is_empty() {
        p.text(
            &ui.translate.status,
            (result.0 + 14.0 * scale, result.3 - 22.0 * scale, result.2 - 14.0 * scale, result.3 - 4.0 * scale),
            11.5 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            acc(1.0),
        )?;
    }

    let provider = ui.cfg.translate.provider;
    let hovered = |hit| ui.translate.hover == Some(hit);
    let r = provider_rect(pill, scale);
    p.fill_round_rect(r, 9.0 * scale, acc(if hovered(TranslateHit::Provider) { 0.5 } else { 0.32 }))?;
    p.text(provider.name(), r, 12.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)?;

    if !ui.translate.detected.is_empty() {
        let name = language_name(&ui.translate.detected);
        let label = if name.is_empty() { ui.translate.detected.clone() } else { name.to_string() };
        p.text(
            &format!("{}: {label}", crate::ui::i18n::tr("Detected")),
            detected_rect(pill, scale),
            11.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.6),
        )?;
    }

    for (i, (code, name)) in LANGUAGES.iter().enumerate() {
        let r = lang_rect(pill, scale, i);
        let on = *code == ui.cfg.translate.target;
        let alpha = match (on, hovered(TranslateHit::Lang(i))) {
            (true, _) => 0.55,
            (false, true) => 0.24,
            (false, false) => 0.1,
        };
        p.fill_round_rect(r, 8.0 * scale, acc(alpha))?;
        p.text(name, r, 10.5 * scale, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)?;
    }

    for (i, label) in TRANSLATE_ACTIONS.iter().enumerate() {
        if !shows_action(i, &ui.cfg) {
            continue;
        }
        let r = action_rect(pill, scale, i);
        let on = i == 3 && ui.cfg.translate.auto;
        let alpha = match (on, hovered(TranslateHit::Action(i))) {
            (true, true) => 0.75,
            (true, false) => 0.6,
            (false, true) => 0.34,
            (false, false) => 0.2,
        };
        p.fill_round_rect(r, 9.0 * scale, acc(alpha))?;
        let label = if i == KEY_ACTION && ui.translate.key_saved { "Replace key from clipboard" } else { label };
        p.text(label, r, 12.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)?;
    }
    Ok(())
}
