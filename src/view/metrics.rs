use crate::platform::metrics::{Metric, Sample};
use crate::view::*;

pub const HISTORY: usize = 60;

#[derive(Default)]
pub struct MetricsUi {
    pub sample: Option<Sample>,
    pub history: [Vec<f32>; 7],
    pub pill: String,
}

impl MetricsUi {
    pub fn push(&mut self, sample: Sample, pill_keys: &[String]) {
        for (i, m) in Metric::ALL.into_iter().enumerate() {
            let list = &mut self.history[i];
            list.push(m.value(&sample).unwrap_or(0.0));
            if list.len() > HISTORY {
                list.remove(0);
            }
        }
        self.pill = pill_text(&sample, pill_keys);
        self.sample = Some(sample);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn pill_value(m: Metric, s: &Sample) -> String {
    let Some(v) = m.value(s) else { return "  —".into() };
    match m {
        Metric::NetDown | Metric::NetUp => {
            if v < 1_000_000.0 {
                format!("{:>4}K", (v / 1000.0).round() as i32)
            } else {
                format!("{:>4.1}M", v / 1_000_000.0)
            }
        }
        Metric::Uptime => crate::platform::metrics::uptime_text(s.uptime).replace(" min", "m").replace(' ', ""),
        _ => format!("{:>3}%", v.round() as i32),
    }
}

pub fn pill_text(s: &Sample, keys: &[String]) -> String {
    keys.iter()
        .filter_map(|k| Metric::from_key(k))
        .map(|m| format!("{} {}", m.short(), pill_value(m, s)))
        .collect::<Vec<_>>()
        .join("  ")
}

pub fn pill_chars(keys: &[String]) -> usize {
    let one = |m: Metric| match m {
        Metric::NetDown | Metric::NetUp => 1 + 1 + 5,
        Metric::Uptime => 2 + 1 + 6,
        _ => 3 + 1 + 4,
    };
    let metrics: Vec<Metric> = keys.iter().filter_map(|k| Metric::from_key(k)).collect();
    metrics.iter().map(|m| one(*m)).sum::<usize>() + 2 * metrics.len().saturating_sub(1)
}

pub fn pill_metrics_on(cfg: &Settings) -> bool {
    cfg.metrics_pill.iter().any(|k| Metric::from_key(k).is_some())
}

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    One(Metric),
    Net,
}

fn cells(shown: &[Metric]) -> Vec<Cell> {
    let both = shown.contains(&Metric::NetDown) && shown.contains(&Metric::NetUp);
    let mut out = Vec::new();
    for m in shown {
        match m {
            Metric::NetDown | Metric::NetUp if both => {
                if !out.contains(&Cell::Net) {
                    out.push(Cell::Net);
                }
            }
            m => out.push(Cell::One(*m)),
        }
    }
    out
}

fn metric_glyph(m: Metric) -> char {
    match m {
        Metric::Cpu => '\u{E950}',
        Metric::Ram => '\u{E964}',
        Metric::Gpu => '\u{E7F4}',
        Metric::Disk => '\u{EDA2}',
        Metric::NetDown => '\u{E74B}',
        Metric::NetUp => '\u{E74A}',
        Metric::Uptime => '\u{E823}',
    }
}

pub(in crate::view) fn paint_metrics_row(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let (x, y, w, h) = pill;
    let shown: Vec<Metric> = ui.cfg.metrics_tab.iter().filter_map(|k| Metric::from_key(k)).collect();
    let Some(sample) = ui.metrics.sample.as_ref().filter(|_| !shown.is_empty()) else { return Ok(()) };
    let list = cells(&shown);
    let side = SIDE_PAD * scale;
    let top = devices_row_top(pill, scale) + devices_row_space(pill, scale, ui);
    let bottom = y + h - 14.0 * scale;
    let avail = bottom - top;
    if avail < 30.0 * scale {
        return Ok(());
    }
    let cols = list.len().clamp(1, 3);
    let rows = list.len().div_ceil(3).max(1);
    let cell_h = (avail / rows as f32).min(68.0 * scale);
    let roomy = rows == 1 && cell_h >= 56.0 * scale;
    let panel = (x + side, top, x + w - side, top + cell_h * rows as f32);
    let cell_w = (panel.2 - panel.0) / cols as f32;
    p.fill_round_rect(panel, 16.0 * scale, rgba(1.0, 1.0, 1.0, 0.06))?;
    let line = rgba(1.0, 1.0, 1.0, 0.09);
    for c in 1..cols {
        let lx = panel.0 + c as f32 * cell_w;
        p.fill_rect((lx - 0.5, panel.1 + 10.0 * scale, lx + 0.5, panel.3 - 10.0 * scale), line)?;
    }
    for r in 1..rows {
        let ly = panel.1 + r as f32 * cell_h;
        p.fill_rect((panel.0 + 12.0 * scale, ly - 0.5, panel.2 - 12.0 * scale, ly + 0.5), line)?;
    }
    let (ar, ag, ab) = crate::ui::theme::accent_u8();
    let accent = (f32::from(ar) / 255.0, f32::from(ag) / 255.0, f32::from(ab) / 255.0);
    for (slot, cell) in list.iter().enumerate() {
        let (col, row) = ((slot % 3) as f32, (slot / 3) as f32);
        let t = (
            panel.0 + col * cell_w,
            panel.1 + row * cell_h,
            panel.0 + (col + 1.0) * cell_w,
            panel.1 + (row + 1.0) * cell_h,
        );
        let level = match cell {
            Cell::One(m) if m.is_percent() => m.value(sample).unwrap_or(0.0),
            _ => 0.0,
        };
        let tint = if level >= 90.0 {
            (1.0, 0.42, 0.38)
        } else if level >= 70.0 {
            (1.0, 0.75, 0.3)
        } else {
            accent
        };
        let d = (cell_h - 18.0 * scale).min(32.0 * scale);
        let circle = (t.0 + 14.0 * scale, (t.1 + t.3 - d) / 2.0, t.0 + 14.0 * scale + d, (t.1 + t.3 + d) / 2.0);
        p.fill_round_rect(circle, d / 2.0, rgba(tint.0, tint.1, tint.2, 0.28))?;
        let glyph = match cell {
            Cell::One(m) => metric_glyph(*m),
            Cell::Net => '\u{E839}',
        };
        p.glyph(glyph, circle, d * 0.5, rgba(tint.0.max(0.8), tint.1.max(0.8), tint.2.max(0.8), 1.0))?;
        let text_left = circle.2 + 10.0 * scale;
        let text_right = t.2 - 12.0 * scale;
        match cell {
            Cell::One(m) => {
                let detail = m.detail(sample).filter(|_| roomy);
                let (label_h, value_h, extra_h) =
                    (if roomy { 17.0 } else { 14.0 }, if roomy { 24.0 } else { 20.0 }, 14.0);
                let gauge = m.is_percent() && detail.is_none();
                let block = (label_h + value_h + if detail.is_some() || gauge { extra_h } else { 0.0 }) * scale;
                let first = (t.1 + t.3) / 2.0 - block / 2.0;
                p.text(
                    m.label(),
                    (text_left, first, text_right, first + label_h * scale),
                    if roomy { 11.5 } else { 10.5 } * scale,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    rgba(1.0, 1.0, 1.0, 0.55),
                )?;
                let value_top = first + label_h * scale;
                p.text(
                    &m.text(sample),
                    (text_left, value_top, text_right, value_top + value_h * scale),
                    if roomy { 18.0 } else { 15.0 } * scale,
                    DWRITE_FONT_WEIGHT_BOLD,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    WHITE,
                )?;
                let extra_top = value_top + value_h * scale;
                if let Some(detail) = detail {
                    p.text(
                        &detail,
                        (text_left, extra_top, text_right, extra_top + extra_h * scale),
                        10.5 * scale,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        rgba(1.0, 1.0, 1.0, 0.45),
                    )?;
                } else if gauge {
                    let mid = extra_top + 5.0 * scale;
                    let bar = (text_left, mid - 1.5 * scale, text_right, mid + 1.5 * scale);
                    p.fill_round_rect(bar, 1.5 * scale, rgba(1.0, 1.0, 1.0, 0.1))?;
                    let fill = (bar.0 + (bar.2 - bar.0) * (level / 100.0).clamp(0.0, 1.0)).max(bar.0 + 3.0 * scale);
                    p.fill_round_rect((bar.0, bar.1, fill, bar.3), 1.5 * scale, rgba(tint.0, tint.1, tint.2, 1.0))?;
                }
            }
            Cell::Net => {
                let lines = [(Metric::NetDown, '\u{E74B}'), (Metric::NetUp, '\u{E74A}')];
                let line_h = ((t.3 - t.1) - 14.0 * scale) / 2.0;
                for (i, (m, arrow)) in lines.into_iter().enumerate() {
                    let top = t.1 + 7.0 * scale + i as f32 * line_h;
                    let arrow_box = (text_left - 2.0 * scale, top, text_left + 14.0 * scale, top + line_h);
                    p.glyph(arrow, arrow_box, 11.0 * scale, rgba(1.0, 1.0, 1.0, 0.6))?;
                    p.text(
                        &m.text(sample),
                        (text_left + 16.0 * scale, top, text_right, top + line_h),
                        if roomy { 15.0 } else { 13.5 } * scale,
                        DWRITE_FONT_WEIGHT_BOLD,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        WHITE,
                    )?;
                }
            }
        }
    }
    Ok(())
}
