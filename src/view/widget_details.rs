use crate::platform::metrics::Metric;
use crate::view::*;
use chrono::Datelike;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetCommand {
    Recording,
    Vpn,
    Reminders,
    Steam,
    Agents,
}
pub fn detailed_widget(id: u8) -> bool {
    matches!(tile_kind(id as usize), Some(Tile::Info(..))) || id == 6
}
pub fn widget_command(id: u8) -> Option<(WidgetCommand, &'static str)> {
    Some(match tile_kind(id as usize)? {
        Tile::Action(_, _) if id == 6 => (WidgetCommand::Recording, "Start recording"),
        Tile::Info(_, _, Info::Vpn) => (WidgetCommand::Vpn, "Connect"),
        Tile::Info(_, _, Info::Reminder) => (WidgetCommand::Reminders, "Open reminders"),
        Tile::Info(_, _, Info::Steam) => (WidgetCommand::Steam, "Open Steam"),
        Tile::Info(_, _, Info::Agents) => (WidgetCommand::Agents, "Agents"),
        _ => return None,
    })
}
pub fn widget_command_rect(r: Rect, s: f32) -> Rect {
    (r.0 + 8.0 * s, r.3 - 34.0 * s, r.2 - 8.0 * s, r.3 - 6.0 * s)
}
fn label(p: &Painter, value: &str, r: Rect, s: f32, size: f32, bold: bool, alpha: f32) -> Result<()> {
    p.text(
        value,
        r,
        size * s,
        if bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, alpha),
    )
}
fn graph(p: &Painter, r: Rect, s: f32, values: &[f32], percent: bool) -> Result<()> {
    let max = if percent { 100.0 } else { values.iter().copied().filter(|v| v.is_finite()).fold(1.0, f32::max) };
    let usable: Vec<f32> =
        values.iter().copied().map(|v| if v.is_finite() { v.clamp(0.0, max) } else { 0.0 }).collect();
    p.line((r.0, r.3), (r.2, r.3), s, rgba(1.0, 1.0, 1.0, 0.1))?;
    for (i, v) in usable.windows(2).enumerate() {
        let point = |n: usize, value: f32| {
            (r.0 + (r.2 - r.0) * n as f32 / (usable.len() - 1).max(1) as f32, r.3 - (r.3 - r.1) * value / max)
        };
        p.line(point(i, v[0]), point(i + 1, v[1]), 1.7 * s, acc(0.95))?;
    }
    Ok(())
}
pub fn paint_widget_details(p: &Painter, r: Rect, s: f32, ui: &UiState, id: u8) -> Result<bool> {
    if !ui.cfg.home_widget_details || !detailed_widget(id) || ((r.3 - r.1) < 60.0 * s && (r.2 - r.0) < 150.0 * s) {
        return Ok(false);
    }
    let Some(kind) = tile_kind(id as usize) else { return Ok(false) };
    if matches!(kind, Tile::Info(_, _, Info::Weather)) {
        return Ok(false);
    }
    let (glyph, title) = match kind {
        Tile::Action(g, t) | Tile::Info(g, t, _) => (g, t),
    };
    let (w, h) = ((r.2 - r.0) / s, (r.3 - r.1) / s);
    let x = r.0 + 12.0 * s;
    let right = r.2 - 12.0 * s;
    p.glyph(glyph, (x - 4.0 * s, r.1 + 4.0 * s, x + 20.0 * s, r.1 + 28.0 * s), 15.0 * s, acc(1.0))?;
    label(p, title, (x + 24.0 * s, r.1 + 4.0 * s, right, r.1 + 28.0 * s), s, 10.5, false, 0.7)?;
    if h < 60.0 {
        if let Tile::Info(_, _, info) = kind {
            let metric = matches!(info, Info::Metric(_));
            label(
                p,
                &info_value(ui, info),
                (x + 24.0 * s, r.1 + 23.0 * s, if metric { r.0 + 135.0 * s } else { right }, r.3 - 3.0 * s),
                s,
                13.0,
                true,
                1.0,
            )?;
            if let Info::Metric(metric) = info {
                let at = Metric::ALL.iter().position(|m| *m == metric).unwrap_or(0);
                graph(
                    p,
                    (r.0 + 145.0 * s, r.1 + 12.0 * s, right, r.3 - 10.0 * s),
                    s,
                    &ui.metrics.history[at],
                    !matches!(metric, Metric::NetDown | Metric::NetUp | Metric::Uptime),
                )?;
            }
        } else if id == 6 {
            let value = ui.recording_secs.map_or_else(
                || crate::ui::i18n::tr("Ready to record").into_owned(),
                |s| format!("{:02}:{:02}", s / 60, s % 60),
            );
            label(p, &value, (x + 24.0 * s, r.1 + 23.0 * s, right, r.3 - 3.0 * s), s, 12.0, true, 1.0)?;
        }
        return Ok(true);
    }
    let command = widget_command(id);
    let bottom = r.3 - if command.is_some() { 40.0 * s } else { 8.0 * s };
    let top = r.1 + 30.0 * s;
    match kind {
        Tile::Info(_, _, Info::Metric(metric)) => {
            let value = info_value(ui, Info::Metric(metric));
            label(p, &value, (x, top, right, top + 32.0 * s), s, if w > 150.0 { 25.0 } else { 21.0 }, true, 1.0)?;
            let subtitle = ui.metrics.sample.as_ref().and_then(|sample| match metric {
                Metric::Ram => Some(format!(
                    "{:.1} / {:.0} GB",
                    sample.ram_used as f64 / 1073741824.0,
                    sample.ram_total as f64 / 1073741824.0
                )),
                Metric::Disk => Some(format!(
                    "{:.0} / {:.0} GB",
                    sample.disk_used as f64 / 1073741824.0,
                    sample.disk_total as f64 / 1073741824.0
                )),
                Metric::Uptime => Some(
                    (chrono::Local::now() - chrono::Duration::seconds(sample.uptime.min(3_155_760_000) as i64))
                        .format("%d.%m · %H:%M")
                        .to_string(),
                ),
                _ => None,
            });
            if let Some(subtitle) = subtitle {
                label(
                    p,
                    &subtitle,
                    if w > 150.0 {
                        (r.0 + 132.0 * s, top, right, top + 30.0 * s)
                    } else {
                        (x, top + 29.0 * s, right, top + 49.0 * s)
                    },
                    s,
                    10.5,
                    false,
                    0.6,
                )?;
            }
            if metric != Metric::Uptime {
                let at = Metric::ALL.iter().position(|m| *m == metric).unwrap_or(0);
                let chart_top = if h > 130.0 { top + 65.0 * s } else { r.3 - 24.0 * s };
                graph(
                    p,
                    (x, chart_top, right, r.3 - 8.0 * s),
                    s,
                    &ui.metrics.history[at],
                    !matches!(metric, Metric::NetDown | Metric::NetUp),
                )?;
            }
        }
        Tile::Info(_, _, Info::Clock) => {
            let now = chrono::Local::now();
            label(p, &now.format("%H:%M").to_string(), (x, top, right, top + 36.0 * s), s, 30.0, true, 1.0)?;
            label(
                p,
                &now.format("%d.%m.%Y").to_string(),
                (x, top + 34.0 * s, right, top + 55.0 * s),
                s,
                12.0,
                false,
                0.65,
            )?;
            if h > 170.0 {
                let first = now.date_naive().with_day(1).unwrap();
                let start = first.weekday().num_days_from_monday();
                let mut date = first;
                let step = (right - x) / 7.0;
                let weekdays = if crate::ui::i18n::is_ru() {
                    ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"]
                } else {
                    ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
                };
                for (i, day) in weekdays.into_iter().enumerate() {
                    p.text(
                        day,
                        (x + i as f32 * step, top + 57.0 * s, x + (i + 1) as f32 * step, top + 68.0 * s),
                        8.0 * s,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        rgba(1.0, 1.0, 1.0, 0.5),
                    )?;
                }

                while date.month() == now.month() {
                    let n = start + date.day() - 1;
                    let left = x + (n % 7) as f32 * step;
                    let y = top + (69.0 + (n / 7) as f32 * 17.0) * s;
                    let box_ = (left, y, left + step, y + 16.0 * s);
                    if date.day() == now.day() {
                        p.fill_round_rect(box_, 5.0 * s, acc(0.3))?;
                    }
                    p.text(
                        &date.day().to_string(),
                        box_,
                        9.5 * s,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        WHITE,
                    )?;
                    let Some(next) = date.succ_opt() else {
                        break;
                    };
                    date = next;
                }
            }
        }
        Tile::Info(_, _, Info::Battery) => {
            let battery = battery();
            label(
                p,
                &battery.map_or_else(|| crate::ui::i18n::tr("AC power").into_owned(), |(value, _)| format!("{value}%")),
                (x, top, right, top + 32.0 * s),
                s,
                25.0,
                true,
                1.0,
            )?;
            label(
                p,
                if battery.is_some_and(|v| v.1) {
                    "Charging"
                } else if battery.is_some() {
                    "On battery"
                } else {
                    "No battery"
                },
                (x, top + 31.0 * s, right, top + 52.0 * s),
                s,
                11.0,
                false,
                0.65,
            )?;
            let track = (x, r.3 - 19.0 * s, right, r.3 - 9.0 * s);
            p.fill_round_rect(track, 5.0 * s, rgba(1.0, 1.0, 1.0, 0.1))?;
            if let Some((percent, _)) = battery {
                p.fill_round_rect(
                    (track.0, track.1, track.0 + (track.2 - track.0) * f32::from(percent) / 100.0, track.3),
                    5.0 * s,
                    if percent < 20 { rgba(1.0, 0.4, 0.3, 1.0) } else { acc(1.0) },
                )?;
            }
        }
        Tile::Info(_, _, Info::ScreenTime) => {
            label(p, &info_value(ui, Info::ScreenTime), (x, top, right, top + 32.0 * s), s, 24.0, true, 1.0)?;
            let total = ui.home.screen_secs.unwrap_or(0).max(1) as f32;
            for (i, (name, seconds)) in ui.home.widget_activity.iter().take(if h > 130.0 { 4 } else { 1 }).enumerate() {
                let y = top + (36.0 + i as f32 * 29.0) * s;
                label(p, name, (x, y, right, y + 18.0 * s), s, 10.5, false, 0.7)?;
                let fraction = (*seconds as f32 / total).clamp(0.0, 1.0);
                p.fill_round_rect((x, y + 19.0 * s, x + (right - x) * fraction, y + 22.0 * s), 1.5 * s, acc(0.65))?;
            }
        }
        Tile::Info(_, _, Info::Reminder) => {
            let rows = ((bottom - top) / (25.0 * s)).floor().max(1.0) as usize;
            if ui.remind.list.is_empty() {
                label(p, "No upcoming reminders", (x, top, right, bottom), s, 12.0, false, 0.6)?;
            }
            for (i, (id, text, when)) in ui.remind.list.iter().take(rows).enumerate() {
                let value = ui
                    .live
                    .iter()
                    .find(|v| v.kind == LiveKind::Reminder(*id))
                    .map_or_else(|| when.clone(), |v| v.value());
                let y = top + i as f32 * 25.0 * s;
                label(
                    p,
                    &format!("{value} · {text}"),
                    (x, y, right, y + 24.0 * s),
                    s,
                    11.5,
                    i == 0,
                    if i == 0 { 1.0 } else { 0.65 },
                )?;
            }
        }
        Tile::Info(_, _, Info::Steam) => {
            if let Some((_, name, started)) = &ui.steam.running {
                label(
                    p,
                    name,
                    (x, top, if w > 150.0 { right - 68.0 * s } else { right }, top + 25.0 * s),
                    s,
                    13.0,
                    true,
                    1.0,
                )?;
                if w > 150.0 {
                    label(
                        p,
                        &duration_text(started.elapsed().as_secs() as i64),
                        (right - 62.0 * s, top, right, top + 25.0 * s),
                        s,
                        10.5,
                        false,
                        0.65,
                    )?;
                }
                if bottom - top > 44.0 * s {
                    label(
                        p,
                        &duration_text(started.elapsed().as_secs() as i64),
                        (x, top + 25.0 * s, right, top + 47.0 * s),
                        s,
                        12.0,
                        false,
                        0.6,
                    )?;
                }
            } else {
                label(p, "No game running", (x, top, right, top + 25.0 * s), s, 12.0, false, 0.65)?;
                if h > 130.0 {
                    for (i, game) in ui.steam.games.iter().take(3).enumerate() {
                        let y = top + (29.0 + i as f32 * 24.0) * s;
                        label(p, &game.name, (x, y, right, y + 23.0 * s), s, 11.0, false, 0.7)?;
                    }
                }
            }
        }
        Tile::Info(_, _, Info::Agents) => {
            let rows = ((bottom - top) / (34.0 * s)).floor().max(1.0) as usize;
            if ui.agents.is_empty() {
                label(p, "No active sessions", (x, top, right, bottom), s, 12.0, false, 0.6)?;
            }
            for (i, session) in ui.agents.iter().take(rows).enumerate() {
                let y = top + i as f32 * 34.0 * s;
                let color = match session.state {
                    crate::services::agents::State::Working => acc(1.0),
                    crate::services::agents::State::Waiting => rgba(1.0, 0.75, 0.3, 1.0),
                    crate::services::agents::State::Done => rgba(0.35, 0.85, 0.55, 1.0),
                };
                p.fill_ellipse(x + 3.0 * s, y + 12.0 * s, 3.0 * s, 3.0 * s, color)?;
                label(p, session.agent.name(), (x + 12.0 * s, y, right, y + 16.0 * s), s, 10.5, true, 0.95)?;
                let status = match session.state {
                    crate::services::agents::State::Working => "Working",
                    crate::services::agents::State::Waiting => "Waiting for you",
                    crate::services::agents::State::Done => "Done",
                };
                label(
                    p,
                    &format!("{} · {}", crate::ui::i18n::tr(status), session.project),
                    (x + 12.0 * s, y + 16.0 * s, right, (y + 32.0 * s).min(bottom)),
                    s,
                    9.5,
                    false,
                    0.65,
                )?;
            }
        }
        Tile::Info(_, _, Info::Vpn) => {
            let status = match ui.vpn {
                Some(true) => "Connected",
                Some(false) => "Off",
                None => "Status unavailable",
            };
            label(p, status, (x, top, right, bottom.min(top + 40.0 * s)), s, 19.0, true, 1.0)?;
            if h > 130.0 {
                p.glyph(
                    '\u{E72E}',
                    (x, top + 45.0 * s, right, bottom - 5.0 * s),
                    44.0 * s,
                    if ui.vpn == Some(true) { acc(0.85) } else { rgba(1.0, 1.0, 1.0, 0.25) },
                )?;
            }
        }
        Tile::Action(_, _) if id == 6 => {
            let value = ui.recording_secs.map_or_else(
                || crate::ui::i18n::tr("Ready to record").into_owned(),
                |seconds| format!("{:02}:{:02}", seconds / 60, seconds % 60),
            );
            label(p, &value, (x, top, right, bottom.min(top + 40.0 * s)), s, 22.0, true, 1.0)?;
        }
        _ => {}
    }
    if let Some((command, default)) = command {
        let title = match command {
            WidgetCommand::Vpn if ui.vpn_busy => "Please wait…",
            WidgetCommand::Vpn if ui.vpn == Some(true) => "Disconnect",
            WidgetCommand::Recording if ui.recording_secs.is_some() => "Stop recording",
            _ => default,
        };
        let button = widget_command_rect(r, s);
        p.fill_round_rect(button, 9.0 * s, acc(0.2))?;
        label(p, title, (button.0 + 8.0 * s, button.1, button.2 - 6.0 * s, button.3), s, 11.5, true, 0.95)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_actions_stay_compact_and_data_widgets_keep_their_controls() {
        let mut ui = UiState::new();
        ui.cfg.home_actions = vec![0, 8, 21];
        ui.cfg.home_widget_sizes.insert(0, crate::data::home_widgets::WidgetSize::Wide4);
        assert_eq!(widget_size(&ui, 0), crate::data::home_widgets::WidgetSize::Tile);
        assert!(!can_resize_widget(&ui, 0, crate::data::home_widgets::WidgetSize::Wide2));
        ui.cfg.home_widget_details = false;
        assert_eq!(widget_size(&ui, 0), crate::data::home_widgets::WidgetSize::Wide4);
        ui.cfg.home_widget_details = true;
        ui.cfg.home_actions = vec![21];
        ui.cfg.home_weather_widget = false;
        ui.cfg.island_animations = false;
        ui.cfg.home_widget_sizes.insert(21, crate::data::home_widgets::WidgetSize::Wide4);
        for scale in [0.8, 1.0, 1.25, 1.5, 2.0] {
            let pill = (0.0, 0.0, 640.0 * scale, 380.0 * scale);
            let (_, r, _) = home_widget_rects(pill, scale, &ui)[0];
            let button = widget_command_rect(r, scale);
            assert!(matches!(
                hit_home_widget(pill, scale, &ui, (button.0 + button.2) / 2.0, (button.1 + button.3) / 2.0),
                Some(HomeHit::WidgetCommand(WidgetCommand::Vpn))
            ));
            assert!(hit_home_widget(pill, scale, &ui, r.0 + 30.0 * scale, r.1 + 30.0 * scale).is_none());
        }
    }
    #[test]
    fn a_countdown_widget_replaces_the_duplicate_activity_tile() {
        let mut ui = UiState::new();
        ui.cfg.home_actions = vec![18];
        ui.cfg.home_widget_sizes.insert(18, crate::data::home_widgets::WidgetSize::Wide4);
        ui.live.push(LiveActivity { kind: LiveKind::Reminder(1), title: "Tea".into(), seconds: Some(180) });
        assert_eq!(live_tile_count(&ui), 0);
        ui.cfg.home_widget_details = false;
        assert_eq!(live_tile_count(&ui), 1);
        ui.cfg.home_widget_details = true;
        ui.cfg.home_widget_sizes.clear();
        assert_eq!(live_tile_count(&ui), 1);
    }
}
