use crate::data::home_widgets::{self, WidgetSize};
use crate::view::*;
use std::cell::RefCell;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetControl {
    Add,
    Done,
    Remove,
    Size(WidgetSize),
}

#[derive(Clone)]
pub struct WidgetDrag {
    pub id: u8,
    pub offset: (f32, f32),
    pub cursor: (f32, f32),
    pub origin: Rect,
    pub order: Vec<u8>,
    pub valid: bool,
}

impl WidgetDrag {
    pub fn rect(&self) -> Rect {
        let (x, y) = (self.cursor.0 - self.offset.0, self.cursor.1 - self.offset.1);
        (x, y, x + self.origin.2 - self.origin.0, y + self.origin.3 - self.origin.1)
    }
}

#[derive(Default)]
pub struct HomeWidgets {
    pub editing: bool,
    pub editor_motion: Option<(bool, Instant)>,
    pub selected: Option<u8>,
    pub resize: Option<(u8, i32, i32, WidgetSize)>,
    pub preview: Option<(u8, WidgetSize)>,
    pub press: Option<(u8, i32, i32, Rect)>,
    pub drag: Option<WidgetDrag>,
    pub status: String,
    pub motion: RefCell<WidgetMotion>,
}

impl HomeWidgets {
    pub fn interacting(&self) -> bool {
        self.resize.is_some() || self.press.is_some() || self.drag.is_some()
    }
    pub fn editor_active(&self) -> bool {
        self.editor_motion.is_some_and(|(_, at)| at.elapsed().as_millis() < 220)
    }
    pub fn editor_alpha(&self) -> f32 {
        self.editor_motion.map_or(if self.editing { 1.0 } else { 0.0 }, |(opening, at)| {
            let p = (at.elapsed().as_secs_f32() / 0.22).clamp(0.0, 1.0);
            let p = p * p * (3.0 - 2.0 * p);
            if opening { p } else { 1.0 - p }
        })
    }
}

#[derive(Clone)]
struct Visual {
    id: u8,
    from: Rect,
    to: Rect,
    opacity: f32,
    target_opacity: f32,
}

#[derive(Default)]
pub struct WidgetMotion {
    initialized: bool,
    target: Vec<(u8, Rect)>,
    nodes: Vec<Visual>,
    start: Option<Instant>,
}

impl WidgetMotion {
    pub fn release_from(&mut self, id: u8, rect: Rect) {
        let now = Instant::now();
        let current = self.at(now);
        for node in &mut self.nodes {
            if let Some((_, r, alpha)) = current.iter().find(|v| v.0 == node.id) {
                node.from = if node.id == id { rect } else { *r };
                node.opacity = *alpha;
            }
        }
        self.start = Some(now);
    }

    pub fn active(&self) -> bool {
        self.start.is_some_and(|at| at.elapsed() < Duration::from_millis(280))
    }
    fn at(&self, now: Instant) -> Vec<(u8, Rect, f32)> {
        let t = self.start.map_or(1.0, |at| now.saturating_duration_since(at).as_secs_f32() / 0.28).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        self.nodes
            .iter()
            .map(|v| {
                let blend = |a, b| a + (b - a) * t;
                (
                    v.id,
                    (
                        blend(v.from.0, v.to.0),
                        blend(v.from.1, v.to.1),
                        blend(v.from.2, v.to.2),
                        blend(v.from.3, v.to.3),
                    ),
                    blend(v.opacity, v.target_opacity),
                )
            })
            .filter(|v| v.2 > 0.001)
            .collect()
    }
    fn sample(&mut self, target: Vec<(u8, Rect)>, animate: bool) -> Vec<(u8, Rect, f32)> {
        let now = Instant::now();
        let animate = animate && self.initialized;
        self.initialized = true;
        if !animate {
            self.start = None;
        }
        if target != self.target {
            let previous = self.at(now);
            self.nodes = target
                .iter()
                .map(|(id, r)| {
                    let from = previous.iter().find(|v| v.0 == *id);
                    Visual {
                        id: *id,
                        from: from.map_or((r.0, r.1 + 6.0, r.2, r.3 + 6.0), |v| v.1),
                        to: *r,
                        opacity: from.map_or(0.0, |v| v.2),
                        target_opacity: 1.0,
                    }
                })
                .chain(
                    previous
                        .iter()
                        .copied()
                        .filter(|v| !target.iter().any(|t| t.0 == v.0))
                        .map(|(id, r, opacity)| Visual { id, from: r, to: r, opacity, target_opacity: 0.0 }),
                )
                .collect();
            self.target = target;
            self.start = animate.then_some(now);
        }
        self.at(now)
    }
}

