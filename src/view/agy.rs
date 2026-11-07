use crate::view::*;

#[derive(Default)]
pub struct AgyUi {
    pub chats: Vec<crate::services::agy::Chat>,
    pub sel: Option<String>,
    pub scroll: usize,
    pub input: TextEdit,
    pub jobs: std::collections::HashMap<String, crate::services::agy::Job>,
    pub open: bool,
    pub msgs: Vec<crate::services::agy::Msg>,
    pub hist: f32,
    pub cache: std::cell::RefCell<(String, i32, usize, Vec<f32>)>,
    pub max: std::cell::Cell<f32>,
    pub input_h: std::cell::Cell<f32>,
    pub quota: Option<i64>,
}

pub(in crate::view) const AGY_ROW_H: f32 = 42.0;

pub(in crate::view) const AGY_LIST_TOP: f32 = 88.0;

pub(in crate::view) fn agy_side(pill: Rect, scale: f32) -> (f32, f32) {
    (pill.0 + (SIDE_PAD - 4.0) * scale, pill.0 + pill.2 - (SIDE_PAD - 4.0) * scale)
}

pub fn agy_rows() -> usize {
    (((372.0 - SUB_H - AGY_LIST_TOP) + 4.0) / (AGY_ROW_H + 4.0)).floor() as usize
}

pub(in crate::view) fn agy_row(pill: Rect, scale: f32, i: usize) -> Rect {
    let (l, r) = agy_side(pill, scale);
    let top = pill.1 + (AGY_LIST_TOP + i as f32 * (AGY_ROW_H + 4.0)) * scale;
    (l, top, r, top + AGY_ROW_H * scale)
}

pub(in crate::view) fn agy_back(pill: Rect, scale: f32) -> Rect {
    let (l, _) = agy_side(pill, scale);
    (l, pill.1 + 56.0 * scale, l + 32.0 * scale, pill.1 + 88.0 * scale)
}

pub fn agy_field_height(scale: f32, measured: f32) -> f32 {
    if measured <= 0.0 { 36.0 * scale } else { measured.clamp(36.0 * scale, 112.0 * scale) }
}

pub(in crate::view) fn agy_view(pill: Rect, scale: f32, input_h: f32) -> Rect {
    let (l, r) = agy_side(pill, scale);
    (l, pill.1 + 94.0 * scale, r, pill.1 + pill.3 - 16.0 * scale - agy_field_height(scale, input_h) - 8.0 * scale)
}

pub fn agy_input(pill: Rect, scale: f32, input_h: f32) -> (Rect, Rect) {
    let (l, r) = agy_side(pill, scale);
    let h = agy_field_height(scale, input_h);
    let frame = (l, pill.1 + pill.3 - 16.0 * scale - h, r - 90.0 * scale, pill.1 + pill.3 - 16.0 * scale);
    (frame, (frame.0 + 12.0 * scale, frame.1 + 8.0 * scale, frame.2 - 10.0 * scale, frame.3 - 8.0 * scale))
}

pub(in crate::view) fn agy_send_button(pill: Rect, scale: f32) -> Rect {
    let (_, r) = agy_side(pill, scale);
    (r - 82.0 * scale, pill.1 + pill.3 - 52.0 * scale, r, pill.1 + pill.3 - 16.0 * scale)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AgyHit {
    Row(usize),
    Back,
    History,
    Input,
    Send,
}

#[allow(clippy::too_many_arguments)]
pub fn hit_agy(
    pill: Rect,
    scale: f32,
    px: f32,
    py: f32,
    count: usize,
    scroll: usize,
    open: bool,
    input_h: f32,
) -> Option<AgyHit> {
    if !open {
        let shown = count.saturating_sub(scroll).min(agy_rows());
        return (0..shown).find(|&i| inside(agy_row(pill, scale, i), px, py)).map(AgyHit::Row);
    }
    if inside(agy_back(pill, scale), px, py) {
        return Some(AgyHit::Back);
    }
    if inside(agy_input(pill, scale, input_h).0, px, py) {
        return Some(AgyHit::Input);
    }
    if inside(agy_send_button(pill, scale), px, py) {
        return Some(AgyHit::Send);
    }
    inside(agy_view(pill, scale, input_h), px, py).then_some(AgyHit::History)
}

pub(in crate::view) fn time_ago(modified: i64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let secs = (now - modified).max(0);
    let ru = crate::ui::i18n::is_ru();
    match secs {
        0..=59 => (if ru { "сейчас" } else { "now" }).to_string(),
        60..=3599 => format!("{}{}", secs / 60, if ru { " мин" } else { "m" }),
        3600..=86399 => format!("{}{}", secs / 3600, if ru { " ч" } else { "h" }),
        _ => format!("{}{}", secs / 86400, if ru { " дн" } else { "d" }),
    }
}

pub(in crate::view) fn agy_limit_text(ui: &UiState) -> (String, bool) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let tr = |s: &str| crate::ui::i18n::tr(s).into_owned();
    let tokens = crate::services::agy::session_tokens();
    let spent = if tokens > 0 { format!(" · {}k {}", (tokens + 500) / 1000, tr("tokens")) } else { String::new() };
    match ui.agy.quota {
        Some(reset) if reset > now => {
            let left = reset - now;
            let when = if left >= 3600 {
                format!("{}h {}m", left / 3600, left % 3600 / 60)
            } else {
                format!("{}m", (left / 60).max(1))
            };
            (format!("{}: {} {when}{spent}", tr("limit reached"), tr("resets in")), true)
        }
        _ => (format!("{}{spent}", tr("limit: ok")), false),
    }
}

