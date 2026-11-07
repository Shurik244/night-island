use crate::data::settings::Edge;
use crate::view::*;

pub const VERT_W: f32 = 46.0;

const V_PAD: f32 = 16.0;

const V_GAP: f32 = 9.0;

const V_MIN: f32 = 140.0;

const V_COVER: f32 = 30.0;
const V_WEATHER: f32 = 46.0;
const V_REC: f32 = 36.0;
const V_AGENT: f32 = 78.0;
const V_CLOCK: f32 = 38.0;
const V_TITLE: f32 = 110.0;

const V_DOT: f32 = 15.0;

pub(in crate::view) fn indicators_height(ui: &UiState) -> f32 {
    let dots = usize::from(ui.privacy.mic || ui.mic_muted == Some(true)) + usize::from(ui.privacy.cam);
    dots as f32 * V_DOT + if vpn_visible(ui) { V_DOT + 2.0 } else { 0.0 }
}

pub(in crate::view) fn top_reserved(ui: &UiState) -> f32 {
    V_PAD + ui.char_inset + indicators_height(ui)
}

fn battery_height(ui: &UiState) -> Option<f32> {
    battery()?;
    match ui.cfg.pill_battery {
        BatteryStyle::Hidden => None,
        BatteryStyle::Icon => Some(26.0),
        BatteryStyle::Percent => Some(44.0),
    }
}

pub struct VSlots {
    pub cover: Option<(f32, f32)>,
    pub weather: Option<(f32, f32)>,
    pub bars: Option<(f32, f32)>,
    pub title: Option<(f32, f32)>,
    pub rec: Option<(f32, f32)>,
    pub agent: Option<(f32, f32)>,
    pub metrics: Option<(f32, f32)>,
    pub clock: (f32, f32),
    pub battery: Option<(f32, f32)>,
    pub length: f32,
}

pub fn vertical_slots(ui: &UiState, scale: f32, pill_len: f32) -> VSlots {
    let top = top_reserved(ui);
    let m = music_slots(ui, 1.0, 0.0);
    let weather = m.weather.map(|_| V_WEATHER);
    let battery = battery_height(ui);
    if !row_pill(ui) {
        let length = (top + weather.unwrap_or(0.0) + V_CLOCK + battery.unwrap_or(0.0) + 2.0 * V_GAP + V_PAD).max(V_MIN);
        let total = if pill_len > 0.0 { pill_len / scale } else { length };
        let at = |y: f32, h: f32| (y * scale, h * scale);
        let free_bottom = total - V_PAD - battery.map_or(0.0, |h| h + V_GAP);
        let group_h = V_CLOCK + weather.map_or(0.0, |h| h + V_GAP);
        let start = top + ((free_bottom - top - group_h) / 2.0).max(0.0);
        let before = ui.cfg.pill_weather_side == crate::data::settings::WeatherSide::Left;
        let clock_y = start + if before { weather.map_or(0.0, |h| h + V_GAP) } else { 0.0 };
        let weather_y = if before { start } else { clock_y + V_CLOCK + V_GAP };
        return VSlots {
            cover: None,
            weather: weather.map(|h| at(weather_y, h)),
            bars: None,
            title: None,
            rec: None,
            agent: None,
            metrics: None,
            clock: at(clock_y, V_CLOCK),
            battery: battery.map(|h| at(total - V_PAD - h, h)),
            length,
        };
    }
    let cover = m.cover.map(|_| V_COVER);
    let bars = m.bars.map(|_| BARS_SLOT);
    let title = m.title.map(|_| V_TITLE);
    let rec = m.rec.map(|_| V_REC);
    let agent = m.agent.map(|_| V_AGENT);
    let metrics = m.metrics.map(|(_, w)| w);
    let heights = [cover, weather, bars, title, rec, agent, metrics, Some(V_CLOCK), battery];
    let count = heights.iter().flatten().count() as f32;
    let content: f32 = heights.iter().flatten().sum::<f32>() + V_GAP * (count - 1.0).max(0.0);
    let length = (top + content + V_PAD).max(V_MIN);
    let total = if pill_len > 0.0 { pill_len / scale } else { length };
    let mut y = top + (total - top - V_PAD - content).max(0.0) / 2.0;
    let mut place = |h: Option<f32>| {
        h.map(|h| {
            let slot = (y * scale, h * scale);
            y += h + V_GAP;
            slot
        })
    };
    let cover = place(cover);
    let bars = place(bars);
    let title = place(title);
    let rec = place(rec);
    let agent = place(agent);
    let metrics = place(metrics);
    let before = ui.cfg.pill_weather_side == crate::data::settings::WeatherSide::Left;
    let weather_left = if before { place(weather) } else { None };
    let clock = place(Some(V_CLOCK)).unwrap_or((0.0, 0.0));
    let weather = weather_left.or_else(|| if before { None } else { place(weather) });
    let battery = place(battery);
    VSlots { cover, weather, bars, title, rec, agent, metrics, clock, battery, length }
}