pub fn weather_widget_id() -> u8 {
    (HOME_ACTIONS.len() + HOME_INFO.iter().position(|(_, _, info)| matches!(info, Info::Weather)).unwrap_or(8)) as u8
}

pub fn widget_size(ui: &UiState, id: u8) -> WidgetSize {
    if ui.cfg.home_widget_details && !detailed_widget(id) {
        return WidgetSize::Tile;
    }
    ui.home
        .widgets
        .preview
        .filter(|v| v.0 == id)
        .map(|v| v.1)
        .or_else(|| ui.cfg.home_widget_sizes.get(&id).copied())
        .unwrap_or(if id == weather_widget_id() && !ui.cfg.home_actions.contains(&id) {
            WidgetSize::Wide4
        } else {
            WidgetSize::Tile
        })
}

fn specs(ui: &UiState) -> Vec<(u8, WidgetSize)> {
    ui.home
        .widgets
        .drag
        .as_ref()
        .map_or(ui.cfg.home_actions.as_slice(), |d| d.order.as_slice())
        .iter()
        .map(|&id| (id, widget_size(ui, id)))
        .collect()
}

pub fn can_resize_widget(ui: &UiState, id: u8, size: WidgetSize) -> bool {
    if ui.cfg.home_widget_details && !detailed_widget(id) && size != WidgetSize::Tile {
        return false;
    }
    let mut items = specs(ui);
    match items.iter_mut().find(|v| v.0 == id) {
        Some(item) => item.1 = size,
        None => items.push((id, size)),
    }
    home_widgets::pack(&items, 0).is_some()
}

fn layout_widgets(ui: &UiState) -> Vec<home_widgets::Placement> {
    let reserved = notification_tile_count(ui) + live_tile_count(ui);
    let occupied = if reserved >= 8 { u8::MAX } else { ((1u16 << reserved) - 1) as u8 };
    let mut selected = Vec::new();
    let mut layout = Vec::new();
    for item in specs(ui) {
        selected.push(item);
        if let Some(next) = home_widgets::pack(&selected, occupied) {
            layout = next;
        } else {
            selected.pop();
        }
    }
    let weather = weather_widget_id();
    if ui.cfg.home_weather_widget && ui.cfg.weather && ui.weather.is_some() && !selected.iter().any(|v| v.0 == weather)
    {
        selected.push((weather, widget_size(ui, weather)));
        if let Some(next) = home_widgets::pack(&selected, occupied) {
            layout = next;
        }
    }
    layout
}

pub fn has_home_widgets(ui: &UiState) -> bool {
    !ui.cfg.home_actions.is_empty() || (ui.cfg.home_weather_widget && ui.cfg.weather && ui.weather.is_some())
}

pub fn home_widget_area(pill: Rect, s: f32) -> Rect {
    let left = grid_left(pill, s, false);
    let top = pill.1 + HOME_TOP * s;
    (left, top, left + (2.0 * GRID_COL_W + GRID_GAP) * s, top + (4.0 * GRID_ROW_H + 3.0 * GRID_GAP) * s)
}

