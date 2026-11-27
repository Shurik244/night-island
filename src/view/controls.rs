use crate::view::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ctl {
    WiFi,
    Bluetooth,
    Vpn,
    Theme,
    Battery,
    Mic,
    Lock,
    Sleep,
    Restart,
    PowerOff,
}

pub const CTLS: [(Ctl, char, &str); 9] = [
    (Ctl::WiFi, '\u{E701}', "Wi-Fi"),
    (Ctl::Bluetooth, '\u{E702}', "Bluetooth"),
    (Ctl::Vpn, VPN_GLYPH, "VPN"),
    (Ctl::Theme, '\u{E706}', "Dark theme"),
    (Ctl::Battery, '\u{E83F}', "Battery"),
    (Ctl::Lock, '\u{E72E}', "Lock"),
    (Ctl::Sleep, '\u{E708}', "Sleep"),
    (Ctl::Restart, '\u{E777}', "Restart"),
    (Ctl::PowerOff, '\u{E7E8}', "Shut down"),
];

pub fn needs_confirm(c: Ctl) -> bool {
    matches!(c, Ctl::Lock | Ctl::Sleep | Ctl::Restart | Ctl::PowerOff)
}

pub(in crate::view) const CTL_COLS: usize = 5;

const CTL_SMALL_COLS: usize = 4;

pub(in crate::view) const CTL_BIG_H: f32 = 112.0;

pub(in crate::view) const CTL_SMALL_H: f32 = 60.0;

pub(in crate::view) const CTL_GAP: f32 = 12.0;

pub(in crate::view) fn ctl_rect(pill: Rect, scale: f32, i: usize) -> Rect {
    let (x, y, w, _) = pill;
    let side = SIDE_PAD * scale;
    let gap = CTL_GAP * scale;
    let (cols, col) = if i < CTL_COLS { (CTL_COLS, i) } else { (CTL_SMALL_COLS, i - CTL_COLS) };
    let tile_w = (w - 2.0 * side - (cols - 1) as f32 * gap) / cols as f32;
    let left = x + side + col as f32 * (tile_w + gap);
    let (top, h) = if i < CTL_COLS {
        (y + 62.0 * scale, CTL_BIG_H * scale)
    } else {
        (y + 62.0 * scale + CTL_BIG_H * scale + gap, CTL_SMALL_H * scale)
    };
    (left, top, left + tile_w, top + h)
}

pub fn hit_ctl(pill: Rect, scale: f32, px: f32, py: f32) -> Option<usize> {
    (0..CTLS.len()).find(|&i| inside(ctl_rect(pill, scale, i), px, py))
}

pub(in crate::view) fn paint_controls(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    for (i, (ctl, glyph, label)) in CTLS.iter().enumerate() {
        let r = ctl_rect(pill, scale, i);
        let (l, t, rr, b) = r;
        let state = match ctl {
            Ctl::WiFi => Some(ui.radios.wifi),
            Ctl::Bluetooth => Some(ui.radios.bluetooth),
            Ctl::Theme => Some(Some(ui.dark)),
            Ctl::Vpn => Some(ui.vpn),
            _ => None,
        };
        let on = state == Some(Some(true));
        let armed = ui.armed == Some(i);
        let hover = ui.feedback.sample(
            700_000 + i as u64,
            f32::from(ui.hover_ctl == Some(i) && *ctl != Ctl::Battery),
            0.0,
            140,
        );
        let bg = if armed { rgba(1.0, 0.35, 0.35, 0.28) } else { rgba(1.0, 1.0, 1.0, 0.08 + 0.05 * hover) };
        p.fill_round_rect(r, 20.0 * scale, bg)?;

        if i < CTL_COLS {
            let d = 46.0 * scale;
            let circle = (l + 16.0 * scale, t + 16.0 * scale, l + 16.0 * scale + d, t + 16.0 * scale + d);
            if *ctl == Ctl::Battery {
                if let Some((percent, charging)) = battery() {
                    paint_battery(p, l + 38.0 * scale, t + 30.0 * scale, scale, percent, charging)?;
                    p.text(
                        &format!("{percent}%"),
                        (l + 56.0 * scale, t + 16.0 * scale, rr - 6.0 * scale, t + 44.0 * scale),
                        13.0 * scale,
                        DWRITE_FONT_WEIGHT_BOLD,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        WHITE,
                    )?;
                } else {
                    p.glyph('\u{E83F}', circle, 22.0 * scale, rgba(1.0, 1.0, 1.0, 0.8))?;
                }
            } else {
                let fill = if armed {
                    rgba(1.0, 0.35, 0.35, 0.9)
                } else if on {
                    rgba(0.55, 0.6, 1.0, 1.0)
                } else {
                    rgba(1.0, 1.0, 1.0, 0.14)
                };
                p.fill_round_rect(circle, d / 2.0, fill)?;
                p.glyph(*glyph, circle, 21.0 * scale, WHITE)?;
            }
            p.text(
                if armed { "Press again" } else { label },
                (l + 16.0 * scale, b - 50.0 * scale, rr - 8.0 * scale, b - 28.0 * scale),
                16.0 * scale,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                WHITE,
            )?;
            let status = match ctl {
                Ctl::Battery => match battery() {
                    Some((_, true)) => "Charging",
                    Some((_, false)) => "On battery",
                    None => "On AC power",
                },
                Ctl::Vpn if ui.vpn_busy => "Switching",
                _ => match state.flatten() {
                    Some(true) => "On",
                    Some(false) => "Off",
                    None => "Unavailable",
                },
            };
            let devices = if *ctl == Ctl::Bluetooth && on {
                crate::platform::bt_battery::summary(&crate::platform::bt_battery::latest())
            } else {
                None
            };
            let status = match devices {
                Some(levels) => format!("{} · {levels}", crate::ui::i18n::tr(status)),
                None => status.to_string(),
            };
            p.text(
                &status,
                (l + 16.0 * scale, b - 30.0 * scale, rr - 8.0 * scale, b - 10.0 * scale),
                13.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                rgba(1.0, 1.0, 1.0, DIM),
            )?;
        } else {
            let alpha = if armed { 1.0 } else { 0.85 };
            p.glyph(*glyph, (l, t + 10.0 * scale, rr, t + 38.0 * scale), 20.0 * scale, rgba(1.0, 1.0, 1.0, alpha))?;
            p.text(
                if armed { "Press again" } else { label },
                (l, t + 38.0 * scale, rr, b - 6.0 * scale),
                13.5 * scale,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                WHITE,
            )?;
        }
    }
    Ok(())
}

