use crate::data::settings::HOME_ACTION_COUNT;
use crate::platform::metrics::Metric;
use crate::view::*;

#[derive(Default)]
pub struct TrackTransition {
    from: Vec<(crate::platform::media::Media, f32, f32)>,
    start: Option<std::time::Instant>,
}

impl TrackTransition {
    #[cfg(test)]
    pub fn preview_progress(&mut self, progress: f32) {
        self.start = Some(std::time::Instant::now() - std::time::Duration::from_secs_f32(progress * 0.24));
    }
    fn progress(&self) -> f32 {
        self.start.map_or(1.0, |start| {
            let t = (start.elapsed().as_secs_f32() / 0.24).clamp(0.0, 1.0);
            1.0 - (1.0 - t).powi(3)
        })
    }

    fn colour(&self, current: &crate::platform::media::Media) -> (f32, f32, f32) {
        let rgb = |media: &crate::platform::media::Media| {
            let (r, g, b) = media.accent.unwrap_or((128, 128, 138));
            (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
        };
        let progress = self.progress();
        let (r, g, b) = rgb(current);
        let mut result = (r * progress, g * progress, b * progress);
        for (media, weight, _) in &self.from {
            let (r, g, b) = rgb(media);
            let w = weight * (1.0 - progress);
            result.0 += r * w;
            result.1 += g * w;
            result.2 += b * w;
        }
        result
    }

    pub fn active(&self) -> bool {
        self.start.is_some_and(|start| start.elapsed().as_millis() < 240)
    }
    pub fn finish(&mut self) {
        self.from.clear();
        self.start = None;
    }

    pub fn retarget(&mut self, previous: &crate::platform::media::Media, enabled: bool) {
        if !enabled || previous.title.is_empty() {
            self.finish();
            return;
        }
        let progress = self.progress();
        for (_, weight, text) in &mut self.from {
            *weight *= 1.0 - progress;
            *text *= (1.0 - 2.0 * progress).max(0.0);
        }
        self.from.retain(|(_, weight, text)| *weight > 0.001 || *text > 0.001);
        if progress > 0.001 {
            self.from.push((previous.clone(), progress, (2.0 * progress - 1.0).max(0.0)));
        }
        if self.from.len() > 16 {
            self.from.sort_by(|a, b| b.1.total_cmp(&a.1));
            self.from.truncate(16);
            let total: f32 = self.from.iter().map(|v| v.1).sum();
            for (_, weight, _) in &mut self.from {
                *weight /= total;
            }
        }
        self.start = Some(std::time::Instant::now());
    }
}

pub struct HomeUi {
    pub track_transition: TrackTransition,
    pub preview_timeline: Option<(f32, f32)>,
    pub vol: f32,
    pub mixer_open: bool,
    pub mixer_motion: Option<(f32, std::time::Instant)>,
    pub device: String,
    pub dev_menu: Option<(Vec<crate::platform::volume::Device>, usize)>,
    pub dock: Vec<ShelfItem>,
    pub dock_open: bool,
    pub reorder: Option<Reorder>,
    pub dock_press: Option<(usize, i32, i32)>,
    pub tile_press: Option<(usize, i32, i32)>,
    pub tile_reorder: Option<Reorder>,
    pub seek: Option<f32>,
    pub vol_track: bool,
    pub app_vol: f32,
    pub hover_act: Option<usize>,
    pub edit: bool,
    pub edit_scroll: usize,
    pub screen_secs: Option<i64>,
}

impl Default for HomeUi {
    fn default() -> Self {
        Self {
            track_transition: Default::default(),
            preview_timeline: None,
            vol: 0.5,
            mixer_open: false,
            mixer_motion: None,
            device: Default::default(),
            dev_menu: Default::default(),
            dock: Default::default(),
            dock_open: false,
            reorder: None,
            dock_press: None,
            tile_press: None,
            tile_reorder: None,
            seek: Default::default(),
            vol_track: Default::default(),
            app_vol: 1.0,
            hover_act: Default::default(),
            edit: false,
            edit_scroll: 0,
            screen_secs: None,
        }
    }
}

pub const HOME_ACTIONS: [(char, &str); 8] = [
    ('\u{E7A8}', "Area"),
    ('\u{E740}', "Full screen"),
    ('\u{E8E9}', "Text from screen"),
    ('\u{E8B9}', "QR code"),
    ('\u{EF3C}', "Color picker"),
    ('\u{ED60}', "Clean text"),
    ('\u{E7C8}', "Screen recording"),
    ('\u{E721}', "Command"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Info {
    Metric(Metric),
    ScreenTime,
    Weather,
    Battery,
    Reminder,
    Steam,
    Agents,
    Vpn,
    Clock,
}

pub const HOME_INFO: [(char, &str, Info); 15] = [
    ('\u{E9D9}', "Processor", Info::Metric(Metric::Cpu)),
    ('\u{E964}', "Memory", Info::Metric(Metric::Ram)),
    ('\u{E7F4}', "Graphics card", Info::Metric(Metric::Gpu)),
    ('\u{EDA2}', "System disk", Info::Metric(Metric::Disk)),
    ('\u{E896}', "Download speed", Info::Metric(Metric::NetDown)),
    ('\u{E898}', "Upload speed", Info::Metric(Metric::NetUp)),
    ('\u{E916}', "Uptime", Info::Metric(Metric::Uptime)),
    ('\u{E823}', "Time at the computer", Info::ScreenTime),
    ('\u{E706}', "Weather", Info::Weather),
    ('\u{E83F}', "Battery", Info::Battery),
    ('\u{EA8F}', "Next reminder", Info::Reminder),
    ('\u{E7FC}', "Steam", Info::Steam),
    ('\u{E756}', "Coding agents", Info::Agents),
    ('\u{E72E}', "VPN", Info::Vpn),
    ('\u{E787}', "Time and date", Info::Clock),
];

const _: () = assert!(HOME_ACTIONS.len() + HOME_INFO.len() == crate::data::settings::HOME_ACTION_COUNT);

pub const MAX_HOME_TILES: usize = 8;

pub enum Tile {
    Action(char, &'static str),
    Info(char, &'static str, Info),
}

pub fn tile_kind(id: usize) -> Option<Tile> {
    if let Some(&(glyph, label)) = HOME_ACTIONS.get(id) {
        return Some(Tile::Action(glyph, label));
    }
    HOME_INFO.get(id.checked_sub(HOME_ACTIONS.len())?).map(|&(glyph, label, info)| Tile::Info(glyph, label, info))
}

pub fn tile_needs_metrics(id: usize) -> bool {
    matches!(tile_kind(id), Some(Tile::Info(_, _, Info::Metric(_))))
}

fn fit(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    let head: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{head}\u{2026}")
}

pub fn info_value(ui: &UiState, info: Info) -> String {
    let tr = |s: &str| crate::ui::i18n::tr(s).into_owned();
    match info {
        Info::Metric(m) => ui.metrics.sample.as_ref().map_or_else(|| "\u{2026}".to_string(), |s| m.text(s)),
        Info::ScreenTime => match ui.home.screen_secs {
            _ if !ui.cfg.activity_track => tr("Tracking off"),
            Some(secs) => duration_text(secs),
            None => "\u{2026}".to_string(),
        },
        Info::Weather => ui.weather.as_ref().map_or_else(
            || "\u{2014}".to_string(),
            |w| format!("{}\u{B0} {}", w.temp.round() as i32, tr(w.sky.name())),
        ),
        Info::Battery => match battery() {
            Some((percent, true)) => format!("{percent}% \u{26A1}"),
            Some((percent, false)) => format!("{percent}%"),
            None => tr("No battery"),
        },
        Info::Reminder => {
            ui.remind.list.first().map_or_else(|| tr("None"), |(_, text, when)| format!("{when} \u{B7} {text}"))
        }
        Info::Steam => ui.steam.running.as_ref().map_or_else(|| tr("Not playing"), |(_, name, _)| name.clone()),
        Info::Agents => {
            use crate::services::agents::State;
            let waiting = ui.agents.iter().filter(|s| s.state == State::Waiting).count();
            let working = ui.agents.iter().filter(|s| s.state == State::Working).count();
            match (waiting, working) {
                (0, 0) => tr("Idle"),
                (0, n) => format!("{n} {}", tr("working")),
                (n, _) => format!("{n} {}", tr("waiting for you")),
            }
        }
        Info::Vpn => match ui.vpn {
            Some(true) => tr("Connected"),
            Some(false) => tr("Off"),
            None => "\u{2014}".to_string(),
        },
        Info::Clock => {
            let c = clock();
            format!("{} \u{B7} {}", c.hh_mm, c.date)
        }
    }
}

pub(in crate::view) const HOME_TOP: f32 = 58.0;

pub(in crate::view) const GRID_COL_W: f32 = 124.0;

pub(in crate::view) const GRID_ROW_H: f32 = 46.0;

pub(in crate::view) const GRID_GAP: f32 = 8.0;

pub fn home_wide(ui: &UiState) -> bool {
    !ui.home.edit && ui.cfg.home_actions.is_empty() && ui.notifications.preview.is_none()
}

pub(in crate::view) fn slot_action(ui: &UiState, slot: usize) -> Option<usize> {
    if ui.home.edit {
        let id = ui.home.edit_scroll * 2 + slot;
        (id < HOME_ACTION_COUNT).then_some(id)
    } else {
        slot.checked_sub(notification_tile_count(ui))
            .and_then(|i| ui.cfg.home_actions.get(i))
            .map(|&id| usize::from(id))
    }
}

fn slot_count(ui: &UiState) -> usize {
    if ui.home.edit {
        HOME_ACTION_COUNT.saturating_sub(ui.home.edit_scroll * 2).min(MAX_HOME_TILES)
    } else {
        (ui.cfg.home_actions.len() + notification_tile_count(ui)).min(MAX_HOME_TILES)
    }
}

pub fn home_edit_rows() -> usize {
    HOME_ACTION_COUNT.div_ceil(2).saturating_sub(MAX_HOME_TILES / 2)
}

pub(in crate::view) fn grid_left(pill: Rect, scale: f32, wide: bool) -> f32 {
    let (x, _, w, _) = pill;
    let right = x + w - SIDE_PAD * scale + 4.0 * scale;
    if wide { right + 12.0 * scale } else { right - (2.0 * GRID_COL_W + GRID_GAP) * scale }
}

pub fn action_tile(pill: Rect, scale: f32, i: usize) -> Rect {
    let (_, y, _, _) = pill;
    let (col, row) = ((i % 2) as f32, (i / 2) as f32);
    let left = grid_left(pill, scale, false) + col * (GRID_COL_W + GRID_GAP) * scale;
    let top = y + (HOME_TOP + row * (GRID_ROW_H + GRID_GAP)) * scale;
    (left, top, left + GRID_COL_W * scale, top + GRID_ROW_H * scale)
}

pub(in crate::view) fn media_card(pill: Rect, scale: f32, wide: bool) -> Rect {
    let (x, y, _, _) = pill;
    let left = x + (SIDE_PAD - 4.0) * scale;
    (left, y + HOME_TOP * scale, grid_left(pill, scale, wide) - 12.0 * scale, y + (HOME_TOP + 154.0) * scale)
}

pub(in crate::view) fn volume_row(pill: Rect, scale: f32, wide: bool) -> Rect {
    let card = media_card(pill, scale, wide);
    (card.0, card.3 + 8.0 * scale, card.2, card.3 + (8.0 + 46.0) * scale)
}

pub fn volume_track(pill: Rect, scale: f32, wide: bool) -> (f32, f32, f32) {
    let row = volume_row(pill, scale, wide);
    let length = if wide { 250.0 } else { 150.0 };
    (row.0 + 44.0 * scale, row.0 + length * scale, (row.1 + row.3) / 2.0)
}

pub(in crate::view) fn device_chip(pill: Rect, scale: f32, wide: bool) -> Rect {
    let row = volume_row(pill, scale, wide);
    let left = row.0 + if wide { 292.0 } else { 192.0 } * scale;
    let right = (row.2 - (8.0 + EDIT_BTN + 6.0) * scale).max(left + 40.0 * scale);
    (left, row.1 + 8.0 * scale, right, row.3 - 8.0 * scale)
}

const EDIT_BTN: f32 = 30.0;

pub(in crate::view) fn edit_button(pill: Rect, scale: f32, wide: bool) -> Rect {
    let row = volume_row(pill, scale, wide);
    let size = EDIT_BTN * scale;
    let right = row.2 - 8.0 * scale;
    (right - size, (row.1 + row.3) / 2.0 - size / 2.0, right, (row.1 + row.3) / 2.0 + size / 2.0)
}

pub(in crate::view) const MENU_ROW_H: f32 = 30.0;

pub(in crate::view) const MENU_MAX_ROWS: usize = 5;

pub fn menu_visible(count: usize) -> usize {
    count.min(MENU_MAX_ROWS)
}

pub fn menu_rect(pill: Rect, scale: f32, count: usize, wide: bool) -> Rect {
    let chip = device_chip(pill, scale, wide);
    let width = 300.0 * scale;
    let right = chip.2;
    let left = (right - width).max(pill.0 + 8.0 * scale);
    let bottom = chip.1 - 8.0 * scale;
    (left, bottom - (menu_visible(count) as f32 * MENU_ROW_H + 12.0) * scale, right, bottom)
}

pub(in crate::view) fn menu_row(frame: Rect, scale: f32, i: usize) -> Rect {
    let top = frame.1 + (6.0 + i as f32 * MENU_ROW_H) * scale;
    (frame.0 + 6.0 * scale, top, frame.2 - 6.0 * scale, top + MENU_ROW_H * scale)
}

pub fn hit_menu(pill: Rect, scale: f32, px: f32, py: f32, count: usize, wide: bool) -> Option<Option<usize>> {
    let frame = menu_rect(pill, scale, count, wide);
    if !inside(frame, px, py) {
        return None;
    }
    Some((0..menu_visible(count)).find(|&i| inside(menu_row(frame, scale, i), px, py)))
}

pub(in crate::view) fn paint_menu(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let Some((items, scroll)) = &ui.home.dev_menu else { return Ok(()) };
    let frame = menu_rect(pill, scale, items.len(), home_wide(ui));
    p.fill_round_rect(frame, 16.0 * scale, rgba(0.07, 0.07, 0.09, 0.98))?;
    p.stroke_round_rect(frame, 16.0 * scale, 1.0 * scale, rgba(1.0, 1.0, 1.0, 0.14))?;
    for (i, d) in items.iter().skip(*scroll).take(menu_visible(items.len())).enumerate() {
        let row = menu_row(frame, scale, i);
        if ui.home.hover_act == Some(300 + i) {
            p.fill_round_rect(row, 10.0 * scale, rgba(1.0, 1.0, 1.0, 0.1))?;
        }
        if d.current {
            p.glyph('\u{E73E}', (row.0 + 2.0 * scale, row.1, row.0 + 28.0 * scale, row.3), 13.0 * scale, WHITE)?;
        }
        p.glyph(
            if d.capture { '\u{E720}' } else { '\u{E767}' },
            (row.0 + 28.0 * scale, row.1, row.0 + 52.0 * scale, row.3),
            13.0 * scale,
            rgba(1.0, 1.0, 1.0, 0.65),
        )?;
        p.text(
            &d.name,
            (row.0 + 56.0 * scale, row.1, row.2 - 8.0 * scale, row.3),
            13.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        )?;
    }
    Ok(())
}

pub(in crate::view) const DOCK_TOP: f32 = HOME_TOP + 4.0 * (GRID_ROW_H + GRID_GAP) + 4.0;

pub(in crate::view) const DOCK_ICON: f32 = 40.0;

pub(in crate::view) const DOCK_GAP: f32 = 12.0;

pub(in crate::view) fn dock_zone(pill: Rect, scale: f32, compact: bool) -> Rect {
    let (x, y, w, _) = pill;
    (
        x + (SIDE_PAD - 4.0) * scale,
        y + DOCK_TOP * scale,
        x + w - (SIDE_PAD - 4.0) * scale,
        y + if compact { 326.0 } else { 372.0 } * scale,
    )
}

pub(in crate::view) fn strip_zone(pill: Rect, scale: f32) -> Rect {
    let (x, y, w, _) = pill;
    (x + (SIDE_PAD - 4.0) * scale, y + 332.0 * scale, x + w - (SIDE_PAD - 4.0) * scale, y + 372.0 * scale)
}

pub(in crate::view) fn dock_capacity(pill: Rect, scale: f32, compact: bool) -> usize {
    let zone = dock_zone(pill, scale, compact);
    let room = (zone.2 - zone.0) / scale - 28.0 + DOCK_GAP;
    ((room / (DOCK_ICON + DOCK_GAP)).floor() as usize).saturating_sub(1).max(1)
}

pub struct DockLayout {
    pub slots: Vec<Rect>,
    pub button: Rect,
    pub more: bool,
    pub zone: Rect,
    pub close: Option<Rect>,
    pub open: bool,
}

pub fn dock_layout(pill: Rect, scale: f32, ui: &UiState) -> DockLayout {
    let compact = discord_strip(ui);
    let n = ui.home.dock.len();
    let cap = dock_capacity(pill, scale, compact);
    let step = (DOCK_ICON + DOCK_GAP) * scale;
    let icon = DOCK_ICON * scale;
    let row_zone = dock_zone(pill, scale, compact);
    if ui.home.dock_open && n > cap {
        let top = pill.1 + HOME_TOP * scale;
        let zone = (row_zone.0, top, row_zone.2, row_zone.3);
        let rows = (((zone.3 - zone.1) / scale - 40.0) / (DOCK_ICON + DOCK_GAP)).floor().max(1.0) as usize;
        let shown = n.min(rows * cap - 1);
        let slot = |i: usize| {
            let (col, row) = ((i % cap) as f32, (i / cap) as f32);
            let left = zone.0 + 14.0 * scale + col * step;
            let t = zone.1 + 32.0 * scale + row * step;
            (left, t, left + icon, t + icon)
        };
        let close = (zone.2 - 40.0 * scale, zone.1 + 4.0 * scale, zone.2 - 8.0 * scale, zone.1 + 32.0 * scale);
        return DockLayout {
            slots: (0..shown).map(slot).collect(),
            button: slot(shown),
            more: false,
            zone,
            close: Some(close),
            open: true,
        };
    }
    let shown = n.min(cap);
    let slot = |i: usize| {
        let left = row_zone.0 + 14.0 * scale + i as f32 * step;
        let t = row_zone.1 + if compact { 4.0 } else { 12.0 } * scale;
        (left, t, left + icon, t + icon)
    };
    DockLayout {
        slots: (0..shown).map(slot).collect(),
        button: slot(shown),
        more: n > cap,
        zone: row_zone,
        close: None,
        open: false,
    }
}

pub fn dock_cross(slot: Rect, scale: f32) -> Rect {
    (slot.2 - 11.0 * scale, slot.1 - 5.0 * scale, slot.2 + 3.0 * scale, slot.1 + 9.0 * scale)
}

fn hovered_dock(ui: &UiState) -> Option<usize> {
    match ui.home.hover_act {
        Some(a @ 600..700) => Some(a - 600),
        Some(a @ 800..900) => Some(a - 800),
        _ => None,
    }
}

pub(in crate::view) fn hit_dock(pill: Rect, scale: f32, px: f32, py: f32, ui: &UiState) -> Option<HomeHit> {
    let layout = dock_layout(pill, scale, ui);
    if layout.close.is_some_and(|c| inside(c, px, py)) {
        return Some(HomeHit::DockMore);
    }
    if let Some(i) = hovered_dock(ui).filter(|_| ui.home.reorder.is_none())
        && let Some(&slot) = layout.slots.get(i)
        && inside(dock_cross(slot, scale), px, py)
    {
        return Some(HomeHit::DockUnpin(i));
    }
    if let Some(i) = layout.slots.iter().position(|&s| inside(s, px, py)) {
        return Some(HomeHit::Dock(i));
    }
    if inside(layout.button, px, py) {
        return Some(if layout.more { HomeHit::DockMore } else { HomeHit::DockAdd });
    }
    None
}

pub(in crate::view) fn dock_name(path: &str) -> String {
    let p = std::path::Path::new(path);
    p.file_stem().or_else(|| p.file_name()).map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

pub(in crate::view) fn paint_dock(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let compact = discord_strip(ui);
    let layout = dock_layout(pill, scale, ui);
    let zone = layout.zone;
    if layout.open {
        let (r, g, b) = crate::ui::theme::panel();
        p.fill_round_rect(zone, 18.0 * scale, rgba(r, g, b, 1.0))?;
    }
    p.fill_round_rect(zone, 18.0 * scale, rgba(1.0, 1.0, 1.0, if layout.open { 0.07 } else { 0.05 }))?;
    let reorder = ui.home.reorder.as_ref();
    let mut hovered: Option<&str> = None;
    let paint_icon = |item: &ShelfItem, slot: Rect| -> Result<()> {
        let inner = 3.0 * scale;
        let icon_rect = (slot.0 + inner, slot.1 + inner, slot.2 - inner, slot.3 - inner);
        match crate::platform::apps::icon_for_path(&item.path) {
            Some(icon) => p.image(icon_rect, 6.0 * scale, icon.side, &icon.bgra),
            None => p.glyph('\u{E8A5}', slot, 22.0 * scale, rgba(1.0, 1.0, 1.0, 0.7)),
        }
    };
    for (i, item) in ui.home.dock.iter().take(layout.slots.len()).enumerate() {
        if reorder.is_some_and(|r| r.from == i) {
            continue;
        }
        let mut slot = layout.slots[i];
        if let Some(r) = reorder.filter(|r| r.pos.len() == layout.slots.len()) {
            let (w, h) = (slot.2 - slot.0, slot.3 - slot.1);
            slot = (r.pos[i].0, r.pos[i].1, r.pos[i].0 + w, r.pos[i].1 + h);
        }
        if reorder.is_none() && hovered_dock(ui) == Some(i) {
            let g = 5.0 * scale;
            p.fill_round_rect(
                (slot.0 - g, slot.1 - g, slot.2 + g, slot.3 + g),
                14.0 * scale,
                rgba(1.0, 1.0, 1.0, 0.13),
            )?;
            hovered = Some(&item.path);
        }
        paint_icon(item, slot)?;
        if reorder.is_none() && hovered_dock(ui) == Some(i) {
            let cross = dock_cross(slot, scale);
            let on = ui.home.hover_act == Some(800 + i);
            let (cx, cy) = ((cross.0 + cross.2) / 2.0, (cross.1 + cross.3) / 2.0);
            let r = 7.0 * scale;
            p.fill_ellipse(cx, cy, r, r, rgba(0.1, 0.1, 0.12, 0.92))?;
            p.fill_ellipse(cx, cy, r, r, rgba(1.0, 1.0, 1.0, if on { 0.28 } else { 0.1 }))?;
            p.glyph('\u{E711}', cross, 8.5 * scale, rgba(1.0, 1.0, 1.0, if on { 1.0 } else { 0.8 }))?;
        }
    }
    if let Some(r) = reorder
        && let (Some(item), Some(&base)) = (ui.home.dock.get(r.from), layout.slots.get(r.from))
    {
        let size = (base.2 - base.0, base.3 - base.1);
        let at = r.dragged_rect(size);
        let g = 6.0 * scale;
        p.fill_round_rect((at.0 - g, at.1 - g, at.2 + g, at.3 + g), 16.0 * scale, rgba(1.0, 1.0, 1.0, 0.2))?;
        paint_icon(item, at)?;
    }
    let add = layout.button;
    let add_hover = ui.home.hover_act == Some(700);
    let ring = rgba(1.0, 1.0, 1.0, if add_hover { 0.55 } else { 0.28 });
    p.stroke_round_rect(add, 12.0 * scale, 1.2 * scale, ring)?;
    let sign_alpha = if add_hover { 0.9 } else { 0.55 };
    if layout.more {
        p.glyph(
            '\u{E710}',
            (add.0, add.1 - 5.0 * scale, add.2, add.3 - 5.0 * scale),
            14.0 * scale,
            rgba(1.0, 1.0, 1.0, sign_alpha),
        )?;
        for k in 0..3 {
            let cx = (add.0 + add.2) / 2.0 + (k as f32 - 1.0) * 6.0 * scale;
            p.fill_ellipse(cx, add.3 - 9.0 * scale, 1.5 * scale, 1.5 * scale, rgba(1.0, 1.0, 1.0, sign_alpha))?;
        }
    } else {
        p.glyph('\u{E710}', add, 16.0 * scale, rgba(1.0, 1.0, 1.0, sign_alpha))?;
    }
    if let Some(close) = layout.close {
        let hover = ui.home.hover_act == Some(701);
        if hover {
            p.fill_round_rect(close, 10.0 * scale, rgba(1.0, 1.0, 1.0, 0.12))?;
        }
        p.glyph('\u{E711}', close, 13.0 * scale, rgba(1.0, 1.0, 1.0, if hover { 1.0 } else { 0.6 }))?;
        p.text(
            "Pinned",
            (zone.0 + 14.0 * scale, zone.1 + 4.0 * scale, zone.2 - 48.0 * scale, zone.1 + 32.0 * scale),
            12.5 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.6),
        )?;
    }
    if ui.home.dock.is_empty() {
        p.text(
            "Drop shortcuts, folders or files here",
            (add.2 + 14.0 * scale, add.1, zone.2 - 12.0 * scale, add.3),
            13.5 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.5),
        )?;
    }
    if !compact || layout.open {
        let name = hovered.map(dock_name).unwrap_or_default();
        let line = (zone.0 + 14.0 * scale, zone.3 - 30.0 * scale, zone.2 - 14.0 * scale, zone.3 - 8.0 * scale);
        if !name.is_empty() {
            p.text(&name, line, 13.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, WHITE)?;
        } else if !ui.home.dock.is_empty() && reorder.is_none() {
            p.text(
                "Click to open, drag to reorder, right-click for more",
                line,
                12.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                rgba(1.0, 1.0, 1.0, 0.4),
            )?;
        }
    }
    Ok(())
}

pub fn dock_path(ui: &UiState, i: usize) -> Option<&str> {
    ui.home.dock.get(i).map(|d| d.path.as_str())
}

pub(in crate::view) fn media_btn(pill: Rect, scale: f32, i: usize, wide: bool) -> Rect {
    let card = media_card(pill, scale, wide);
    let (bw, gap) = ([34.0, 42.0, 34.0][i] * scale, 22.0 * scale);
    let total = 34.0 * 2.0 * scale + 42.0 * scale + 2.0 * gap;
    let mut left = card.0 + (card.2 - card.0 - total) / 2.0;
    for k in 0..i {
        left += [34.0, 42.0, 34.0][k] * scale + gap;
    }
    let cy = card.3 - 25.0 * scale;
    (left, cy - bw / 2.0, left + bw, cy + bw / 2.0)
}

#[derive(Clone, Copy, PartialEq)]
pub enum HomeHit {
    Notification,
    NotificationExpand,
    Mixer(MixerHit),
    Media(usize),
    Action(usize),
    Volume,
    Device,
    Extra(usize),
    DiscordMute,
    DiscordDeaf,
    DiscordOverlay,
    Dock(usize),
    DockUnpin(usize),
    DockAdd,
    DockMore,
    Seek(f32),
    VolumeIcon,
    Edit,
    Player,
    Wave,
}

pub fn hit_home(pill: Rect, scale: f32, px: f32, py: f32, ui: &UiState) -> Option<HomeHit> {
    if let Some(rect) = home_notification_rect(pill, scale, ui)
        && inside(rect, px, py)
        && !ui.home.dock_open
        && ui.home.dev_menu.is_none()
    {
        return Some(if inside(home_notification_expand(rect, scale), px, py) {
            HomeHit::NotificationExpand
        } else {
            HomeHit::Notification
        });
    }
    if ui.home.mixer_open && inside(media_card(pill, scale, home_wide(ui)), px, py) {
        return hit_home_mixer(pill, scale, ui, px, py).map(HomeHit::Mixer);
    }
    let (has_media, spotify, strip) = (!ui.media.title.is_empty(), spotify_active(ui), discord_strip(ui));
    let wide = home_wide(ui);
    if let Some(hit) = hit_dock(pill, scale, px, py, ui) {
        return Some(hit);
    }
    if dock_layout(pill, scale, ui).open {
        return None;
    }
    if strip {
        for (i, hit) in [HomeHit::DiscordDeaf, HomeHit::DiscordMute, HomeHit::DiscordOverlay].into_iter().enumerate() {
            if inside(strip_toggle(pill, scale, i), px, py) {
                return Some(hit);
            }
        }
    }
    if wave_active(ui) && inside(wave_link(pill, scale, ui), px, py) {
        return Some(HomeHit::Wave);
    }
    if spotify && let Some(i) = (0..3).find(|&i| inside(extra_btn(pill, scale, i, wide), px, py)) {
        return Some(HomeHit::Extra(i));
    }
    if has_media && ui.media.players >= 2 && inside(player_btn(pill, scale, wide), px, py) {
        return Some(HomeHit::Player);
    }
    if has_media && let Some(i) = (0..3).find(|&i| inside(media_btn(pill, scale, i, wide), px, py)) {
        return Some(HomeHit::Media(i));
    }
    if has_media {
        let (l, r, y) = seek_track(pill, scale, wide);
        if px >= l - 6.0 * scale && px <= r + 6.0 * scale && (py - y).abs() < 10.0 * scale {
            return Some(HomeHit::Seek(((px - l) / (r - l).max(1.0)).clamp(0.0, 1.0)));
        }
    }
    if let Some(id) = (0..slot_count(ui))
        .find(|&slot| inside(action_tile(pill, scale, slot), px, py))
        .and_then(|slot| slot_action(ui, slot))
    {
        return Some(HomeHit::Action(id));
    }
    if inside(edit_button(pill, scale, wide), px, py) {
        return Some(HomeHit::Edit);
    }
    let row = volume_row(pill, scale, wide);
    if inside((row.0 + 2.0 * scale, row.1, row.0 + 42.0 * scale, row.3), px, py) {
        return Some(HomeHit::VolumeIcon);
    }
    let (tl, tr, cy) = volume_track(pill, scale, wide);
    if px >= tl - 10.0 * scale && px <= tr + 10.0 * scale && (py - cy).abs() < 14.0 * scale {
        return Some(HomeHit::Volume);
    }
    inside(device_chip(pill, scale, wide), px, py).then_some(HomeHit::Device)
}

pub fn seek_track(pill: Rect, scale: f32, wide: bool) -> (f32, f32, f32) {
    let card = media_card(pill, scale, wide);
    (card.0 + 16.0 * scale, card.2 - 16.0 * scale, card.1 + 100.0 * scale)
}

pub(in crate::view) fn extra_btn(pill: Rect, scale: f32, i: usize, wide: bool) -> Rect {
    let card = media_card(pill, scale, wide);
    let size = 30.0 * scale;
    match i {
        0 => {
            let prev = media_btn(pill, scale, 0, wide);
            let cy = (prev.1 + prev.3) / 2.0;
            (prev.0 - 18.0 * scale - size, cy - size / 2.0, prev.0 - 18.0 * scale, cy + size / 2.0)
        }
        1 => {
            let next = media_btn(pill, scale, 2, wide);
            let cy = (next.1 + next.3) / 2.0;
            (next.2 + 18.0 * scale, cy - size / 2.0, next.2 + 18.0 * scale + size, cy + size / 2.0)
        }
        _ => {
            let right = card.2 - 14.0 * scale - size - 6.0 * scale;
            (right - size, card.1 + 10.0 * scale, right, card.1 + 10.0 * scale + size)
        }
    }
}

pub(in crate::view) fn player_btn(pill: Rect, scale: f32, wide: bool) -> Rect {
    let card = media_card(pill, scale, wide);
    let size = 30.0 * scale;
    (card.2 - 14.0 * scale - size, card.1 + 10.0 * scale, card.2 - 14.0 * scale, card.1 + 10.0 * scale + size)
}

pub fn wave_active(ui: &UiState) -> bool {
    ui.yandex.state.active && ui.media.source == "Night Island · Yandex Music"
}

pub fn wave_link(pill: Rect, s: f32, ui: &UiState) -> Rect {
    let card = media_card(pill, s, home_wide(ui));
    (card.0 + (16.0 + 72.0 * cover_aspect(ui) + 14.0) * s, card.1 + 10.0 * s, card.2 - 16.0 * s, card.1 + 32.0 * s)
}

pub fn spotify_active(ui: &UiState) -> bool {
    ui.spotify.status == crate::services::spotify::Status::Connected
        && ui.media.source == "SPOTIFY"
        && !ui.media.title.is_empty()
}

pub(in crate::view) fn mmss(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

pub(in crate::view) fn paint_home(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let wide = home_wide(ui);
    let card = media_card(pill, scale, wide);
    let wave = wave_active(ui);
    let colours = ui.yandex.colours();
    if wave {
        p.fill_round_gradient(
            card,
            20.0 * scale,
            wave_colour(colours.card, 1.0),
            wave_colour(colours.background, 1.0),
        )?;
    } else {
        p.fill_round_rect(card, 20.0 * scale, rgba(1.0, 1.0, 1.0, 0.07))?;
        if !ui.media.title.is_empty() {
            let colour = ui.home.track_transition.colour(&ui.media);
            p.fill_round_gradient(
                card,
                20.0 * scale,
                rgba(colour.0, colour.1, colour.2, 0.11),
                rgba(colour.0, colour.1, colour.2, 0.01),
            )?;
        }
    }
    let (cl, ct, cr, cb) = card;
    let pad = 16.0 * scale;
    if ui.media.title.is_empty() {
        let clock = clock();
        p.text(
            &clock.hh_mm,
            (cl + pad, ct + 20.0 * scale, cr - pad, ct + 92.0 * scale),
            52.0 * scale,
            DWRITE_FONT_WEIGHT_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        )?;
        p.text(
            &clock.date,
            (cl + pad, ct + 92.0 * scale, cr - pad, ct + 118.0 * scale),
            15.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, DIM),
        )?;
        p.text(
            "Nothing is playing",
            (cl + pad, cb - 34.0 * scale, cr - pad, cb - 10.0 * scale),
            13.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.45),
        )?;
    } else {
        let header = (cl, ct, cr, ct + 94.0 * scale);
        let blend = &ui.home.track_transition;
        let progress = blend.progress();
        if blend.active() {
            p.push_clip(header);
            let drawn = (|| -> Result<()> {
                for (media, weight, text) in &blend.from {
                    let bitmap =
                        p.offscreen((cr - cl, 94.0 * scale), (cl, ct), |q| paint_track(q, pill, scale, ui, media))?;
                    draw_track_mix(
                        p,
                        &bitmap,
                        header,
                        scale,
                        ui,
                        weight * (1.0 - progress),
                        text * (1.0 - 2.0 * progress).max(0.0),
                    );
                }
                let bitmap =
                    p.offscreen((cr - cl, 94.0 * scale), (cl, ct), |q| paint_track(q, pill, scale, ui, &ui.media))?;
                draw_track_mix(p, &bitmap, header, scale, ui, progress, (2.0 * progress - 1.0).max(0.0));
                Ok(())
            })();
            p.pop_clip();
            drawn?;
        } else {
            paint_track(p, pill, scale, ui, &ui.media)?;
        }
        let (ar, ag, ab) =
            if wave { colours.accent } else { ui.media.accent.unwrap_or_else(crate::ui::theme::accent_u8) };
        let accent = rgba(f32::from(ar) / 255.0, f32::from(ag) / 255.0, f32::from(ab) / 255.0, 1.0);
        let (tl, tr_) = (cl + pad, cr - pad);
        let ty = ct + 100.0 * scale;
        p.fill_round_rect((tl, ty - 1.5 * scale, tr_, ty + 1.5 * scale), 1.5 * scale, rgba(1.0, 1.0, 1.0, 0.18))?;
        if let Some((real_pos, len)) = ui.home.preview_timeline.or_else(crate::platform::media::progress) {
            let pos = ui.home.seek.map_or(real_pos, |f| f * len);
            let x = tl + (tr_ - tl) * (pos / len).clamp(0.0, 1.0);
            let live = ui.home.hover_act == Some(520) || ui.home.seek.is_some();
            let half = if live { 2.5 } else { 1.5 } * scale;
            p.fill_round_rect((tl, ty - half, tr_, ty + half), half, rgba(1.0, 1.0, 1.0, 0.18))?;
            p.fill_round_rect((tl, ty - half, x, ty + half), half, accent)?;
            if live {
                p.fill_round_rect(
                    (x - 5.0 * scale, ty - 5.0 * scale, x + 5.0 * scale, ty + 5.0 * scale),
                    5.0 * scale,
                    WHITE,
                )?;
            }
            p.text(
                &mmss(pos),
                (tl, ty + 2.0 * scale, tl + 60.0 * scale, ty + 18.0 * scale),
                11.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                rgba(1.0, 1.0, 1.0, 0.55),
            )?;
            p.text(
                &format!("-{}", mmss(len - pos)),
                (tr_ - 60.0 * scale, ty + 2.0 * scale, tr_, ty + 18.0 * scale),
                11.0 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                rgba(1.0, 1.0, 1.0, 0.55),
            )?;
        }
        if ui.media.players >= 2 {
            let btn = player_btn(pill, scale, wide);
            p.fill_round_rect(btn, 15.0 * scale, rgba(1.0, 1.0, 1.0, 0.14 * ui.action_hover(560)))?;
            p.glyph('\u{E8AB}', btn, 15.0 * scale, rgba(1.0, 1.0, 1.0, 0.7))?;
            p.text(
                &ui.media.players.to_string(),
                (btn.0, btn.3 - 12.0 * scale, btn.2 - 2.0 * scale, btn.3 + 2.0 * scale),
                10.0 * scale,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                accent,
            )?;
        }
        if spotify_active(ui) {
            let accent_on = accent;
            let off = rgba(1.0, 1.0, 1.0, 0.55);
            let repeat_glyph =
                if ui.spotify.repeat == crate::services::spotify::Repeat::Track { '\u{E8ED}' } else { '\u{E8EE}' };
            let items = [
                ('\u{E8B1}', ui.spotify.shuffle),
                (repeat_glyph, ui.spotify.repeat != crate::services::spotify::Repeat::Off),
                (if ui.spotify.liked { '\u{EB51}' } else { '\u{E006}' }, ui.spotify.liked),
            ];
            for (i, (glyph, on)) in items.iter().enumerate() {
                p.fill_round_rect(
                    extra_btn(pill, scale, i, wide),
                    15.0 * scale,
                    rgba(1.0, 1.0, 1.0, 0.14 * ui.action_hover(510 + i)),
                )?;
                p.glyph(*glyph, extra_btn(pill, scale, i, wide), 15.0 * scale, if *on { accent_on } else { off })?;
            }
        }
        for (i, g) in
            ['\u{E892}', if ui.media.playing { '\u{E769}' } else { '\u{E768}' }, '\u{E893}'].iter().enumerate()
        {
            let r = ui.action_rect(500 + i, media_btn(pill, scale, i, wide));
            let hover = ui.action_hover(500 + i);
            if i != 1 {
                p.fill_round_rect(r, 17.0 * scale, rgba(1.0, 1.0, 1.0, 0.16 * hover))?;
            }
            if i == 1 {
                p.fill_round_rect(
                    r,
                    21.0 * scale,
                    rgba(0.96 - hover * 0.11, 0.96 - hover * 0.11, 0.98 - hover * 0.1, 1.0),
                )?;
                p.glyph(*g, r, 16.0 * scale, rgba(0.0, 0.0, 0.0, 1.0))?;
            } else {
                p.glyph(*g, r, 16.0 * scale, WHITE)?;
            }
        }
    }

    let row = volume_row(pill, scale, wide);
    p.fill_round_rect(row, 16.0 * scale, rgba(1.0, 1.0, 1.0, 0.07))?;
    let level = if ui.home.vol_track { ui.home.app_vol } else { ui.home.vol };
    let tint = if ui.home.vol_track {
        ui.media
            .accent
            .map_or(WHITE, |(r, g, b)| rgba(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 1.0))
    } else {
        WHITE
    };
    let icon = if ui.home.vol_track {
        '\u{E8D6}'
    } else if level <= 0.001 {
        '\u{E74F}'
    } else {
        '\u{E767}'
    };
    let icon_box = (row.0 + 6.0 * scale, row.1 + 6.0 * scale, row.0 + 40.0 * scale, row.3 - 6.0 * scale);
    if ui.home.hover_act == Some(530) || ui.home.mixer_open {
        p.fill_round_rect(
            icon_box,
            12.0 * scale,
            if ui.home.mixer_open { acc(0.25) } else { rgba(1.0, 1.0, 1.0, 0.14) },
        )?;
    }
    p.glyph(icon, icon_box, 15.0 * scale, if ui.home.vol_track { tint } else { rgba(1.0, 1.0, 1.0, 0.8) })?;
    let (tl, tr, cy) = volume_track(pill, scale, wide);
    let x = tl + (tr - tl) * level.clamp(0.0, 1.0);
    p.fill_round_rect((tl, cy - 2.0 * scale, tr, cy + 2.0 * scale), 2.0 * scale, rgba(1.0, 1.0, 1.0, 0.2))?;
    p.fill_round_rect((tl, cy - 2.0 * scale, x, cy + 2.0 * scale), 2.0 * scale, tint)?;
    p.fill_round_rect((x - 6.0 * scale, cy - 6.0 * scale, x + 6.0 * scale, cy + 6.0 * scale), 6.0 * scale, WHITE)?;
    p.text(
        &format!("{}", (level * 100.0).round() as u32),
        (tr + 8.0 * scale, row.1, tr + 38.0 * scale, row.3),
        13.0 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
        rgba(1.0, 1.0, 1.0, 0.75),
    )?;
    if !ui.home.device.is_empty() {
        let chip = device_chip(pill, scale, wide);
        let hover = ui.home.hover_act == Some(100);
        p.fill_round_rect(chip, 12.0 * scale, rgba(1.0, 1.0, 1.0, if hover { 0.16 } else { 0.1 }))?;
        p.glyph(
            '\u{E7F5}',
            (chip.0 + 4.0 * scale, chip.1, chip.0 + 26.0 * scale, chip.3),
            12.0 * scale,
            rgba(1.0, 1.0, 1.0, 0.75),
        )?;
        p.text(
            &ui.home.device,
            (chip.0 + 28.0 * scale, chip.1, chip.2 - 6.0 * scale, chip.3),
            12.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.9),
        )?;
    }

    let edit = edit_button(pill, scale, wide);
    if ui.home.edit {
        p.fill_round_rect(edit, 12.0 * scale, acc(if ui.home.hover_act == Some(550) { 0.55 } else { 0.4 }))?;
    } else if ui.home.hover_act == Some(550) {
        p.fill_round_rect(edit, 12.0 * scale, rgba(1.0, 1.0, 1.0, 0.14))?;
    }
    p.glyph(
        if ui.home.edit { '\u{E73E}' } else { '\u{E70F}' },
        edit,
        14.0 * scale,
        rgba(1.0, 1.0, 1.0, if ui.home.edit { 1.0 } else { 0.6 }),
    )?;

    let carried = ui.home.tile_reorder.as_ref().filter(|_| !ui.home.edit);
    let slots = slot_count(ui);
    let order: Vec<usize> =
        (0..slots).filter(|s| Some(*s) != carried.map(|r| r.from)).chain(carried.map(|r| r.from)).collect();
    for slot in order {
        let Some(id) = slot_action(ui, slot) else { continue };
        let Some(kind) = tile_kind(id) else { continue };
        let mut r = action_tile(pill, scale, slot);
        if let Some(c) = carried.filter(|c| c.pos.len() == slots) {
            let (w, h) = (r.2 - r.0, r.3 - r.1);
            r = if slot == c.from {
                c.dragged_rect((w, h))
            } else {
                (c.pos[slot].0, c.pos[slot].1, c.pos[slot].0 + w, c.pos[slot].1 + h)
            };
        }
        let chosen = ui.cfg.home_actions.contains(&(id as u8));
        let recording = id == 6 && ui.recording_secs.is_some() && !ui.home.edit;
        let hovered = ui.action_hover(id);
        let r = ui.action_rect(id, r);
        let bg = if recording {
            rgba(1.0, 0.3, 0.3, 0.3)
        } else if ui.home.edit && chosen {
            acc(0.32 + hovered * 0.13)
        } else {
            rgba(1.0, 1.0, 1.0, if ui.home.edit { 0.05 + hovered * 0.1 } else { 0.08 + hovered * 0.07 })
        };
        p.fill_round_rect(r, 16.0 * scale, bg)?;
        if ui.home.edit && chosen {
            p.stroke_round_rect(r, 16.0 * scale, 1.2 * scale, acc(0.8))?;
        }
        let dim = if ui.home.edit && !chosen { 0.5 } else { 1.0 };
        match kind {
            Tile::Action(glyph, label) => {
                p.glyph(
                    glyph,
                    (r.0 + 6.0 * scale, r.1, r.0 + 38.0 * scale, r.3),
                    17.0 * scale,
                    rgba(1.0, 1.0, 1.0, 0.9 * dim),
                )?;
                p.text_wrapped(
                    if recording { "Stop recording" } else { label },
                    (r.0 + 40.0 * scale, r.1 + 2.0 * scale, r.2 - 6.0 * scale, r.3 - 2.0 * scale),
                    12.5 * scale,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    rgba(1.0, 1.0, 1.0, dim),
                )?;
            }
            Tile::Info(glyph, label, info) => {
                p.glyph(
                    glyph,
                    (r.0 + 6.0 * scale, r.1, r.0 + 38.0 * scale, r.3),
                    17.0 * scale,
                    rgba(1.0, 1.0, 1.0, 0.9 * dim),
                )?;
                p.text(
                    label,
                    (r.0 + 40.0 * scale, r.1 + 4.0 * scale, r.2 - 6.0 * scale, r.1 + 21.0 * scale),
                    10.5 * scale,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    rgba(1.0, 1.0, 1.0, 0.55 * dim),
                )?;
                p.text(
                    &fit(info_value(ui, info), 14),
                    (r.0 + 40.0 * scale, r.1 + 19.0 * scale, r.2 - 6.0 * scale, r.3 - 3.0 * scale),
                    13.5 * scale,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    rgba(1.0, 1.0, 1.0, dim),
                )?;
            }
        }
    }
    if ui.home.edit && home_edit_rows() > 0 {
        let first = action_tile(pill, scale, 1);
        let last = action_tile(pill, scale, MAX_HOME_TILES - 1);
        let track = (first.2 + 3.0 * scale, first.1, first.2 + 6.0 * scale, last.3);
        let share = ui.home.edit_scroll as f32 / home_edit_rows() as f32;
        let thumb_h = (track.3 - track.1) * 0.35;
        let top = track.1 + (track.3 - track.1 - thumb_h) * share;
        p.fill_round_rect((track.0, top, track.2, top + thumb_h), 1.5 * scale, rgba(1.0, 1.0, 1.0, 0.35))?;
    }

    paint_home_notification(p, pill, scale, ui)?;
    if ui.home.preview_timeline.is_none() {
        paint_dock(p, pill, scale, ui)?;
    }
    if discord_strip(ui) {
        paint_discord_strip(p, pill, scale, ui)?;
    }
    paint_home_mixer(p, pill, scale, ui)?;
    paint_menu(p, pill, scale, ui)
}

fn draw_track_mix(
    p: &Painter,
    bitmap: &windows::Win32::Graphics::Direct2D::ID2D1Bitmap,
    header: Rect,
    scale: f32,
    ui: &UiState,
    cover: f32,
    text: f32,
) {
    let split = header.0 + (16.0 + 72.0 * cover_aspect(ui) + 8.0) * scale;
    p.push_clip((header.0, header.1, split, header.3));
    p.draw_bitmap(bitmap, header, cover);
    p.pop_clip();
    p.push_clip((split, header.1, header.2, header.3));
    p.draw_bitmap(bitmap, header, text);
    p.pop_clip();
}

pub fn home_cover_rect(pill: Rect, scale: f32, ui: &UiState) -> Rect {
    let body = body_rect(pill, scale, Tab::Home);
    let card = media_card(body, scale, home_wide(ui));
    (
        card.0 + 16.0 * scale,
        card.1 + 16.0 * scale,
        card.0 + (16.0 + 72.0 * cover_aspect(ui)) * scale,
        card.1 + 88.0 * scale,
    )
}

fn paint_track(p: &Painter, pill: Rect, scale: f32, ui: &UiState, media: &crate::platform::media::Media) -> Result<()> {
    let wide = home_wide(ui);
    let card = media_card(pill, scale, wide);
    let (cl, ct, cr, _) = card;
    let pad = 16.0 * scale;
    let wave = ui.yandex.state.active && media.source == "Night Island · Yandex Music";
    let colours = ui.yandex.colours();
    let side = 72.0 * scale;
    let cover = (cl + pad, ct + pad, cl + pad + side * cover_aspect(ui), ct + pad + side);
    if !ui.cover_in_flight {
        match &media.cover {
            Some(c) => p.cover(cover, 12.0 * scale, c, ui.cfg.cover_mode)?,
            None => {
                p.fill_round_rect(cover, 12.0 * scale, rgba(1.0, 1.0, 1.0, 0.1))?;
                p.glyph('\u{E8D6}', cover, 28.0 * scale, rgba(1.0, 1.0, 1.0, 0.6))?;
            }
        }
    }
    let tx = cover.2 + 14.0 * scale;
    p.text(
        if wave { "My Wave · Settings" } else { &media.source },
        (tx, ct + 14.0 * scale, cr - pad - if wave { 20.0 * scale } else { 0.0 }, ct + 32.0 * scale),
        11.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.5),
    )?;
    if wave {
        let link = wave_link(pill, scale, ui);
        p.glyph(
            '\u{E76C}',
            (link.2 - 20.0 * scale, link.1, link.2, link.3),
            10.0 * scale,
            wave_colour(colours.accent, 1.0),
        )?;
        if ui.home.hover_act == Some(561) {
            p.fill_round_rect(link, 7.0 * scale, rgba(1.0, 1.0, 1.0, 0.07))?;
        }
    }
    p.text(
        &media.title,
        (tx, ct + 32.0 * scale, cr - pad, ct + 58.0 * scale),
        18.0 * scale,
        DWRITE_FONT_WEIGHT_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    p.text(
        &media.artist,
        (tx, ct + 58.0 * scale, cr - pad, ct + 80.0 * scale),
        14.0 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.65),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapid_track_changes_keep_the_visible_mix_and_release_previous_covers() {
        let media = |title: &str| crate::platform::media::Media { title: title.into(), ..Default::default() };
        let mut blend = TrackTransition::default();
        blend.retarget(&media("A"), true);
        blend.start = Some(std::time::Instant::now() - std::time::Duration::from_millis(80));
        let old_weight = 1.0 - blend.progress();
        blend.retarget(&media("B"), true);
        assert!((blend.from.iter().find(|(v, _, _)| v.title == "A").unwrap().1 - old_weight).abs() < 0.01);
        assert!(blend.from.iter().any(|(v, w, _)| v.title == "B" && *w > 0.0));
        assert!((blend.from.iter().map(|v| v.1).sum::<f32>() - 1.0).abs() < 0.001);
        for i in 0..100 {
            blend.retarget(&media(&i.to_string()), true);
        }
        assert!(blend.from.len() <= 16);
        blend.retarget(&media("Stopped"), false);
        assert!(!blend.active());
        assert!(blend.from.is_empty());
    }

    #[test]
    fn wave_link_preserves_transport_controls_and_respects_the_mixer_overlay() {
        let mut ui = UiState::new();
        ui.media.title = "Fixture".into();
        ui.media.source = "Night Island · Yandex Music".into();
        ui.yandex.state.active = true;
        let pill = (0.0, 0.0, 640.0, 384.0);
        let r = wave_link(pill, 1.0, &ui);
        assert!(matches!(hit_home(pill, 1.0, (r.0 + r.2) / 2.0, (r.1 + r.3) / 2.0, &ui), Some(HomeHit::Wave)));
        let r = media_btn(pill, 1.0, 1, home_wide(&ui));
        assert!(matches!(hit_home(pill, 1.0, (r.0 + r.2) / 2.0, (r.1 + r.3) / 2.0, &ui), Some(HomeHit::Media(1))));
        ui.home.mixer_open = true;
        let r = wave_link(pill, 1.0, &ui);
        assert!(!matches!(hit_home(pill, 1.0, (r.0 + r.2) / 2.0, (r.1 + r.3) / 2.0, &ui), Some(HomeHit::Wave)));
    }

    #[test]
    fn every_tile_of_the_catalogue_has_a_kind() {
        for id in 0..HOME_ACTION_COUNT {
            assert!(tile_kind(id).is_some(), "{id}");
        }
        assert!(tile_kind(HOME_ACTION_COUNT).is_none());
        assert!(matches!(tile_kind(0), Some(Tile::Action(..))));
        assert!(matches!(tile_kind(HOME_ACTIONS.len()), Some(Tile::Info(_, _, Info::Metric(Metric::Cpu)))));
        assert!(
            tile_needs_metrics(HOME_ACTIONS.len())
                && !tile_needs_metrics(0)
                && !tile_needs_metrics(HOME_ACTION_COUNT - 1)
        );
    }

    #[test]
    fn editing_lays_out_the_whole_catalogue_two_to_a_row() {
        let mut ui = UiState::new();
        ui.home.edit = true;
        assert_eq!((slot_action(&ui, 0), slot_action(&ui, 7)), (Some(0), Some(7)));
        ui.home.edit_scroll = home_edit_rows();
        let last_row = slot_count(&ui);
        assert!((1..=MAX_HOME_TILES).contains(&last_row));
        assert_eq!(slot_action(&ui, last_row - 1), Some(HOME_ACTION_COUNT - 1));
        assert_eq!(slot_action(&ui, last_row), None);
    }

    #[test]
    fn chosen_tiles_follow_each_other_up_to_the_grid_size() {
        let mut ui = UiState::new();
        ui.cfg.home_actions = vec![9, 2, 20];
        assert_eq!(
            (slot_action(&ui, 0), slot_action(&ui, 1), slot_action(&ui, 2), slot_action(&ui, 3)),
            (Some(9), Some(2), Some(20), None)
        );
        assert!(!home_wide(&ui));
        ui.cfg.home_actions.clear();
        assert!(home_wide(&ui));
    }

    #[test]
    fn readouts_say_something_before_any_data_arrives() {
        crate::ui::i18n::set_ru(false);
        let mut ui = UiState::new();
        ui.cfg.activity_track = false;
        assert_eq!(info_value(&ui, Info::Metric(Metric::Cpu)), "\u{2026}");
        assert_eq!(info_value(&ui, Info::Reminder), "None");
        assert_eq!(info_value(&ui, Info::Steam), "Not playing");
        assert_eq!(info_value(&ui, Info::Agents), "Idle");
        assert_eq!(info_value(&ui, Info::Vpn), "\u{2014}");
        assert_eq!(info_value(&ui, Info::ScreenTime), "Tracking off");
    }
}