pub fn home_widget_targets(pill: Rect, s: f32, ui: &UiState) -> Vec<(u8, Rect)> {
    let left = grid_left(pill, s, false);
    layout_widgets(ui)
        .into_iter()
        .map(|p| {
            let (w, h) = p.size.dimensions();
            let x = left + f32::from(p.column) * (GRID_COL_W + GRID_GAP) * s;
            let y = pill.1 + (HOME_TOP + f32::from(p.row) * (GRID_ROW_H + GRID_GAP)) * s;
            (
                p.id,
                (
                    x,
                    y,
                    x + (f32::from(w) * GRID_COL_W + f32::from(w - 1) * GRID_GAP) * s,
                    y + (f32::from(h) * GRID_ROW_H + f32::from(h - 1) * GRID_GAP) * s,
                ),
            )
        })
        .collect()
}

pub fn home_widget_rects(pill: Rect, s: f32, ui: &UiState) -> Vec<(u8, Rect, f32)> {
    let targets = home_widget_targets(pill, s, ui);
    ui.home
        .widgets
        .motion
        .borrow_mut()
        .sample(targets, ui.cfg.island_animations && crate::ui::feedback::motion_allowed())
}

pub fn widget_resize_handle(r: Rect, s: f32) -> Rect {
    (r.2 - 22.0 * s, r.3 - 22.0 * s, r.2, r.3)
}

pub fn hit_home_widget(pill: Rect, s: f32, ui: &UiState, x: f32, y: f32) -> Option<HomeHit> {
    let visible = layout_widgets(ui);
    for (id, r, opacity) in home_widget_rects(pill, s, ui).into_iter().rev() {
        if opacity < 0.2 || !visible.iter().any(|v| v.id == id) {
            continue;
        }
        if ui.home.widgets.editing
            && (!ui.cfg.home_widget_details || detailed_widget(id))
            && inside(widget_resize_handle(r, s), x, y)
        {
            return Some(HomeHit::WidgetHandle(id));
        }
        if inside(r, x, y) {
            if ui.cfg.home_widget_details
                && !ui.home.widgets.editing
                && r.3 - r.1 > 60.0 * s
                && let Some((command, _)) = widget_command(id)
            {
                if inside(widget_command_rect(r, s), x, y) {
                    return Some(HomeHit::WidgetCommand(command));
                }
                return None;
            }
            return Some(HomeHit::Action(usize::from(id)));
        }
    }
    None
}

pub fn widget_editor_controls(pill: Rect, s: f32, ui: &UiState) -> Vec<(Rect, WidgetControl)> {
    let card = media_card(pill, s, false);
    let width = card.2 - card.0;
    let mut out = Vec::new();
    if ui.home.widgets.selected.is_some() {
        for (i, size) in WidgetSize::ALL.iter().enumerate().filter(|(_, size)| {
            **size == WidgetSize::Tile
                || !ui.cfg.home_widget_details
                || ui.home.widgets.selected.is_some_and(detailed_widget)
        }) {
            let (columns, column, row) = if i < 3 { (3.0, i as f32, 0.0) } else { (2.0, (i - 3) as f32, 1.0) };
            let step = (width - 24.0 * s) / columns;
            let x = card.0 + 12.0 * s + column * step;
            let y = card.1 + (61.0 + row * 37.0) * s;
            out.push(((x, y, x + step - 6.0 * s, y + 30.0 * s), WidgetControl::Size(*size)));
        }
    }
    let row = volume_row(pill, s, false);
    let mut x = row.0 + 6.0 * s;
    let width = (row.2 - row.0 - 24.0 * s) / 3.0;
    for control in [WidgetControl::Add, WidgetControl::Remove, WidgetControl::Done] {
        out.push(((x, row.1 + 7.0 * s, x + width, row.3 - 7.0 * s), control));
        x += width + 6.0 * s;
    }
    out
}