pub(in crate::view) fn agy_block(kind: crate::services::agy::Kind) -> (f32, f32, f32, f32, f32) {
    use crate::services::agy::Kind;
    match kind {
        Kind::User => (0.0, 20.0, 13.0, 8.0, 8.0),
        Kind::Assistant => (2.0, 6.0, 13.0, 0.0, 10.0),
        Kind::Thinking => (14.0, 24.0, 11.5, 0.0, 6.0),
        Kind::Tool => (22.0, 30.0, 11.5, 0.0, 5.0),
        Kind::Error => (4.0, 14.0, 12.0, 0.0, 8.0),
    }
}

pub(in crate::view) fn agy_layout(
    p: &Painter,
    m: &crate::services::agy::Msg,
    width: f32,
    scale: f32,
) -> Result<windows::Win32::Graphics::DirectWrite::IDWriteTextLayout> {
    let (_, trim, size, _, _) = agy_block(m.kind);
    p.md_layout(&m.doc, (width - trim * scale).max(40.0), size * scale, m.kind == crate::services::agy::Kind::Thinking)
}

pub struct MsgGeom {
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub layout: windows::Win32::Graphics::DirectWrite::IDWriteTextLayout,
}

pub type TextRange = ((usize, u32), (usize, u32));

pub(in crate::view) struct History<'a> {
    pub msgs: &'a [crate::services::agy::Msg],
    pub cache: &'a std::cell::RefCell<(String, i32, usize, Vec<f32>)>,
    pub id: String,
    pub hist: f32,
    pub max: &'a std::cell::Cell<f32>,
    pub input_h: f32,
    pub reserve: f32,
    pub geom: Option<&'a std::cell::RefCell<Vec<MsgGeom>>>,
    pub sel: Option<TextRange>,
}

fn selected_part(sel: Option<TextRange>, index: usize, len: u32) -> Option<(u32, u32)> {
    let (lo, hi) = sel?;
    if index < lo.0 || index > hi.0 {
        return None;
    }
    let start = if index == lo.0 { lo.1 } else { 0 };
    let end = if index == hi.0 { hi.1.min(len) } else { len };
    (end > start).then_some((start, end - start))
}

pub(in crate::view) fn paint_history(p: &Painter, pill: Rect, scale: f32, h: &History, running: bool) -> Result<()> {
    let mut view = agy_view(pill, scale, h.input_h);
    view.3 -= h.reserve;
    paint_history_in(p, view, scale, h, running)
}