pub(in crate::view) const DEVICES_H: f32 = 44.0;

const DEVICES_SHOWN: usize = 4;

pub(in crate::view) fn devices_row_top(pill: Rect, scale: f32) -> f32 {
    pill.1 + (62.0 + CTL_BIG_H + CTL_GAP + CTL_SMALL_H + CTL_GAP) * scale
}

pub(in crate::view) fn devices_row_space(pill: Rect, scale: f32, ui: &UiState) -> f32 {
    let fits = devices_row_top(pill, scale) + DEVICES_H * scale <= pill.1 + pill.3 - 14.0 * scale;
    if ui.peripherals.is_empty() || !fits { 0.0 } else { (DEVICES_H + CTL_GAP) * scale }
}

pub(in crate::view) fn paint_devices_row(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    if devices_row_space(pill, scale, ui) <= 0.0 {
        return Ok(());
    }
    let (x, _, w, _) = pill;
    let side = SIDE_PAD * scale;
    let top = devices_row_top(pill, scale);
    let panel = (x + side, top, x + w - side, top + DEVICES_H * scale);
    p.fill_round_rect(panel, 16.0 * scale, rgba(1.0, 1.0, 1.0, 0.06))?;
    let extra = ui.peripherals.len().saturating_sub(DEVICES_SHOWN);
    let shown = &ui.peripherals[..ui.peripherals.len().min(DEVICES_SHOWN)];
    let cell_w = (panel.2 - panel.0) / shown.len() as f32;
    let line = rgba(1.0, 1.0, 1.0, 0.09);
    for (i, d) in shown.iter().enumerate() {
        let l = panel.0 + i as f32 * cell_w;
        let r = l + cell_w;
        if i > 0 {
            p.fill_rect((l - 0.5, panel.1 + 10.0 * scale, l + 0.5, panel.3 - 10.0 * scale), line)?;
        }
        let color = if d.low() {
            rgba(1.0, 0.45, 0.45, 1.0)
        } else if d.charging {
            rgba(0.55, 0.9, 0.6, 1.0)
        } else {
            WHITE
        };
        p.glyph(
            d.glyph(),
            (l + 8.0 * scale, panel.1, l + 34.0 * scale, panel.3),
            16.0 * scale,
            rgba(1.0, 1.0, 1.0, 0.85),
        )?;
        let more = if i + 1 == shown.len() && extra > 0 { format!("  +{extra}") } else { String::new() };
        let level = if d.charging { format!("\u{26A1}{}%{more}", d.percent) } else { format!("{}%{more}", d.percent) };
        let level_w = (48.0 + if d.charging { 16.0 } else { 0.0 } + if more.is_empty() { 0.0 } else { 26.0 }) * scale;
        p.text(
            &d.name,
            (l + 36.0 * scale, panel.1, r - level_w, panel.3),
            12.5 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, DIM),
        )?;
        p.text(
            &level,
            (r - level_w, panel.1, r - 10.0 * scale, panel.3),
            13.0 * scale,
            DWRITE_FONT_WEIGHT_BOLD,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
            color,
        )?;
    }
    Ok(())
}