pub fn paint_home_widgets(p: &Painter, pill: Rect, s: f32, ui: &UiState) -> Result<()> {
    for (id, r, alpha) in home_widget_rects(pill, s, ui) {
        if ui.home.widgets.drag.as_ref().is_some_and(|d| d.id == id) {
            continue;
        }
        paint_widget(p, r, s, ui, id, alpha)?;
    }
    if let Some(drag) = &ui.home.widgets.drag
        && let Some((_, target)) = home_widget_targets(pill, s, ui).into_iter().find(|v| v.0 == drag.id)
    {
        let color = if drag.valid { acc(0.75) } else { rgba(1.0, 0.35, 0.35, 0.6) };
        p.fill_round_rect(target, 16.0 * s, rgba(color.r, color.g, color.b, 0.12))?;
        p.stroke_round_rect(target, 16.0 * s, 1.5 * s, color)?;
    }
    Ok(())
}

pub fn paint_widget_drag(p: &Painter, s: f32, ui: &UiState) -> Result<()> {
    if let Some(drag) = &ui.home.widgets.drag {
        let r = drag.rect();
        p.fill_round_rect(
            (r.0 + 2.0 * s, r.1 + 4.0 * s, r.2 + 2.0 * s, r.3 + 4.0 * s),
            17.0 * s,
            rgba(0.0, 0.0, 0.0, 0.5),
        )?;
        p.fill_round_rect(r, 16.0 * s, rgba(0.08, 0.08, 0.1, 0.98))?;
        paint_widget(p, r, s, ui, drag.id, 0.95)?;
    }
    Ok(())
}

fn paint_widget(p: &Painter, r: Rect, s: f32, ui: &UiState, id: u8, alpha: f32) -> Result<()> {
    p.with_opacity(r, alpha, || {
        let Some(kind) = tile_kind(usize::from(id)) else { return Ok(()) };
        let recording = id == 6 && ui.recording_secs.is_some();
        let kind = if recording { Tile::Action('\u{E714}', "Stop recording") } else { kind };
        let selected = ui.home.widgets.editing && ui.home.widgets.selected == Some(id);
        p.fill_round_rect(
            r,
            16.0 * s,
            if selected {
                acc(0.19)
            } else if recording {
                rgba(1.0, 0.3, 0.3, 0.25)
            } else {
                rgba(1.0, 1.0, 1.0, 0.07 + ui.action_hover(usize::from(id)) * 0.06)
            },
        )?;
        if selected {
            p.stroke_round_rect(r, 16.0 * s, s, acc(0.85))?;
        }
        if paint_widget_details(p, r, s, ui, id)? {
        } else if let Tile::Info(_, _, Info::Weather) = kind {
            paint_weather_widget(p, r, s, ui)?;
        } else {
            let (glyph, title, value) = match kind {
                Tile::Action(g, t) => (g, t, None),
                Tile::Info(g, t, i) => (g, t, Some(info_value(ui, i))),
            };
            let large = r.3 - r.1 > 60.0 * s;
            if large {
                p.glyph(glyph, (r.0 + 10.0 * s, r.1 + 5.0 * s, r.0 + 38.0 * s, r.1 + 32.0 * s), 17.0 * s, acc(1.0))?;
                text(p, title, (r.0 + 40.0 * s, r.1 + 5.0 * s, r.2 - 8.0 * s, r.1 + 31.0 * s), s, 11.0, false, 0.7)?;
                let value = value.unwrap_or_else(|| crate::ui::i18n::tr(title).into_owned());
                text(
                    p,
                    &value,
                    (r.0 + 12.0 * s, r.1 + 33.0 * s, r.2 - 10.0 * s, r.3 - 12.0 * s),
                    s,
                    if r.2 - r.0 > 150.0 * s { 25.0 } else { 21.0 },
                    true,
                    1.0,
                )?;
            } else {
                p.glyph(glyph, (r.0 + 5.0 * s, r.1, r.0 + 37.0 * s, r.3), 17.0 * s, WHITE)?;
                if let Some(value) = value {
                    text(
                        p,
                        title,
                        (r.0 + 39.0 * s, r.1 + 3.0 * s, r.2 - 6.0 * s, r.1 + 21.0 * s),
                        s,
                        10.5,
                        false,
                        0.55,
                    )?;
                    text(
                        p,
                        &value,
                        (r.0 + 39.0 * s, r.1 + 21.0 * s, r.2 - 6.0 * s, r.3 - 2.0 * s),
                        s,
                        13.0,
                        true,
                        1.0,
                    )?;
                } else {
                    text(p, title, (r.0 + 39.0 * s, r.1, r.2 - 6.0 * s, r.3), s, 12.0, true, 1.0)?;
                }
            }
        }
        if ui.home.widgets.editing && (!ui.cfg.home_widget_details || detailed_widget(id)) {
            p.glyph('\u{E784}', widget_resize_handle(r, s), 12.0 * s, acc(0.8))?;
        }
        Ok(())
    })
}