pub(in crate::view) fn paint_history_in(p: &Painter, view: Rect, scale: f32, h: &History, running: bool) -> Result<()> {
    use crate::services::agy::Kind;
    let width = view.2 - view.0 - 10.0 * scale;
    if let Some(geom) = h.geom {
        geom.borrow_mut().clear();
    }
    let running_h = if running { 24.0 * scale } else { 0.0 };
    let key = (
        format!("{:?}:{}:{}", crate::ui::fonts::selected(), crate::ui::fonts::revision(), h.id),
        width.round() as i32,
        h.msgs.len(),
    );
    let mut cache = h.cache.borrow_mut();
    if (cache.0.clone(), cache.1, cache.2) != key {
        let heights = h
            .msgs
            .iter()
            .map(|m| {
                let (_, _, _, pad, gap) = agy_block(m.kind);
                let text = agy_layout(p, m, width, scale).map_or(20.0 * scale, |l| p.layout_height(&l));
                text + (2.0 * pad + gap) * scale
            })
            .collect();
        *cache = (key.0, key.1, key.2, heights);
    }
    let total: f32 = cache.3.iter().sum::<f32>() + running_h;
    let view_h = view.3 - view.1;
    let max = (total - view_h).max(0.0);
    h.max.set(max);
    let offset = h.hist.clamp(0.0, max);
    p.push_clip(view);
    let mut y = view.1 - (max - offset);
    if total < view_h {
        y = view.1 + (view_h - total).min(0.0);
    }
    let code_bg = rgba(1.0, 1.0, 1.0, 0.1);
    for (index, (m, mh)) in h.msgs.iter().zip(cache.3.iter()).enumerate() {
        let (indent, _, _, pad, _) = agy_block(m.kind);
        if y + mh >= view.1
            && y <= view.3
            && let Ok(layout) = agy_layout(p, m, width, scale)
        {
            let (tx, ty) = (view.0 + indent * scale, y + pad * scale);
            let text_h = p.layout_height(&layout);
            let origin = (tx + if m.kind == Kind::User { 10.0 * scale } else { 0.0 }, ty);
            if let Some(geom) = h.geom {
                geom.borrow_mut().push(MsgGeom { index, x: origin.0, y: origin.1, layout: layout.clone() });
            }
            let part = h.sel.and_then(|_| selected_part(h.sel, index, m.doc.text.encode_utf16().count() as u32));
            match m.kind {
                Kind::User => {
                    p.fill_round_rect(
                        (view.0, y, view.0 + width, y + text_h + 2.0 * pad * scale),
                        12.0 * scale,
                        acc(0.18),
                    )?;
                    if let Some((start, len)) = part {
                        p.highlight(&layout, origin, start, len, acc(0.6))?;
                    }
                    p.md_draw(&layout, &m.doc, tx + 10.0 * scale, ty, WHITE, code_bg)?;
                }
                Kind::Assistant => {
                    if let Some((start, len)) = part {
                        p.highlight(&layout, origin, start, len, acc(0.6))?;
                    }
                    p.md_draw(&layout, &m.doc, tx, ty, WHITE, code_bg)?
                }
                Kind::Thinking => {
                    p.fill_round_rect(
                        (view.0 + 3.0 * scale, y, view.0 + 5.0 * scale, y + text_h),
                        1.0 * scale,
                        acc(0.5),
                    )?;
                    p.md_draw(&layout, &m.doc, tx, ty, rgba(1.0, 1.0, 1.0, 0.5), code_bg)?;
                }
                Kind::Tool => {
                    p.glyph(
                        '\u{E713}',
                        (view.0 + 2.0 * scale, y, view.0 + 20.0 * scale, y + 16.0 * scale),
                        11.0 * scale,
                        rgba(1.0, 1.0, 1.0, 0.45),
                    )?;
                    p.md_draw(&layout, &m.doc, tx, ty, rgba(1.0, 1.0, 1.0, 0.6), code_bg)?;
                }
                Kind::Error => p.md_draw(&layout, &m.doc, tx, ty, rgba(1.0, 0.5, 0.45, 1.0), code_bg)?,
            }
        }
        y += mh;
    }
    if running {
        p.text(
            "Working…",
            (view.0 + 2.0 * scale, y, view.2, y + 22.0 * scale),
            12.5 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 0.75, 0.3, 1.0),
        )?;
    }
    p.pop_clip();
    if max > 0.0 {
        let track = (view.2 - 4.0 * scale, view.1, view.2 - 1.0 * scale, view.3);
        let thumb_h = (view_h * view_h / total).max(24.0 * scale);
        let top = track.1 + (view_h - thumb_h) * (1.0 - offset / max);
        p.fill_round_rect((track.0, top, track.2, top + thumb_h), 1.5 * scale, rgba(1.0, 1.0, 1.0, 0.3))?;
    }
    Ok(())
}

pub const AGY_FONT: f32 = 13.0;

