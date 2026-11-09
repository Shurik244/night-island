use crate::platform::notifications::{Record, Snapshot, Status};
use crate::view::*;

pub struct NotificationsUi {
    pub preview: Option<Record>,
    pub full: bool,
    pub detail_scroll: f32,
    pub detail_max: std::cell::Cell<f32>,
    pub state: Snapshot,
    pub scroll: usize,
    pub hover: Option<NotificationsHit>,
    pub setup: bool,
}

impl Default for NotificationsUi {
    fn default() -> Self {
        Self {
            preview: None,
            full: false,
            detail_scroll: 0.0,
            detail_max: Default::default(),
            state: Snapshot::default(),
            scroll: 0,
            hover: None,
            setup: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NotificationsHit {
    Row(usize),
    Back,
    OpenApp,
    Dismiss,
    Connect,
    WindowsSettings,
    Example,
    Setup,
    History,
}

pub const NOTIFICATION_SIZE: (f32, f32) = (420.0, 158.0);

struct CardContent {
    title_top: f32,
    title_height: f32,
    body_top: f32,
    height: f32,
}

fn card_content(record: &Record, width: f32, scale: f32) -> CardContent {
    let width = (width - 36.0 * scale).max(1.0);
    let title_top = if record.avatar.is_some() { 62.0 } else { 54.0 } * scale;
    let measure = |text: &str, size, weight, max: f32| {
        if text.trim().is_empty() {
            0.0
        } else {
            crate::ui::fonts::wrapped_height(text, width, size * scale, weight).min(max * scale)
        }
    };
    let title_height = measure(&record.title, 15.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, 44.0);
    let body_height = measure(&record.body, 13.0, DWRITE_FONT_WEIGHT_NORMAL, 116.0);
    let gap = if title_height > 0.0 && body_height > 0.0 { 6.0 * scale } else { 0.0 };
    let body_top = title_top + title_height + gap;
    CardContent { title_top, title_height, body_top, height: body_top + body_height + 16.0 * scale }
}

pub fn notification_card_height(record: &Record, width: f32, scale: f32) -> f32 {
    card_content(record, width, scale).height
}

pub fn notification_card_width(record: &Record) -> f32 {
    let font = crate::ui::fonts::selected();
    let width = |text: &str, size, weight| crate::ui::fonts::width(font, text, size, weight);
    (width(&record.body, 13.0, DWRITE_FONT_WEIGHT_NORMAL).max(width(&record.title, 15.0, DWRITE_FONT_WEIGHT_SEMI_BOLD))
        + 36.0)
        .max(width(&record.app, 12.0, DWRITE_FONT_WEIGHT_SEMI_BOLD) + 166.0)
        .clamp(320.0, NOTIFICATION_SIZE.0)
}

pub fn notification_tile_count(ui: &UiState) -> usize {
    if ui.home.edit || ui.home.widgets.editing || ui.notifications.preview.is_none() {
        return 0;
    }
    match ui.cfg.notification_tiles {
        1 => 1,
        2 => 2,
        _ => 4,
    }
}

pub fn home_notification_rect(pill: Rect, scale: f32, ui: &UiState) -> Option<Rect> {
    let count = notification_tile_count(ui);
    if count == 0 {
        return None;
    }
    let first = action_tile(pill, scale, 0);
    let last = action_tile(pill, scale, count - 1);
    Some((first.0, first.1, last.2, last.3))
}

pub fn home_notification_expand(rect: Rect, scale: f32) -> Rect {
    (rect.2 - 57.0 * scale, rect.1 + 6.0 * scale, rect.2 - 31.0 * scale, rect.1 + 40.0 * scale)
}

pub fn home_notification_close(rect: Rect, scale: f32) -> Rect {
    (rect.2 - 29.0 * scale, rect.1 + 6.0 * scale, rect.2 - 3.0 * scale, rect.1 + 40.0 * scale)
}

pub fn paint_home_notification(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let Some(rect) = home_notification_rect(pill, scale, ui) else {
        return Ok(());
    };
    let Some(record) = &ui.notifications.preview else {
        return Ok(());
    };
    let large = notification_tile_count(ui) == 4;
    let hover = matches!(ui.home.hover_act, Some(562 | 563 | 566));
    p.fill_round_rect(rect, 16.0 * scale, acc(if hover { 0.23 } else { 0.14 }))?;
    if notification_tile_count(ui) == 1 {
        p.text(
            &record.title,
            (rect.0 + 10.0 * scale, rect.1 + 3.0 * scale, rect.2 - 31.0 * scale, rect.1 + 23.0 * scale),
            11.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        )?;
        p.text(
            &record.body,
            (rect.0 + 10.0 * scale, rect.1 + 23.0 * scale, rect.2 - 8.0 * scale, rect.3 - 3.0 * scale),
            11.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.65),
        )?;
        let close = home_notification_close(rect, scale);
        if ui.home.hover_act == Some(566) {
            p.fill_round_rect(close, 9.0 * scale, acc(0.3))?;
        }
        return p.glyph('\u{E711}', close, 10.0 * scale, WHITE);
    }
    let icon = (rect.0 + 9.0 * scale, rect.1 + 9.0 * scale, rect.0 + 37.0 * scale, rect.1 + 37.0 * scale);
    paint_notification_avatar(p, icon, scale, record)?;
    let text_left = icon.2 + 8.0 * scale;
    p.text(
        &record.title,
        (text_left, rect.1 + 4.0 * scale, rect.2 - 59.0 * scale, rect.1 + 25.0 * scale),
        12.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    p.text(
        if large { &record.app } else { &record.body },
        (text_left, rect.1 + 24.0 * scale, rect.2 - 59.0 * scale, rect.1 + 42.0 * scale),
        11.5 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.65),
    )?;
    if large {
        p.text_wrapped_trimmed(
            &record.body,
            (rect.0 + 12.0 * scale, rect.1 + 47.0 * scale, rect.2 - 12.0 * scale, rect.3 - 8.0 * scale),
            12.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            rgba(1.0, 1.0, 1.0, 0.8),
        )?;
    }
    let button = home_notification_expand(rect, scale);
    if ui.home.hover_act == Some(563) {
        p.fill_round_rect(button, 9.0 * scale, acc(0.3))?;
    }
    p.glyph('\u{E76C}', button, 11.0 * scale, WHITE)?;
    let close = home_notification_close(rect, scale);
    if ui.home.hover_act == Some(566) {
        p.fill_round_rect(close, 9.0 * scale, acc(0.3))?;
    }
    p.glyph('\u{E711}', close, 10.0 * scale, WHITE)
}

fn notification_detail_buttons(pill: Rect, scale: f32) -> [Rect; 3] {
    let (x, y, w, h) = pill;
    let top = y + h - 54.0 * scale;
    [
        (x + 24.0 * scale, top, x + 124.0 * scale, top + 32.0 * scale),
        (x + w / 2.0 - 100.0 * scale, top, x + w / 2.0 + 100.0 * scale, top + 32.0 * scale),
        (x + w - 124.0 * scale, top, x + w - 24.0 * scale, top + 32.0 * scale),
    ]
}

fn paint_notification_detail(
    p: &Painter,
    pill: Rect,
    scale: f32,
    ui: &NotificationsUi,
    record: &Record,
    _open_app: bool,
) -> Result<()> {
    let (x, y, w, h) = pill;
    paint_notification_avatar(
        p,
        (x + 24.0 * scale, y + 63.0 * scale, x + 64.0 * scale, y + 103.0 * scale),
        scale,
        record,
    )?;
    p.text(
        &record.app,
        (x + 78.0 * scale, y + 63.0 * scale, x + w - 24.0 * scale, y + 103.0 * scale),
        15.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    let body = (x + 24.0 * scale, y + 116.0 * scale, x + w - 32.0 * scale, y + h - 68.0 * scale);
    let max = p.text_scrolled(
        &format!("{}\n\n{}", record.title, record.body),
        body,
        14.0 * scale,
        ui.detail_scroll,
        rgba(1.0, 1.0, 1.0, 0.85),
    )?;
    ui.detail_max.set(max);
    if max > 0.0 {
        let height = body.3 - body.1;
        let thumb = (height * height / (height + max)).max(20.0 * scale);
        let top = body.1 + (height - thumb) * ui.detail_scroll.clamp(0.0, max) / max;
        p.fill_round_rect(
            (body.2 + 6.0 * scale, top, body.2 + 9.0 * scale, top + thumb),
            1.5 * scale,
            rgba(1.0, 1.0, 1.0, 0.35),
        )?;
    }
    for ((rect, label), hit) in notification_detail_buttons(pill, scale)
        .into_iter()
        .zip(["Back", "Open sender app", "Dismiss"])
        .zip([NotificationsHit::Back, NotificationsHit::OpenApp, NotificationsHit::Dismiss])
    {
        if hit == NotificationsHit::OpenApp && record.app_id == crate::platform::notifications::LOCAL_APP_ID {
            continue;
        }
        p.fill_round_rect(rect, 11.0 * scale, rgba(1.0, 1.0, 1.0, if ui.hover == Some(hit) { 0.14 } else { 0.07 }))?;
        p.text(label, rect, 12.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)?;
    }
    Ok(())
}

pub fn notification_buttons(pill: Rect, scale: f32) -> [Rect; 3] {
    let left = pill.0 + SIDE_PAD * scale;
    let right = pill.0 + pill.2 - SIDE_PAD * scale;
    let gap = 10.0 * scale;
    let width = (right - left - 2.0 * gap) / 3.0;
    std::array::from_fn(|i| {
        let x = left + i as f32 * (width + gap);
        (x, pill.1 + 145.0 * scale, x + width, pill.1 + 179.0 * scale)
    })
}

pub fn notifications_rows(pill: Rect, scale: f32) -> usize {
    ((pill.3 - 218.0 * scale) / (62.0 * scale)).floor().max(1.0) as usize
}

pub fn hit_notifications(
    pill: Rect,
    scale: f32,
    x: f32,
    y: f32,
    ui: &NotificationsUi,
    _open_app: bool,
) -> Option<NotificationsHit> {
    if ui.full
        && let Some(record) = &ui.preview
    {
        return notification_detail_buttons(pill, scale)
            .into_iter()
            .zip([NotificationsHit::Back, NotificationsHit::OpenApp, NotificationsHit::Dismiss])
            .find_map(|(rect, hit)| {
                let available =
                    hit != NotificationsHit::OpenApp || record.app_id != crate::platform::notifications::LOCAL_APP_ID;
                (available && inside(rect, x, y)).then_some(hit)
            });
    }
    for (rect, hit) in
        notification_sections(pill, scale).into_iter().zip([NotificationsHit::Setup, NotificationsHit::History])
    {
        if inside(rect, x, y) {
            return Some(hit);
        }
    }
    if !ui.setup {
        for row in 0..notifications_rows(pill, scale) {
            let top = pill.1 + (216.0 + row as f32 * 62.0) * scale;
            if inside((pill.0 + SIDE_PAD * scale, top, pill.0 + pill.2 - SIDE_PAD * scale, top + 56.0 * scale), x, y)
                && row + ui.scroll < ui.state.records.len()
            {
                return Some(NotificationsHit::Row(row + ui.scroll));
            }
        }
    }
    notification_buttons(pill, scale)
        .iter()
        .position(|rect| inside(*rect, x, y))
        .map(|i| [NotificationsHit::Connect, NotificationsHit::WindowsSettings, NotificationsHit::Example][i])
}

fn notification_sections(pill: Rect, scale: f32) -> [Rect; 2] {
    let left = pill.0 + SIDE_PAD * scale;
    [
        (left, pill.1 + 187.0 * scale, left + 126.0 * scale, pill.1 + 213.0 * scale),
        (left + 134.0 * scale, pill.1 + 187.0 * scale, left + 260.0 * scale, pill.1 + 213.0 * scale),
    ]
}

pub fn notification_close(rect: Rect, scale: f32) -> Rect {
    (rect.0 + rect.2 - 42.0 * scale, rect.1 + 10.0 * scale, rect.0 + rect.2 - 12.0 * scale, rect.1 + 40.0 * scale)
}

fn paint_notification_avatar(p: &Painter, rect: Rect, scale: f32, record: &Record) -> Result<()> {
    if let Some(avatar) = &record.avatar {
        p.image(rect, (rect.2 - rect.0) / 2.0, avatar.side, &avatar.bgra)?;
        if let Some(icon) = &record.icon {
            let badge = (rect.2 - 12.0 * scale, rect.3 - 12.0 * scale, rect.2 + 2.0 * scale, rect.3 + 2.0 * scale);
            p.fill_round_rect(
                (badge.0 - scale, badge.1 - scale, badge.2 + scale, badge.3 + scale),
                5.0 * scale,
                rgba(0.0, 0.0, 0.0, 1.0),
            )?;
            p.image(badge, 4.0 * scale, icon.side, &icon.bgra)?;
        }
    } else if let Some(icon) = &record.icon {
        p.image(rect, 9.0 * scale, icon.side, &icon.bgra)?;
    } else {
        p.fill_round_rect(rect, 9.0 * scale, acc(0.18))?;
        p.glyph('\u{E7F4}', rect, 17.0 * scale, acc(1.0))?;
    }
    Ok(())
}

pub fn paint_notification_card(
    p: &Painter,
    rect: Rect,
    scale: f32,
    record: &Record,
    pending: usize,
    hover_close: bool,
    scroll: Option<f32>,
) -> Result<f32> {
    let (x, y, w, h) = rect;
    let size = if record.avatar.is_some() { 40.0 } else { 32.0 } * scale;
    let icon = (x + 18.0 * scale, y + 14.0 * scale, x + 18.0 * scale + size, y + 14.0 * scale + size);
    paint_notification_avatar(p, icon, scale, record)?;
    p.text(
        &record.app,
        (icon.2 + 12.0 * scale, y + 14.0 * scale, x + w - 104.0 * scale, y + 38.0 * scale),
        12.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.62),
    )?;
    if pending > 0 {
        p.text(
            &format!("+{pending}"),
            (x + w - 98.0 * scale, y + 16.0 * scale, x + w - 48.0 * scale, y + 38.0 * scale),
            11.5 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
            rgba(1.0, 1.0, 1.0, 0.5),
        )?;
    }
    let close = notification_close(rect, scale);
    if hover_close {
        p.fill_round_rect(close, 15.0 * scale, rgba(1.0, 1.0, 1.0, 0.1))?;
    }
    p.glyph('\u{E711}', close, 10.0 * scale, rgba(1.0, 1.0, 1.0, 0.55))?;
    let layout = card_content(record, w, scale);
    p.text_wrapped_trimmed(
        &record.title,
        (x + 18.0 * scale, y + layout.title_top, x + w - 18.0 * scale, y + layout.title_top + layout.title_height),
        15.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        WHITE,
    )?;
    let body_bottom = y + h - 16.0 * scale;
    let body = (x + 18.0 * scale, (y + layout.body_top).min(body_bottom - scale), x + w - 18.0 * scale, body_bottom);
    let color = rgba(1.0, 1.0, 1.0, 0.72);
    if let Some(scroll) = scroll {
        let max = p.text_scrolled(&record.body, body, 13.0 * scale, scroll, color)?;
        if max > 0.0 {
            let height = body.3 - body.1;
            let thumb = (height * height / (height + max)).max(12.0 * scale);
            let top = body.1 + (height - thumb) * scroll.clamp(0.0, max) / max;
            p.fill_round_rect((body.2 + 3.0 * scale, top, body.2 + 6.0 * scale, top + thumb), scale, color)?;
        }
        Ok(max)
    } else {
        p.text_wrapped_trimmed(&record.body, body, 13.0 * scale, DWRITE_FONT_WEIGHT_NORMAL, color)?;
        Ok(0.0)
    }
}

pub fn notification_status(status: Status) -> &'static str {
    match status {
        Status::Disabled => "Show Windows notifications from the island",
        Status::NeedsAccess => "Allow notification access to connect the island",
        Status::Denied => "Notification access is disabled in Windows privacy settings",
        Status::NeedsQuietMode => "Turn on Do not disturb in Windows to hide the standard banners",
        Status::Ready => "Connected. The island receives Windows system notifications",
        Status::Unavailable => "Windows notifications are unavailable. Try connecting again",
    }
}

pub fn paint_notifications(
    p: &Painter,
    pill: Rect,
    scale: f32,
    ui: &NotificationsUi,
    enabled: bool,
    open_app: bool,
) -> Result<()> {
    if ui.full
        && let Some(record) = &ui.preview
    {
        return paint_notification_detail(p, pill, scale, ui, record, open_app);
    }
    let (x, y, w, _) = pill;
    let left = x + SIDE_PAD * scale;
    let right = x + w - SIDE_PAD * scale;
    p.text(
        "Windows notifications",
        (left, y + 62.0 * scale, right, y + 86.0 * scale),
        17.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    p.text_wrapped(
        notification_status(ui.state.status),
        (left, y + 91.0 * scale, right, y + 137.0 * scale),
        12.5 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        rgba(1.0, 1.0, 1.0, 0.6),
    )?;
    for (i, rect) in notification_buttons(pill, scale).into_iter().enumerate() {
        let selected = ui.hover
            == Some([NotificationsHit::Connect, NotificationsHit::WindowsSettings, NotificationsHit::Example][i]);
        p.fill_round_rect(
            rect,
            11.0 * scale,
            if i == 0 && enabled { acc(0.24) } else { rgba(1.0, 1.0, 1.0, if selected { 0.13 } else { 0.07 }) },
        )?;
        let label = match i {
            0 => {
                if matches!(ui.state.status, Status::NeedsAccess | Status::Denied) {
                    "Allow access"
                } else if ui.state.status == Status::Unavailable {
                    "Try again"
                } else if enabled {
                    "Disable"
                } else {
                    "Connect"
                }
            }
            1 => "Windows settings",
            _ => "Show example",
        };
        p.text(label, rect, 12.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)?;
    }
    for (i, rect) in notification_sections(pill, scale).into_iter().enumerate() {
        let selected = ui.setup == (i == 0);
        if selected {
            p.fill_round_rect(rect, 9.0 * scale, rgba(1.0, 1.0, 1.0, 0.1))?;
        }
        p.text(
            if i == 0 { "Setup guide" } else { "Notification history" },
            rect,
            11.5 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            rgba(1.0, 1.0, 1.0, if selected { 1.0 } else { 0.55 }),
        )?;
    }
    if ui.setup {
        let steps = [
            ("1. Connect notification access", "Press Connect and allow access when Windows asks."),
            (
                "2. Keep notifications, hide duplicate banners",
                "Windows settings → your app: banners off, notification center on. Do not disturb alone can have exceptions.",
            ),
            (
                "3. Telegram needs Windows notifications",
                "Telegram → Settings → Notifications and Sounds → Use Windows notifications. Custom Telegram popups cannot reach the island.",
            ),
        ];
        for (i, (title, body)) in steps.into_iter().enumerate() {
            let top = y + (221.0 + i as f32 * 49.0) * scale;
            p.text(
                title,
                (left + 4.0 * scale, top, right - 4.0 * scale, top + 18.0 * scale),
                12.0 * scale,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                WHITE,
            )?;
            p.text_wrapped_trimmed(
                &crate::ui::i18n::tr(body),
                (left + 4.0 * scale, top + 19.0 * scale, right - 4.0 * scale, top + 50.0 * scale),
                11.5 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                rgba(1.0, 1.0, 1.0, 0.65),
            )?;
        }
        return Ok(());
    }
    if ui.state.records.is_empty() {
        return p.text(
            "New notifications will be listed here",
            (left, y + 228.0 * scale, right, y + 298.0 * scale),
            13.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            rgba(1.0, 1.0, 1.0, 0.45),
        );
    }
    for (row, record) in ui.state.records.iter().skip(ui.scroll).take(notifications_rows(pill, scale)).enumerate() {
        let top = y + (216.0 + row as f32 * 62.0) * scale;
        p.fill_round_rect((left, top, right, top + 56.0 * scale), 12.0 * scale, rgba(1.0, 1.0, 1.0, 0.045))?;
        paint_notification_avatar(
            p,
            (left + 10.0 * scale, top + 12.0 * scale, left + 42.0 * scale, top + 44.0 * scale),
            scale,
            record,
        )?;
        p.text(
            &format!("{} · {}", record.app, record.title),
            (left + 54.0 * scale, top + 4.0 * scale, right - 12.0 * scale, top + 27.0 * scale),
            12.5 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        )?;
        p.text(
            &record.body.replace('\n', " "),
            (left + 54.0 * scale, top + 27.0 * scale, right - 12.0 * scale, top + 49.0 * scale),
            12.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.55),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tile_tests {
    use super::*;

    fn message() -> Record {
        Record {
            id: 1,
            created: 1,
            app_id: "com.squirrel.Discord.Discord".into(),
            app: "Discord".into(),
            title: "Александр".into(),
            body: "Уведомление с полным текстом".into(),
            icon: None,
            avatar: None,
        }
    }

    #[test]
    #[ignore = "Needs real DirectWrite text measuring, which wine does not provide"]
    fn notification_cards_follow_wrapping_font_and_content_length() {
        let selected = crate::ui::fonts::selected();
        for font in crate::data::settings::InterfaceFont::ALL {
            crate::ui::fonts::set(font);
            for scale in [0.75, 1.0, 1.25, 1.5, 2.0] {
                let mut record = message();
                record.app = "Night Island".into();
                record.title = "Ассистент".into();
                record.body = "Музыка на паузе.".into();
                let width = notification_card_width(&record) * scale;
                let short = notification_card_height(&record, width, scale);
                assert!(short < NOTIFICATION_SIZE.1 * scale);
                assert!(width < NOTIFICATION_SIZE.0 * scale);
                let content = card_content(&record, width, scale);
                let measured = crate::ui::fonts::wrapped_height(
                    &record.body,
                    width - 36.0 * scale,
                    13.0 * scale,
                    DWRITE_FONT_WEIGHT_NORMAL,
                );
                assert!(content.height - content.body_top - 16.0 * scale >= measured - 0.01);
                record.body = "Первая строка.\nВторая строка.\nТретья строка.".into();
                let multiline = notification_card_height(&record, width, scale);
                assert!(multiline > short);
                record.body = "Подробный ответ ассистента с пояснениями. ".repeat(100);
                let long = notification_card_height(&record, width, scale);
                assert!(long > multiline && long <= 238.0 * scale);
                record.body.clear();
                assert!(notification_card_height(&record, width, scale) < short);
            }
        }
        crate::ui::fonts::set(selected);
    }

    #[test]
    fn notification_tiles_reserve_only_the_selected_slots_at_every_scale() {
        for scale in [0.8, 1.0, 1.25, 1.5, 2.0] {
            for count in [1, 2, 4] {
                let mut ui = UiState::new();
                ui.cfg.home_actions = vec![0, 3, 22];
                ui.cfg.notification_tiles = count;
                ui.notifications.preview = Some(message());
                let pill = (20.0, 10.0, 640.0 * scale, 384.0 * scale);
                let rect = home_notification_rect(pill, scale, &ui).unwrap();
                let end = action_tile(pill, scale, count as usize - 1);
                assert_eq!((rect.2, rect.3), (end.2, end.3));
                assert!(matches!(
                    hit_home(pill, scale, rect.0 + 10.0 * scale, rect.1 + 20.0 * scale, &ui),
                    Some(HomeHit::Notification)
                ));
                let button = home_notification_expand(rect, scale);
                let hit = hit_home(pill, scale, (button.0 + button.2) / 2.0, (button.1 + button.3) / 2.0, &ui);
                assert!(if count == 1 {
                    matches!(hit, Some(HomeHit::Notification))
                } else {
                    matches!(hit, Some(HomeHit::NotificationExpand))
                });
                let close = home_notification_close(rect, scale);
                assert!(matches!(
                    hit_home(pill, scale, (close.0 + close.2) / 2.0, (close.1 + close.3) / 2.0, &ui),
                    Some(HomeHit::NotificationClose)
                ));
                let next = action_tile(pill, scale, count as usize);
                assert!(matches!(
                    hit_home(pill, scale, (next.0 + next.2) / 2.0, (next.1 + next.3) / 2.0, &ui),
                    Some(HomeHit::Action(0))
                ));
                ui.notifications.preview = None;
                assert_eq!(notification_tile_count(&ui), 0);
                assert_eq!(ui.cfg.home_actions, vec![0, 3, 22]);
            }
        }
    }

    #[test]
    fn full_notification_has_separate_reading_and_app_actions() {
        let mut ui = NotificationsUi { preview: Some(message()), full: true, ..Default::default() };
        let pill = (0.0, 0.0, 640.0, 384.0);
        let buttons = notification_detail_buttons(pill, 1.0);
        for (index, button) in buttons.iter().enumerate() {
            let hit = hit_notifications(pill, 1.0, (button.0 + button.2) / 2.0, (button.1 + button.3) / 2.0, &ui, true);
            assert!(hit == Some([NotificationsHit::Back, NotificationsHit::OpenApp, NotificationsHit::Dismiss][index]));
        }
        let b = buttons[1];
        assert!(
            hit_notifications(pill, 1.0, (b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0, &ui, false)
                == Some(NotificationsHit::OpenApp)
        );
        ui.full = false;
        ui.setup = false;
        ui.state.records.push(message());
        assert!(hit_notifications(pill, 1.0, 40.0, 240.0, &ui, false) == Some(NotificationsHit::Row(0)));
    }
}