fn text(p: &Painter, value: &str, r: Rect, s: f32, size: f32, strong: bool, alpha: f32) -> Result<()> {
    p.text(
        value,
        r,
        size * s,
        if strong { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, alpha),
    )
}

fn paint_weather_widget(p: &Painter, r: Rect, s: f32, ui: &UiState) -> Result<()> {
    if !ui.cfg.weather {
        return text(p, "Weather is off", r, s, 12.0, false, 0.6);
    }
    let Some(wx) = &ui.weather else { return text(p, "Weather", r, s, 13.0, true, 0.65) };
    let (w, h) = ((r.2 - r.0) / s, (r.3 - r.1) / s);
    if h < 60.0 {
        weather_icon(p, r.0 + 24.0 * s, (r.1 + r.3) / 2.0, 17.0 * s, wx.sky, wx.night)?;
        text(
            p,
            &format!("{}°", wx.temp.round() as i32),
            (r.0 + 48.0 * s, r.1, r.2 - 6.0 * s, r.3),
            s,
            19.0,
            true,
            1.0,
        )?;
        if w > 150.0 {
            text(p, &wx.place, (r.0 + 106.0 * s, r.1, r.2 - 8.0 * s, r.3), s, 12.0, false, 0.65)?;
        }
        return Ok(());
    }
    text(p, &wx.place, (r.0 + 12.0 * s, r.1 + 5.0 * s, r.2 - 8.0 * s, r.1 + 25.0 * s), s, 11.0, false, 0.65)?;
    weather_icon(p, r.0 + 33.0 * s, r.1 + 51.0 * s, 22.0 * s, wx.sky, wx.night)?;
    text(
        p,
        &format!("{}°", wx.temp.round() as i32),
        (r.0 + 62.0 * s, r.1 + 28.0 * s, r.2 - 8.0 * s, r.1 + 73.0 * s),
        s,
        29.0,
        true,
        1.0,
    )?;
    if w > 150.0 {
        text(
            p,
            &format!("{}%", wx.humidity.round() as i32),
            (r.0 + 130.0 * s, r.1 + 30.0 * s, r.2 - 8.0 * s, r.1 + 53.0 * s),
            s,
            12.0,
            false,
            0.7,
        )?;
        text(
            p,
            &format!("{:.0} {}", wx.wind, crate::ui::i18n::tr("m/s")),
            (r.0 + 130.0 * s, r.1 + 52.0 * s, r.2 - 8.0 * s, r.1 + 74.0 * s),
            s,
            12.0,
            false,
            0.7,
        )?;
    }
    if h > 130.0 {
        for (i, hour) in wx.hourly.iter().take(4).enumerate() {
            let y = r.1 + (91.0 + i as f32 * 25.0) * s;
            text(
                p,
                &format!("{:02}:00", hour.hour),
                (r.0 + 10.0 * s, y, r.0 + 49.0 * s, y + 22.0 * s),
                s,
                10.5,
                false,
                0.6,
            )?;
            weather_icon(p, r.0 + 62.0 * s, y + 11.0 * s, 9.0 * s, hour.sky, hour.night)?;
            text(
                p,
                &format!("{}°", hour.temp.round() as i32),
                (r.0 + 78.0 * s, y, r.2 - 7.0 * s, y + 22.0 * s),
                s,
                12.0,
                true,
                0.9,
            )?;
        }
    } else if w > 150.0 {
        for (i, hour) in wx.hourly.iter().take(3).enumerate() {
            let x = r.0 + (12.0 + i as f32 * 79.0) * s;
            weather_icon(p, x + 8.0 * s, r.3 - 14.0 * s, 8.0 * s, hour.sky, hour.night)?;
            text(
                p,
                &format!("{}°", hour.temp.round() as i32),
                (x + 22.0 * s, r.3 - 25.0 * s, x + 70.0 * s, r.3 - 3.0 * s),
                s,
                12.0,
                true,
                0.85,
            )?;
        }
    }
    Ok(())
}