pub(in crate::view) fn paint_agy(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let (x, y, w, _) = pill;
    if ui.agy.open {
        let (_, inner) = agy_input(pill, scale, ui.agy.input_h.get());
        if let Ok(layout) =
            crate::ui::gfx::make_layout(p.dwrite(), &ui.agy.input.text(), inner.2 - inner.0, AGY_FONT * scale)
        {
            ui.agy.input_h.set(p.layout_height(&layout) + 16.0 * scale);
        }
    }
    if !crate::services::agy::installed() || ui.agy.chats.is_empty() {
        let msg = if crate::services::agy::installed() { "No conversations found" } else { "agy is not installed" };
        return p.text(
            msg,
            (x, y + 60.0 * scale, x + w, y + 340.0 * scale),
            15.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            rgba(1.0, 1.0, 1.0, 0.5),
        );
    }
    let (l, r) = agy_side(pill, scale);
    let (limit, exhausted) = agy_limit_text(ui);
    let chip_color = if exhausted { rgba(1.0, 0.5, 0.45, 1.0) } else { rgba(0.45, 0.9, 0.6, 0.9) };
    let chip = (r - 270.0 * scale, y + 60.0 * scale, r, y + 84.0 * scale);
    let selected_chat = ui.agy.sel.as_deref().and_then(|id| ui.agy.chats.iter().find(|c| c.id == id));

    if !ui.agy.open || selected_chat.is_none() {
        p.text(
            "AGY conversations",
            (l + 2.0 * scale, y + 60.0 * scale, l + 260.0 * scale, y + 84.0 * scale),
            13.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.7),
        )?;
        p.text(&limit, chip, 11.5 * scale, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_TRAILING, chip_color)?;
        for (i, chat) in ui.agy.chats.iter().skip(ui.agy.scroll).take(agy_rows()).enumerate() {
            let row = agy_row(pill, scale, i);
            let hovered = ui.home.hover_act == Some(1000 + i);
            p.fill_round_rect(row, 12.0 * scale, rgba(1.0, 1.0, 1.0, if hovered { 0.1 } else { 0.05 }))?;
            let dot = match ui.agy.jobs.get(&chat.id) {
                Some(crate::services::agy::Job::Running) => Some(rgba(1.0, 0.7, 0.25, 1.0)),
                Some(crate::services::agy::Job::Done { ok: true, .. }) => Some(rgba(0.4, 0.9, 0.55, 1.0)),
                Some(crate::services::agy::Job::Done { ok: false, .. }) => Some(rgba(1.0, 0.4, 0.4, 1.0)),
                None => None,
            };
            let text_left = row.0 + if dot.is_some() { 28.0 } else { 14.0 } * scale;
            if let Some(c) = dot {
                p.fill_ellipse(row.0 + 15.0 * scale, (row.1 + row.3) / 2.0, 4.5 * scale, 4.5 * scale, c)?;
            }
            p.text(
                &chat.title,
                (text_left, row.1 + 4.0 * scale, row.2 - 90.0 * scale, row.1 + 23.0 * scale),
                13.0 * scale,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                WHITE,
            )?;
            p.text(
                &chat.folder,
                (text_left, row.1 + 23.0 * scale, row.2 - 90.0 * scale, row.1 + 40.0 * scale),
                11.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                rgba(1.0, 1.0, 1.0, 0.5),
            )?;
            p.text(
                &time_ago(chat.modified),
                (row.2 - 84.0 * scale, row.1, row.2 - 12.0 * scale, row.3),
                11.5 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                rgba(1.0, 1.0, 1.0, 0.45),
            )?;
        }
        return Ok(());
    }

    let Some(chat) = selected_chat else { return Ok(()) };
    let back = agy_back(pill, scale);
    let back_hover = ui.home.hover_act == Some(1101);
    p.fill_round_rect(back, 16.0 * scale, rgba(1.0, 1.0, 1.0, if back_hover { 0.18 } else { 0.08 }))?;
    p.glyph('\u{E72B}', back, 14.0 * scale, WHITE)?;
    p.text(
        &chat.title,
        (back.2 + 10.0 * scale, y + 56.0 * scale, r - 250.0 * scale, y + 74.0 * scale),
        14.0 * scale,
        DWRITE_FONT_WEIGHT_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    p.text(
        &chat.folder,
        (back.2 + 10.0 * scale, y + 72.0 * scale, r - 250.0 * scale, y + 88.0 * scale),
        11.0 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.5),
    )?;
    p.text(
        &limit,
        (r - 248.0 * scale, y + 60.0 * scale, r, y + 84.0 * scale),
        11.5 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
        chip_color,
    )?;
    let running = matches!(ui.agy.jobs.get(&chat.id), Some(crate::services::agy::Job::Running));
    let history = History {
        msgs: &ui.agy.msgs,
        cache: &ui.agy.cache,
        id: ui.agy.sel.clone().unwrap_or_default(),
        hist: ui.agy.hist,
        max: &ui.agy.max,
        input_h: ui.agy.input_h.get(),
        reserve: 0.0,
        geom: None,
        sel: None,
    };
    paint_history(p, pill, scale, &history, running)?;
    let (frame, text_area) = agy_input(pill, scale, ui.agy.input_h.get());
    paint_editor(
        p,
        frame,
        text_area,
        &ui.agy.input,
        ui.editing && ui.tab == Tab::Agy,
        ui.caret_on,
        "Message to this chat…",
        AGY_FONT,
        scale,
    )?;
    let button = agy_send_button(pill, scale);
    let hover = ui.home.hover_act == Some(1100);
    let fill = if running {
        rgba(1.0, 0.35, 0.35, if hover { 0.5 } else { 0.35 })
    } else {
        acc(if hover { 0.5 } else { 0.35 })
    };
    p.fill_round_rect(button, 12.0 * scale, fill)?;
    p.text(
        if running { "Stop" } else { "Send" },
        button,
        13.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_CENTER,
        WHITE,
    )?;
    Ok(())
}