pub fn vertical_length(ui: &UiState) -> f32 {
    let chars = |s: &str| crate::ui::i18n::tr(s).chars().count() as f32;
    if let Some((_, text)) = &ui.notice {
        return (46.0 + notice_text_width(ui, text) + 16.0).clamp(V_MIN, 460.0);
    }
    if ui.volume.is_some() {
        return V_MIN + 20.0;
    }
    if let Some(name) = &ui.downloading {
        return (top_reserved(ui) + 40.0 + chars(&downloading_label(name)) * 6.9 + 16.0).clamp(V_MIN, 420.0);
    }
    vertical_slots(ui, 1.0, 0.0).length
}

pub(in crate::view) fn paint_collapsed_vertical(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let (x, y, w, h) = pill;
    let cx = x + w / 2.0;
    let ccw = ui.cfg.dock_edge == Edge::Left;
    let along = |text: &str, rect: Rect, size: f32, weight, align, color| -> Result<()> {
        p.rotated(rect, ccw, |p, local| p.text(text, local, size * scale, weight, align, color))
    };
    let (top, bottom) = (y + 10.0 * scale, y + h - 10.0 * scale);
    if let Some((glyph, text)) = &ui.notice {
        p.glyph(*glyph, (x, top, x + w, top + 30.0 * scale), 16.0 * scale, WHITE)?;
        return along(
            text,
            (x, top + 36.0 * scale, x + w, bottom),
            13.0,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        );
    }
    paint_indicators(p, pill, scale, ui)?;
    if let Some(level) = ui.volume {
        let level = level.clamp(0.0, 1.0);
        let icon = if level <= 0.001 { '\u{E74F}' } else { '\u{E767}' };
        p.glyph(icon, (x, top, x + w, top + 30.0 * scale), 16.0 * scale, WHITE)?;
        let (track_top, track_bottom) = (top + 40.0 * scale, bottom - 26.0 * scale);
        let half = 3.0 * scale;
        p.fill_round_rect((cx - half, track_top, cx + half, track_bottom), half, rgba(1.0, 1.0, 1.0, 0.22))?;
        let filled = track_bottom - (track_bottom - track_top) * level;
        p.fill_round_rect((cx - half, filled, cx + half, track_bottom), half, WHITE)?;
        return p.text(
            &format!("{}%", (level * 100.0).round() as u32),
            (x, bottom - 20.0 * scale, x + w, bottom + 4.0 * scale),
            12.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            WHITE,
        );
    }
    if let Some(name) = &ui.downloading {
        let start = y + top_reserved(ui) * scale;
        p.glyph('\u{E896}', (x, start, x + w, start + 28.0 * scale), 15.0 * scale, acc(1.0))?;
        return along(
            &downloading_label(name),
            (x, start + 34.0 * scale, x + w, bottom),
            12.5,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        );
    }
    let slots = vertical_slots(ui, scale, h);
    let slot = |(off, len): (f32, f32)| (x, y + off, x + w, y + off + len);
    if let (Some(s), Some(cover)) = (slots.cover, &ui.media.cover) {
        let side = (w - 14.0 * scale).min(V_COVER * scale);
        let top = y + s.0;
        if !ui.cover_in_flight && !ui.track_in_flight {
            p.cover((cx - side / 2.0, top, cx + side / 2.0, top + side), 6.0 * scale, cover, ui.cfg.cover_mode)?;
        }
    }
    if let (Some(s), Some(wx)) = (slots.weather, &ui.weather) {
        let r = slot(s);
        paint_pill_weather_icon(p, pill, scale, ui)?;
        p.text(
            &format!("{}°", wx.temp.round() as i32),
            (r.0, r.1 + 26.0 * scale, r.2, r.3),
            row_text_size(ui) * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            rgba(1.0, 1.0, 1.0, 0.85),
        )?;
    }
    if let Some(s) = slots.title
        && !ui.track_in_flight
    {
        along(
            &ui.media.title,
            slot(s),
            row_text_size(ui),
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        )?;
    }
    if let (Some(s), Some(secs)) = (slots.rec, ui.recording_secs) {
        let r = slot(s);
        let alpha = if secs % 2 == 0 { 1.0 } else { 0.45 };
        let rad = 4.5 * scale;
        let cy = r.1 + 6.0 * scale;
        p.fill_ellipse(cx, cy, rad * 1.8, rad * 1.8, rgba(1.0, 0.27, 0.27, 0.18 * alpha))?;
        p.fill_ellipse(cx, cy, rad, rad, rgba(1.0, 0.27, 0.27, alpha))?;
        p.text(
            &rec_time(secs),
            (r.0, r.1 + 15.0 * scale, r.2, r.3),
            12.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            rgba(1.0, 0.55, 0.55, 1.0),
        )?;
    }
    if let Some(s) = slots.agent {
        let r = slot(s);
        let (label, color) = agent_label(ui);
        let cy = r.1 + 5.0 * scale;
        p.fill_ellipse(cx, cy, 8.0 * scale, 8.0 * scale, rgba(color.r, color.g, color.b, 0.2))?;
        p.fill_ellipse(cx, cy, 4.0 * scale, 4.0 * scale, color)?;
        along(
            &label,
            (r.0, r.1 + 16.0 * scale, r.2, r.3),
            12.0,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            color,
        )?;
    }
    if let Some(s) = slots.metrics {
        along(
            &ui.metrics.pill,
            slot(s),
            11.5,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.85),
        )?;
    }
    let clock = clock().hh_mm;
    let r = slot(slots.clock);
    let half = (r.3 - r.1) / 2.0;
    let line = |text: &str, top: f32| {
        p.text(
            text,
            (r.0, top, r.2, top + half),
            clock_text_size(ui) * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            WHITE,
        )
    };
    line(clock.get(..2).unwrap_or(""), r.1)?;
    line(clock.get(3..).unwrap_or(""), r.1 + half)?;
    if let (Some(s), Some((percent, charging))) = (slots.battery, battery()) {
        let r = slot(s);
        let icon = (r.0, r.1, r.2, r.1 + 24.0 * scale);
        p.rotated(icon, ccw, |p, local| {
            paint_battery(
                p,
                (local.0 + local.2) / 2.0 - 2.0 * scale,
                (local.1 + local.3) / 2.0,
                scale,
                percent,
                charging,
            )
        })?;
        if ui.cfg.pill_battery == BatteryStyle::Percent {
            p.text(
                &format!("{percent}%"),
                (r.0, r.1 + 24.0 * scale, r.2, r.3),
                12.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                rgba(1.0, 1.0, 1.0, DIM),
            )?;
        }
    }
    if progress_strip_on(ui) {
        p.rotated((x, y, x + w, y + h), ccw, |p, local| paint_progress_strip(p, local, scale, ui))?;
    }
    Ok(())
}