pub fn paint_widget_editor(p: &Painter, pill: Rect, s: f32, ui: &UiState) -> Result<()> {
    let alpha = ui.home.widgets.editor_alpha();
    if alpha <= 0.001 {
        return Ok(());
    }
    let card = media_card(pill, s, false);
    let row = volume_row(pill, s, false);
    p.with_opacity((card.0, card.1, row.2, row.3), alpha, || paint_widget_editor_contents(p, pill, s, ui))
}

fn paint_widget_editor_contents(p: &Painter, pill: Rect, s: f32, ui: &UiState) -> Result<()> {
    let card = media_card(pill, s, false);
    let row = volume_row(pill, s, false);
    p.fill_round_rect(card, 20.0 * s, rgba(0.06, 0.06, 0.075, 1.0))?;
    p.fill_round_rect(row, 16.0 * s, rgba(0.06, 0.06, 0.075, 1.0))?;
    text(
        p,
        "Widgets",
        (card.0 + 12.0 * s, card.1 + 6.0 * s, card.2 - 12.0 * s, card.1 + 33.0 * s),
        s,
        17.0,
        true,
        1.0,
    )?;
    let title = ui
        .home
        .widgets
        .selected
        .and_then(|id| tile_kind(usize::from(id)))
        .map(|v| match v {
            Tile::Action(_, t) | Tile::Info(_, t, _) => t,
        })
        .unwrap_or("Choose a tile on the right");
    text(p, title, (card.0 + 12.0 * s, card.1 + 32.0 * s, card.2 - 12.0 * s, card.1 + 54.0 * s), s, 12.0, false, 0.7)?;
    for (rect, control) in widget_editor_controls(pill, s, ui) {
        let active = matches!(control,WidgetControl::Size(size) if ui.home.widgets.selected.is_some_and(|id|widget_size(ui,id)==size));
        p.fill_round_rect(rect, 9.0 * s, if active { acc(0.3) } else { rgba(1.0, 1.0, 1.0, 0.07) })?;
        let label = match control {
            WidgetControl::Size(size) => size.label(),
            WidgetControl::Add => {
                if ui.home.edit {
                    "Layout"
                } else {
                    "Add"
                }
            }
            WidgetControl::Done => "Done",
            WidgetControl::Remove => "Remove",
        };
        p.text(label, rect, 12.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)?;
    }
    {
        text(
            p,
            if ui.cfg.home_widget_details && ui.home.widgets.selected.is_some_and(|id| !detailed_widget(id)) {
                "This quick action stays compact. Add a data widget to resize."
            } else if ui.home.widgets.status.is_empty() {
                "Drag tiles to arrange; pull a corner to resize"
            } else {
                &ui.home.widgets.status
            },
            (card.0 + 12.0 * s, card.3 - 23.0 * s, card.2 - 12.0 * s, card.3 - 3.0 * s),
            s,
            10.5,
            false,
            0.7,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ui() -> UiState {
        let mut ui = UiState::new();
        ui.cfg.island_animations = false;
        ui.cfg.home_actions = vec![0, 1, 7, 19];
        ui.weather = Some(crate::services::weather::Weather {
            place: "Москва".into(),
            temp: 7.0,
            wind: 2.0,
            humidity: 65.0,
            sky: Sky::Cloudy,
            night: false,
            hourly: Vec::new(),
            daily: Vec::new(),
            city_missing: false,
        });
        ui
    }
    #[test]
    fn reorder_and_drop_retarget_from_the_current_visual_position() {
        let mut motion = WidgetMotion::default();
        let first = (10.0, 10.0, 134.0, 56.0);
        let second = (142.0, 10.0, 266.0, 56.0);
        motion.sample(vec![(0, first), (1, second)], false);
        let moved = motion.sample(vec![(1, first), (0, second)], true);
        assert_eq!(moved.iter().find(|v| v.0 == 1).unwrap().1, second);
        motion.start = Some(Instant::now() - Duration::from_millis(140));
        let mid = motion.at(Instant::now());
        let x = mid.iter().find(|v| v.0 == 1).unwrap().1.0;
        assert!(x > first.0 && x < second.0);
        let ghost = (90.0, 28.0, 214.0, 74.0);
        motion.release_from(0, ghost);
        let restored = motion.sample(vec![(0, first), (1, second)], true);
        let position = restored.iter().find(|v| v.0 == 0).unwrap().1;
        assert!((position.0 - ghost.0).abs() < 0.1 && (position.1 - ghost.1).abs() < 0.1);
    }
    #[test]
    fn weather_occupies_unused_cells_and_returns_after_notification_closes() {
        let mut ui = ui();
        let original = ui.cfg.home_actions.clone();
        let weather = layout_widgets(&ui).into_iter().find(|v| v.id == weather_widget_id()).unwrap();
        assert_eq!((weather.column, weather.row), (0, 2));
        ui.notifications.preview = Some(crate::platform::notifications::Record {
            id: 3,
            created: 0,
            app_id: "test".into(),
            app: "Test".into(),
            title: "Message".into(),
            body: "Body".into(),
            icon: None,
            avatar: None,
        });
        let during = layout_widgets(&ui);
        assert!(during.iter().all(|v| v.row >= 2));
        assert!(!during.iter().any(|v| v.id == weather_widget_id()));
        let pill = (0.0, 0.0, 640.0, 380.0);
        let notification = home_notification_rect(pill, 1.0, &ui).unwrap();
        let close = home_notification_close(notification, 1.0);
        assert!(matches!(
            hit_home(pill, 1.0, (close.0 + close.2) / 2.0, (close.1 + close.3) / 2.0, &ui),
            Some(HomeHit::NotificationClose)
        ));
        ui.notifications.preview = None;
        assert!(layout_widgets(&ui).iter().any(|v| v.id == weather_widget_id()));
        assert_eq!(ui.cfg.home_actions, original);
    }
    #[test]
    fn horizontal_and_vertical_widgets_keep_the_same_expanded_bounds() {
        let mut ui = ui();
        ui.cfg.home_weather_widget = false;
        ui.cfg.home_actions[0] = 8;
        for size in WidgetSize::ALL {
            ui.cfg.home_widget_sizes.insert(8, size);
            for scale in [0.75, 1.0, 1.25, 1.5, 2.0] {
                let pill = (0.0, 0.0, 640.0 * scale, 380.0 * scale);
                let rects = home_widget_rects(pill, scale, &ui);
                assert_eq!(rects.len(), 4);
                for (_, r, _) in &rects {
                    assert!(r.0 >= 0.0 && r.2 <= pill.2 && r.1 >= 58.0 * scale && r.3 < 282.0 * scale);
                }
                for (i, a) in rects.iter().enumerate() {
                    for b in &rects[i + 1..] {
                        assert!(a.1.2 <= b.1.0 || b.1.2 <= a.1.0 || a.1.3 <= b.1.1 || b.1.3 <= a.1.1);
                    }
                }
            }
        }
    }
}
