use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::Once;
use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING,
};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_ESCAPE,
    VK_LBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Result, w};

use crate::data::top_panel::{PanelEntrance, TopPanelSettings};

#[path = "panel_motion.rs"]
mod motion;
#[path = "panel_reveal.rs"]
mod panel_reveal;
use crate::platform::{apps, desktop_dock as desktop, system::Radios};
use crate::ui::{
    gfx::{Gpu, Painter, rgba},
    i18n::tr,
    popup::PopupSurface,
};
use crate::view::Rect;

pub const WM_TOP_PANEL: u32 = WM_APP + 90;
const FRAME: usize = 1;
const DISMISS: usize = 2;
const TRAY_ANIMATION: usize = 3;
const MARGIN: f32 = 10.0;
const HEIGHT: f32 = 34.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Block {
    Application,
    Wifi,
    Bluetooth,
    Volume,
    Battery,
    Tray,
    Microphone,
    Keyboard,
    Workspaces,
}

#[derive(Clone, Default, PartialEq)]
pub struct Snapshot {
    pub active: Option<desktop::Window>,
    pub windows: Vec<desktop::Window>,
    pub radios: Radios,
    pub volume: Option<u8>,
    pub muted: bool,
    pub device: String,
    pub battery: Option<(u8, bool)>,
    pub microphone_muted: Option<bool>,
    pub keyboard_layout: String,
}

pub enum Request {
    Settings,
    Tray((i32, i32)),
    TrayAction(String, bool, (i32, i32)),
    Microphone,
    Keyboard,
    Workspaces,
    Open(Block),
    WindowsSettings(Block),
    ToggleWifi,
    ToggleBluetooth,
    Volume(u8),
    Mute,
    Mixer,
    Window(desktop::Window, desktop::WindowAction),
    Activate(desktop::Window),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Hit {
    Block(Block),
    Settings,
    Close,
    Toggle,
    Volume,
    Mute,
    Mixer,
    WindowsSettings,
    Minimize,
    Maximize,
    CloseWindow,
    TrayRetry,
    TrayIcon(usize),
    TrayPage,
    Window(usize),
}

#[derive(Clone, Copy)]
enum Role {
    Left,
    Right,
    Card(Block),
}

struct Button {
    rect: Rect,
    hit: Hit,
    enabled: bool,
}

#[derive(Clone)]
pub(crate) struct IslandMotion {
    pub progress: crate::ui::reveal::Reveal,
    pub closed: Rect,
    pub opened: Rect,
}

impl IslandMotion {
    fn bounds(&self, now: Instant) -> Rect {
        let p = self.progress.value(now);
        let mix = |a: f32, b: f32| a + (b - a) * p;
        (
            mix(self.closed.0, self.opened.0),
            mix(self.closed.1, self.opened.1),
            mix(self.closed.2, self.opened.2),
            mix(self.closed.3, self.opened.3),
        )
    }
}

struct State {
    hwnd: HWND,
    parent: HWND,
    id: usize,
    gpu: Gpu,
    role: Role,
    settings: TopPanelSettings,
    snapshot: Snapshot,
    icon: Option<std::sync::Arc<apps::Icon>>,
    surface: RefCell<Option<PopupSurface>>,
    size: (i32, i32),
    origin: (i32, i32),
    scale: f32,
    buttons: Vec<Button>,
    hover: Option<Hit>,
    pressed: Option<Hit>,
    requests: VecDeque<Request>,
    visible: bool,
    opening: bool,
    frame_timer: Cell<bool>,
    reveal: RefCell<Option<panel_reveal::Reveal>>,
    revealing: Cell<bool>,
    travel: motion::Travel,
    relocation: motion::Travel,
    displacement: (f32, f32),
    destination: (i32, i32),
    departure: (i32, i32),
    boundary: Option<(f32, bool)>,
    island_motion: Option<IslandMotion>,
    motion_changed: bool,
    alpha: u8,
    ghost_alpha: u8,
    click_through: bool,
    dirty: bool,
    feedback: crate::ui::feedback::Feedback,
    tray_icons: Vec<crate::platform::system_tray::inline::Entry>,
    tray_open: bool,
    tray_requested: bool,
    tray_page: usize,
    tray_capacity: usize,
    tray_budget: f32,
    tray_width: f32,
    tray_from: f32,
    tray_started: Instant,
}

struct Wing {
    state: Box<State>,
}

pub struct TopPanel {
    left: Wing,
    right: Wing,
    card: Wing,
    pub(super) settings: TopPanelSettings,
    icons: apps::DockIconCache,
    pub(super) reservation: Option<super::top_panel_space::Reservation>,
    pub(super) monitor: desktop::Monitor,
    pub(super) scale: f32,
    snapshot: Snapshot,
    shown: bool,
    last_inside: Instant,
    revealed: bool,
}

fn inside(r: Rect, p: (f32, f32)) -> bool {
    p.0 >= r.0 && p.0 < r.2 && p.1 >= r.1 && p.1 < r.3
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}
fn scaled(r: Rect, s: f32) -> Rect {
    (r.0 * s, r.1 * s, r.2 * s, r.3 * s)
}

fn across_gap(island: Rect, panel: Rect, point: (f32, f32), padding: f32) -> bool {
    let top = panel.1.min(island.1) - padding;
    let bottom = panel.3.max(island.3) + padding;
    if panel.0 >= island.2 {
        inside((island.2 - padding, top, panel.0 + padding, bottom), point)
    } else if panel.2 <= island.0 {
        inside((panel.2 - padding, top, island.0 + padding, bottom), point)
    } else {
        false
    }
}

fn app_name(snapshot: &Snapshot) -> String {
    snapshot
        .active
        .as_ref()
        .map(|window| {
            if window.island_settings {
                return "Night Island".into();
            }
            std::path::Path::new(&window.path).file_stem().unwrap_or_default().to_string_lossy().into_owned()
        })
        .unwrap_or_else(|| tr("Desktop").into_owned())
}

fn block_width(block: Block) -> f32 {
    match block {
        Block::Volume | Block::Battery => 68.0,
        Block::Keyboard => 50.0,
        _ => 42.0,
    }
}

fn blocks(settings: &TopPanelSettings) -> Vec<Block> {
    [
        (settings.tray, Block::Tray),
        (settings.wifi, Block::Wifi),
        (settings.bluetooth, Block::Bluetooth),
        (settings.volume, Block::Volume),
        (settings.battery, Block::Battery),
    ]
    .into_iter()
    .filter_map(|(on, block)| on.then_some(block))
    .collect()
}

fn left_blocks(settings: &TopPanelSettings) -> Vec<Block> {
    [
        (settings.microphone, Block::Microphone),
        (settings.keyboard, Block::Keyboard),
        (settings.workspaces, Block::Workspaces),
    ]
    .into_iter()
    .filter_map(|(on, block)| on.then_some(block))
    .collect()
}

fn liquid_panel(
    p: &Painter,
    bounds: Rect,
    bend: f32,
    focus: f32,
    color: windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F,
) -> Result<()> {
    let (l, t, r, b) = bounds;
    let radius = (b - t) / 2.0;
    let mid = (t + b) / 2.0;
    let x = (l + (r - l) * focus).clamp(l + radius, r - radius);
    let k = radius * 0.5523;
    p.fill_curves(
        (l + radius, t),
        &[
            [(x, t - bend), (x, t - bend), (r - radius, t)],
            [(r - radius + k, t), (r + bend * 0.3, mid - k), (r + bend * 0.3, mid)],
            [(r + bend * 0.3, mid + k), (r - radius + k, b), (r - radius, b)],
            [(x, b + bend), (x, b + bend), (l + radius, b)],
            [(l + radius - k, b), (l - bend * 0.3, mid + k), (l - bend * 0.3, mid)],
            [(l - bend * 0.3, mid - k), (l + radius - k, t), (l + radius, t)],
        ],
        color,
    )
}

impl State {
    fn start_frames(&self) {
        if !self.frame_timer.get() {
            self.frame_timer.set(unsafe { SetTimer(Some(self.hwnd), FRAME, 16, None) } != 0);
        }
    }

    fn stop_frames(&self) {
        if self.frame_timer.replace(false) {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), FRAME);
            }
        }
    }

    fn position_at(&self, now: Instant) -> (f32, f32) {
        let progress = self.travel.value(now);
        if let Some(motion) = &self.island_motion
            && let Some((_, left)) = self.boundary
        {
            let bounds = motion.bounds(now);
            let width = self.size.0 as f32;
            let gap = self.settings.island_gap as f32 * self.scale;
            let x =
                if left { bounds.0 - (width + gap) * progress } else { bounds.2 - width + (width + gap) * progress };
            return (x, bounds.1 - 10.0 * self.scale);
        }
        let remaining = 1.0 - self.relocation.value(now);
        let coordinate = |from: i32, to: i32, displacement: f32| {
            from as f32 + (to - from) as f32 * progress + displacement * remaining
        };
        (
            coordinate(self.departure.0, self.destination.0, self.displacement.0),
            coordinate(self.departure.1, self.destination.1, self.displacement.1),
        )
    }

    fn boundary_at(&self, now: Instant) -> Option<(f32, bool)> {
        self.boundary.map(|(edge, left)| {
            (
                self.island_motion.as_ref().map_or(edge, |motion| {
                    let rect = motion.bounds(now);
                    if left { rect.0 } else { rect.2 }
                }),
                left,
            )
        })
    }

    fn remaining(&self, now: Instant) -> std::time::Duration {
        self.travel
            .remaining(now)
            .max(self.relocation.remaining(now))
            .max(self.island_motion.as_ref().map_or(std::time::Duration::ZERO, |m| m.progress.remaining(now)))
    }

    fn animate_reveal(&self) {
        let mut visual = self.reveal.borrow_mut();
        let started = self.boundary.is_some_and(|(edge, left)| {
            if visual.is_none() {
                *visual = panel_reveal::Reveal::new(&self.gpu).ok();
            }
            visual.as_mut().is_some_and(|visual| visual.animate(self, edge, left).is_ok())
        });
        if !started {
            visual.take();
        }
        drop(visual);
        if started {
            self.revealing.set(true);
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        } else if self.revealing.replace(false) {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
        }
    }

    fn motion_frame(&mut self) {
        let now = Instant::now();
        let progress = self.travel.value(now);
        if !matches!(self.role, Role::Card(_)) {
            let (x, y) = self.position_at(now);
            self.origin = (x.round() as i32, y.round() as i32);
        }
        self.alpha = (progress * 255.0).round() as u8;
        self.opening = !self.remaining(now).is_zero();
    }

    fn fit_tray(&mut self, available: f32) {
        self.tray_budget = available;
        let slots = (available.max(0.0) / 30.0).floor() as usize;
        let capacity = if self.tray_icons.len() > slots { slots.saturating_sub(1) } else { slots };
        if capacity == 0 && self.tray_requested {
            self.tray_requested = false;
            self.animate_tray(false);
            self.tray_width = 0.0;
            self.tray_from = 0.0;
            self.dirty = true;
        }
        let capacity = capacity.clamp(1, 6);
        if capacity != self.tray_capacity {
            self.tray_capacity = capacity;
            self.tray_page = 0;
            self.tray_width = self.tray_width.min(self.tray_target());
            self.dirty = true;
            if self.tray_open {
                self.animate_tray(true);
            }
        }
    }

    fn tray_target(&self) -> f32 {
        if self.tray_open {
            (self.tray_icons.len().saturating_sub(self.tray_page).min(self.tray_capacity) as f32
                + if self.tray_icons.len() > self.tray_capacity { 1.0 } else { 0.0 })
                * 30.0
        } else {
            0.0
        }
    }

    fn animate_tray(&mut self, open: bool) {
        self.tray_open = open;
        self.tray_from = self.tray_width;
        self.tray_started = Instant::now();
        unsafe {
            SetTimer(Some(self.hwnd), TRAY_ANIMATION, 16, None);
        }
    }

    fn tray_frame(&mut self) {
        let t = if self.settings.animations && crate::ui::feedback::motion_allowed() {
            (self.tray_started.elapsed().as_secs_f32() / 0.24).min(1.0)
        } else {
            1.0
        };
        self.tray_width = self.tray_from + (self.tray_target() - self.tray_from) * (1.0 - (1.0 - t).powi(3));
        let width = (self.dimensions().0 * self.scale).ceil() as i32;
        if self.settings.entrance != PanelEntrance::Island {
            self.origin.0 += self.size.0 - width;
            self.destination.0 += self.size.0 - width;
        }
        self.departure.0 += self.size.0 - width;
        self.size.0 = width;
        self.layout();
        unsafe {
            if t >= 1.0 {
                let _ = KillTimer(Some(self.hwnd), TRAY_ANIMATION);
            }
        }
        self.render();
    }

    fn tray_action(&mut self, index: usize, context: bool) {
        if let Some(entry) = self.tray_icons.get(index) {
            let anchor = self
                .buttons
                .iter()
                .find(|b| b.hit == Hit::TrayIcon(index))
                .map(|b| {
                    (
                        self.origin.0 + ((b.rect.0 + b.rect.2) * 0.5 * self.scale) as i32,
                        self.origin.1 + (44.0 * self.scale) as i32,
                    )
                })
                .unwrap_or(self.origin);
            self.post(Request::TrayAction(entry.name.clone(), context, anchor));
        }
    }

    fn post(&mut self, request: Request) {
        if matches!(request, Request::Volume(_)) {
            self.requests.retain(|r| !matches!(r, Request::Volume(_)));
        }
        if self.requests.len() < 32 {
            self.requests.push_back(request);
            unsafe {
                let _ = PostMessageW(Some(self.parent), WM_TOP_PANEL, WPARAM(self.id), LPARAM(0));
            }
        }
    }

    fn dimensions(&self) -> (f32, f32) {
        match self.role {
            Role::Left => (
                (if self.settings.application { 214.0 } else { 28.0 })
                    + left_blocks(&self.settings).into_iter().map(block_width).sum::<f32>(),
                HEIGHT + 2.0 * MARGIN,
            ),
            Role::Right => (
                blocks(&self.settings).iter().copied().map(block_width).sum::<f32>()
                    + 2.0 * MARGIN
                    + 8.0
                    + self.tray_width,
                HEIGHT + 2.0 * MARGIN,
            ),
            Role::Card(Block::Application) => (306.0, 210.0 + self.snapshot.windows.len().min(3) as f32 * 32.0),
            Role::Card(Block::Volume) => (306.0, 190.0),
            Role::Card(Block::Tray) => (338.0, 314.0),
            Role::Card(_) => (306.0, 176.0),
        }
    }

    fn layout(&mut self) {
        let (width, _) = self.dimensions();
        let tray_target = self.tray_target();
        self.buttons.clear();
        let mut add = |rect, hit, enabled| self.buttons.push(Button { rect, hit, enabled });
        match self.role {
            Role::Left => {
                if self.settings.application {
                    add((10.0, 10.0, 204.0, 44.0), Hit::Block(Block::Application), true);
                }
                let mut x = if self.settings.application { 214.0 } else { 14.0 };
                for block in left_blocks(&self.settings) {
                    let next = x + block_width(block);
                    add(
                        (x, 12.0, next, 42.0),
                        Hit::Block(block),
                        block != Block::Microphone || self.snapshot.microphone_muted.is_some(),
                    );
                    x = next;
                }
            }
            Role::Right => {
                let trailing = self.settings.entrance == PanelEntrance::Island;
                let base_width: f32 = blocks(&self.settings).into_iter().map(block_width).sum();
                let mut x = 14.0 + if trailing { base_width } else { 0.0 };
                if self.tray_open && self.tray_width >= tray_target - 1.0 {
                    for index in (self.tray_page..self.tray_icons.len()).take(self.tray_capacity) {
                        add((x, 12.0, x + 30.0, 42.0), Hit::TrayIcon(index), true);
                        x += 30.0;
                    }
                    if self.tray_icons.len() > self.tray_capacity {
                        add((x, 12.0, x + 30.0, 42.0), Hit::TrayPage, true);
                    }
                }
                x = 14.0 + if trailing { 0.0 } else { self.tray_width };
                for block in blocks(&self.settings) {
                    let next = x + block_width(block);
                    add((x, 12.0, next, 42.0), Hit::Block(block), true);
                    x = next;
                }
            }
            Role::Card(block) => {
                add((width - 45.0, 20.0, width - 21.0, 44.0), Hit::Close, true);
                match block {
                    Block::Application => {
                        let enabled = self.snapshot.active.is_some();
                        add((24.0, 88.0, 106.0, 132.0), Hit::Minimize, enabled);
                        add((112.0, 88.0, 194.0, 132.0), Hit::Maximize, enabled);
                        add((200.0, 88.0, 282.0, 132.0), Hit::CloseWindow, enabled);
                        for i in 0..self.snapshot.windows.len().min(3) {
                            let y = 142.0 + i as f32 * 32.0;
                            add((24.0, y, width - 24.0, y + 30.0), Hit::Window(i), true);
                        }
                        let y = 146.0 + self.snapshot.windows.len().min(3) as f32 * 32.0;
                        add((24.0, y + 16.0, width - 24.0, y + 44.0), Hit::Settings, true);
                    }
                    Block::Volume => {
                        add((24.0, 79.0, 58.0, 117.0), Hit::Mute, self.snapshot.volume.is_some());
                        add((65.0, 79.0, width - 57.0, 117.0), Hit::Volume, self.snapshot.volume.is_some());
                        add((24.0, 133.0, 150.0, 169.0), Hit::Mixer, true);
                        add((156.0, 133.0, width - 24.0, 169.0), Hit::WindowsSettings, true);
                    }
                    Block::Wifi | Block::Bluetooth => {
                        let available = if block == Block::Wifi {
                            self.snapshot.radios.wifi
                        } else {
                            self.snapshot.radios.bluetooth
                        };
                        add((24.0, 66.0, width - 24.0, 108.0), Hit::Toggle, available.is_some());
                        add((24.0, 123.0, width - 24.0, 155.0), Hit::WindowsSettings, true);
                    }
                    Block::Tray => {
                        add((24.0, 224.0, width - 24.0, 256.0), Hit::WindowsSettings, true);
                        add((24.0, 264.0, width - 24.0, 296.0), Hit::TrayRetry, true);
                    }
                    Block::Battery => add((24.0, 123.0, width - 24.0, 155.0), Hit::WindowsSettings, true),
                    Block::Microphone | Block::Keyboard | Block::Workspaces => {}
                }
            }
        }
    }

    fn draw(&self, p: &Painter) -> Result<()> {
        let s = self.scale;
        let (w, h) = self.dimensions();
        let text = |value: &str, r: Rect, size: f32, strong: bool, alpha: f32| {
            p.text(
                value,
                scaled(r, s),
                size * s,
                if strong { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
                DWRITE_TEXT_ALIGNMENT_LEADING,
                rgba(1.0, 1.0, 1.0, alpha),
            )
        };
        let glyph = |value: char, r: Rect, size: f32, alpha: f32| {
            p.glyph(value, scaled(r, s), size * s, rgba(1.0, 1.0, 1.0, alpha))
        };
        match self.role {
            Role::Card(_) => crate::view::paint_popup_panel(p, (w * s, h * s), s)?,
            _ => {
                let (r, g, b) = crate::ui::theme::panel();
                let panel = scaled((10.0, 10.0, w - 10.0, h - 10.0), s);
                if self.settings.liquid && self.settings.animations && crate::ui::feedback::motion_allowed() {
                    let hover = self.feedback.sample(u64::MAX, if self.hover.is_some() { 1.0 } else { 0.0 }, 0.0, 220);
                    let focus = self
                        .buttons
                        .iter()
                        .find(|button| Some(button.hit) == self.hover)
                        .map_or(0.5, |button| (button.rect.0 + button.rect.2) / (2.0 * w));
                    let focus = self.feedback.sample(u64::MAX - 1, focus, 0.5, 200);
                    let progress = self.travel.value(Instant::now());
                    let bend = (hover * 2.0 + (progress * std::f32::consts::PI).sin() * 3.0) * s;
                    liquid_panel(p, panel, bend, focus, rgba(1.0, 1.0, 1.0, 0.12))?;
                    liquid_panel(
                        p,
                        (panel.0 + s, panel.1 + s, panel.2 - s, panel.3 - s),
                        bend,
                        focus,
                        rgba(r, g, b, 1.0),
                    )?;
                } else {
                    p.fill_round_rect(panel, 17.0 * s, rgba(r, g, b, 0.98))?;
                    p.stroke_round_rect(panel, 17.0 * s, s, rgba(1.0, 1.0, 1.0, 0.09))?;
                }
            }
        }
        for button in &self.buttons {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            button.hit.hash(&mut hasher);
            let target = if !button.enabled {
                0.0
            } else if self.pressed == Some(button.hit) {
                1.6
            } else if self.hover == Some(button.hit) {
                1.0
            } else {
                0.0
            };
            let glow = self.feedback.sample(hasher.finish(), target, 0.0, 140);
            if glow > 0.001 {
                p.fill_round_rect(scaled(button.rect, s), 12.0 * s, rgba(1.0, 1.0, 1.0, 0.085 * glow))?;
            }
        }
        match self.role {
            Role::Left => {
                if self.settings.application {
                    if let Some(icon) = &self.icon {
                        p.image(scaled((22.0, 19.0, 38.0, 35.0), s), 0.0, icon.side, &icon.bgra)?;
                    } else {
                        glyph('\u{E737}', (22.0, 18.0, 40.0, 36.0), 15.0, 0.7)?;
                    }
                    text(&app_name(&self.snapshot), (48.0, 14.0, 174.0, 40.0), 12.0, true, 0.92)?;
                    glyph('\u{E70D}', (180.0, 19.0, 196.0, 35.0), 9.0, 0.45)?;
                }
                for button in &self.buttons {
                    let Hit::Block(block) = button.hit else {
                        continue;
                    };
                    let x = button.rect.0;
                    match block {
                        Block::Keyboard => text(
                            &self.snapshot.keyboard_layout,
                            (x + 7.0, 14.0, button.rect.2 - 4.0, 40.0),
                            12.0,
                            true,
                            0.85,
                        )?,
                        Block::Workspaces => glyph('\u{E7C4}', (x + 12.0, 18.0, x + 30.0, 36.0), 15.0, 0.85)?,
                        Block::Microphone => {
                            glyph(
                                '\u{E720}',
                                (x + 12.0, 18.0, x + 30.0, 36.0),
                                15.0,
                                if self.snapshot.microphone_muted.is_some() { 0.9 } else { 0.3 },
                            )?;
                            if self.snapshot.microphone_muted == Some(true) {
                                p.line(
                                    ((x + 13.0) * s, 34.0 * s),
                                    ((x + 30.0) * s, 18.0 * s),
                                    1.6 * s,
                                    rgba(1.0, 0.4, 0.42, 1.0),
                                )?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Role::Right => {
                let icon_width = (self.tray_icons.len().saturating_sub(self.tray_page).min(self.tray_capacity) as f32
                    + if self.tray_icons.len() > self.tray_capacity { 1.0 } else { 0.0 })
                    * 30.0;
                let trailing = self.settings.entrance == PanelEntrance::Island;
                let tray_x = 14.0
                    + if trailing { blocks(&self.settings).into_iter().map(block_width).sum::<f32>() } else { 0.0 };
                p.push_clip(scaled((tray_x, 14.0, tray_x + self.tray_width, 40.0), s));
                for (offset, entry) in self.tray_icons.iter().skip(self.tray_page).take(self.tray_capacity).enumerate()
                {
                    let x = tray_x + if trailing { 0.0 } else { self.tray_width - icon_width } + offset as f32 * 30.0;
                    if let Some(icon) = &entry.icon {
                        p.image(scaled((x + 6.0, 18.0, x + 24.0, 36.0), s), 0.0, icon.side, &icon.bgra)?;
                    } else {
                        let bluetooth = entry.name.to_lowercase().contains("bluetooth");
                        glyph(
                            if bluetooth { '\u{E702}' } else { '\u{E737}' },
                            (x + 6.0, 18.0, x + 24.0, 36.0),
                            15.0,
                            0.8,
                        )?;
                    }
                }
                p.pop_clip();
                for button in &self.buttons {
                    if matches!(button.hit, Hit::TrayIcon(_)) {
                        continue;
                    }
                    if button.hit == Hit::TrayPage {
                        glyph('\u{E712}', (button.rect.0 + 6.0, 18.0, button.rect.0 + 24.0, 36.0), 14.0, 0.8)?;
                        continue;
                    }
                    let Hit::Block(block) = button.hit else { continue };
                    let x = button.rect.0;
                    if block == Block::Keyboard {
                        text(
                            &self.snapshot.keyboard_layout,
                            (x + 7.0, 14.0, button.rect.2 - 4.0, 40.0),
                            12.0,
                            true,
                            0.85,
                        )?;
                        continue;
                    }
                    if block == Block::Microphone {
                        glyph(
                            '\u{E720}',
                            (x + 12.0, 18.0, x + 30.0, 36.0),
                            15.0,
                            if self.snapshot.microphone_muted.is_some() { 0.9 } else { 0.3 },
                        )?;
                        if self.snapshot.microphone_muted == Some(true) {
                            p.line(
                                ((x + 13.0) * s, 34.0 * s),
                                ((x + 30.0) * s, 18.0 * s),
                                1.6 * s,
                                rgba(1.0, 0.4, 0.42, 1.0),
                            )?;
                        }
                        continue;
                    }
                    let (icon, value, active) = match block {
                        Block::Wifi => ('\u{E701}', String::new(), self.snapshot.radios.wifi == Some(true)),
                        Block::Bluetooth => ('\u{E702}', String::new(), self.snapshot.radios.bluetooth == Some(true)),
                        Block::Tray => {
                            (if self.tray_open != trailing { '\u{E76C}' } else { '\u{E76B}' }, String::new(), true)
                        }
                        Block::Workspaces => ('\u{E7F4}', String::new(), true),
                        Block::Volume => (
                            if self.snapshot.muted { '\u{E74F}' } else { '\u{E767}' },
                            self.snapshot.volume.map(|v| v.to_string()).unwrap_or_else(|| "—".into()),
                            true,
                        ),
                        Block::Battery => (
                            if self.snapshot.battery.is_some() { '\u{E83F}' } else { '\u{E945}' },
                            self.snapshot.battery.map(|(v, _)| format!("{v}%")).unwrap_or_else(|| "AC".into()),
                            true,
                        ),
                        _ => continue,
                    };
                    let gx = if value.is_empty() { x + 12.0 } else { x + 7.0 };
                    glyph(icon, (gx, 18.0, gx + 18.0, 36.0), 15.0, if active { 0.92 } else { 0.4 })?;
                    if !value.is_empty() {
                        text(&value, (gx + 23.0, 14.0, button.rect.2 - 3.0, 40.0), 11.0, false, 0.8)?;
                    }
                }
            }
            Role::Card(block) => {
                let title = match block {
                    Block::Application => app_name(&self.snapshot),
                    Block::Wifi => "Wi-Fi".into(),
                    Block::Bluetooth => "Bluetooth".into(),
                    Block::Volume => tr("System volume").into_owned(),
                    Block::Battery => tr("Battery and power").into_owned(),
                    Block::Tray => tr("System tray").into_owned(),
                    Block::Microphone => tr("Microphone").into_owned(),
                    Block::Keyboard => tr("Keyboard layout").into_owned(),
                    Block::Workspaces => tr("Windows and desktops").into_owned(),
                };
                text(&title, (26.0, 20.0, w - 55.0, 46.0), 14.0, true, 0.96)?;
                glyph('\u{E711}', (w - 43.0, 23.0, w - 23.0, 43.0), 11.0, 0.5)?;
                match block {
                    Block::Application => {
                        text(
                            self.snapshot
                                .active
                                .as_ref()
                                .map(|w| w.title.as_str())
                                .unwrap_or(&tr("No active application")),
                            (26.0, 49.0, w - 26.0, 75.0),
                            11.0,
                            false,
                            0.5,
                        )?;
                        for (x, icon, label, danger) in [
                            (24.0, '\u{E921}', "Minimize window", false),
                            (112.0, '\u{E922}', "Maximize window", false),
                            (200.0, '\u{E711}', "Close", true),
                        ] {
                            p.fill_round_rect(
                                scaled((x, 88.0, x + 82.0, 132.0), s),
                                12.0 * s,
                                if danger { rgba(1.0, 0.3, 0.3, 0.09) } else { rgba(1.0, 1.0, 1.0, 0.06) },
                            )?;
                            glyph(
                                icon,
                                (x + 31.0, 91.0, x + 51.0, 111.0),
                                12.0,
                                if self.snapshot.active.is_some() { 0.85 } else { 0.25 },
                            )?;
                            let label = match label {
                                "Minimize window" => "Minimize",
                                "Maximize window" => "Maximize",
                                other => other,
                            };
                            text(&tr(label), (x + 6.0, 109.0, x + 78.0, 130.0), 9.0, false, 0.6)?;
                        }
                        for (i, window) in self.snapshot.windows.iter().take(3).enumerate() {
                            let y = 142.0 + i as f32 * 32.0;
                            glyph('\u{E737}', (29.0, y + 7.0, 44.0, y + 22.0), 12.0, 0.5)?;
                            text(&window.title, (54.0, y, w - 30.0, y + 30.0), 11.0, false, 0.8)?;
                        }
                        let y = 162.0 + self.snapshot.windows.len().min(3) as f32 * 32.0;
                        text(&tr("Top panel"), (34.0, y, w - 34.0, y + 28.0), 11.0, false, 0.5)?;
                    }
                    Block::Volume => {
                        text(
                            &if self.snapshot.device.is_empty() {
                                tr("Unavailable").into_owned()
                            } else {
                                self.snapshot.device.clone()
                            },
                            (26.0, 48.0, w - 26.0, 71.0),
                            11.0,
                            false,
                            0.5,
                        )?;
                        glyph(
                            if self.snapshot.muted { '\u{E74F}' } else { '\u{E767}' },
                            (29.0, 89.0, 49.0, 109.0),
                            16.0,
                            0.8,
                        )?;
                        let level = self.snapshot.volume.unwrap_or(0) as f32 / 100.0;
                        let x = 69.0 + level * (w - 132.0);
                        p.fill_round_rect(scaled((69.0, 97.0, w - 63.0, 101.0), s), 2.0 * s, rgba(1.0, 1.0, 1.0, 0.2))?;
                        p.fill_round_rect(scaled((69.0, 97.0, x, 101.0), s), 2.0 * s, rgba(1.0, 1.0, 1.0, 0.9))?;
                        p.fill_ellipse(x * s, 99.0 * s, 5.0 * s, 5.0 * s, rgba(1.0, 1.0, 1.0, 1.0))?;
                        text(
                            &self.snapshot.volume.map(|v| v.to_string()).unwrap_or_else(|| "—".into()),
                            (w - 48.0, 85.0, w - 22.0, 113.0),
                            12.0,
                            false,
                            0.6,
                        )?;
                        for (x, label) in [(24.0, "Mixer"), (156.0, "Settings")] {
                            p.fill_round_rect(
                                scaled((x, 133.0, x + 126.0, 169.0), s),
                                12.0 * s,
                                rgba(1.0, 1.0, 1.0, 0.06),
                            )?;
                            text(&tr(label), (x + 12.0, 133.0, x + 116.0, 169.0), 11.0, false, 0.85)?;
                        }
                    }
                    Block::Wifi | Block::Bluetooth => {
                        let value = if block == Block::Wifi {
                            self.snapshot.radios.wifi
                        } else {
                            self.snapshot.radios.bluetooth
                        };
                        p.fill_round_rect(
                            scaled((24.0, 66.0, w - 24.0, 108.0), s),
                            14.0 * s,
                            rgba(1.0, 1.0, 1.0, 0.06),
                        )?;
                        text(
                            &tr(match value {
                                Some(true) => "Enabled",
                                Some(false) => "Disabled",
                                None => "Radio unavailable",
                            }),
                            (38.0, 66.0, w - 89.0, 108.0),
                            12.0,
                            false,
                            0.85,
                        )?;
                        let color = crate::ui::theme::accent();
                        p.fill_round_rect(
                            scaled((w - 78.0, 76.0, w - 40.0, 98.0), s),
                            11.0 * s,
                            if value == Some(true) {
                                rgba(color.0, color.1, color.2, 0.85)
                            } else {
                                rgba(1.0, 1.0, 1.0, 0.14)
                            },
                        )?;
                        p.fill_ellipse(
                            (if value == Some(true) { w - 51.0 } else { w - 67.0 }) * s,
                            87.0 * s,
                            7.0 * s,
                            7.0 * s,
                            rgba(1.0, 1.0, 1.0, 0.9),
                        )?;
                        text(&tr("Open Windows settings"), (34.0, 123.0, w - 34.0, 155.0), 11.0, false, 0.6)?;
                    }
                    Block::Tray => {
                        text(&tr("Could not open the Windows tray"), (26.0, 54.0, w - 26.0, 78.0), 12.0, false, 0.8)?;
                        for (label, y) in [
                            ("1. Open Windows taskbar settings.", 88.0),
                            ("2. Open Other system tray icons.", 114.0),
                            ("3. Enable Hidden icon menu.", 140.0),
                            ("You do not need to enable each application icon.", 163.0),
                            ("Already enabled? Try again below.", 188.0),
                        ] {
                            text(&tr(label), (26.0, y, w - 26.0, y + 22.0), 11.0, false, 0.7)?;
                        }
                        for (label, top) in [("Open Windows settings", 224.0), ("Try again", 264.0)] {
                            p.fill_round_rect(
                                scaled((24.0, top, w - 24.0, top + 32.0), s),
                                10.0 * s,
                                rgba(1.0, 1.0, 1.0, 0.07),
                            )?;
                            text(&tr(label), (36.0, top, w - 36.0, top + 32.0), 11.0, false, 0.9)?;
                        }
                    }
                    Block::Microphone | Block::Keyboard | Block::Workspaces => {}
                    Block::Battery => {
                        let (value, label) = match self.snapshot.battery {
                            Some((v, charging)) => (
                                format!("{v}%"),
                                if charging {
                                    if v == 100 { "Power connected" } else { "Charging" }
                                } else {
                                    "On battery"
                                },
                            ),
                            None => ("AC".into(), "Power connected"),
                        };
                        text(&value, (26.0, 57.0, 119.0, 111.0), 32.0, true, 0.96)?;
                        text(&tr(label), (126.0, 66.0, w - 26.0, 106.0), 12.0, false, 0.6)?;
                        text(&tr("Open Windows settings"), (34.0, 123.0, w - 34.0, 155.0), 11.0, false, 0.6)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn render(&self) {
        if !self.visible {
            return;
        }
        if self.revealing.get() && self.opening && self.boundary.is_some() {
            if let Some(visual) = &*self.reveal.borrow() {
                visual.opacity(self.ghost_alpha);
            }
            return;
        }
        let mut surface = self.surface.borrow_mut();
        if surface.as_ref().is_none_or(|surface| !surface.fits(self.size)) {
            let capacity = if matches!(self.role, Role::Right) {
                ((self.dimensions().0 - self.tray_width + 210.0) * self.scale).ceil() as i32
            } else {
                self.size.0
            };
            *surface = PopupSurface::new((capacity.max(self.size.0), self.size.1)).ok();
        }
        if let Some(surface) = &*surface
            && surface
                .paint_size(&self.gpu, self.size, |p| {
                    if let Some((edge, left)) = self.boundary_at(Instant::now()) {
                        let edge = edge - self.origin.0 as f32;
                        p.push_clip(if left {
                            (0.0, 0.0, edge.max(0.0), self.size.1 as f32)
                        } else {
                            (edge.max(0.0), 0.0, self.size.0 as f32, self.size.1 as f32)
                        });
                    }
                    let result = self.draw(p);
                    if self.boundary.is_some() {
                        p.pop_clip();
                    }
                    result
                })
                .is_ok()
        {
            let _ = surface.present_size(
                self.hwnd,
                Some(POINT { x: self.origin.0, y: self.origin.1 }),
                self.size,
                (u16::from(self.alpha) * u16::from(self.ghost_alpha) / 255) as u8,
            );
        }
        if self.revealing.replace(false) {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            self.reveal.borrow_mut().take();
        }
        if self.feedback.active() {
            self.start_frames();
        }
    }

    fn hide(&mut self) {
        if self.hover.is_some() {
            crate::ui::tip::hide();
        }
        self.hover = None;
        self.visible = false;
        self.revealing.set(false);
        self.reveal.borrow_mut().take();
        self.opening = false;
        self.island_motion = None;
        self.relocation.show(true, 0, Instant::now());
        self.travel.show(false, 0, Instant::now());
        self.pressed = None;
        self.stop_frames();
        if matches!(self.role, Role::Right) {
            self.tray_open = false;
            self.tray_requested = false;
            self.tray_width = 0.0;
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), DISMISS);
            let _ = KillTimer(Some(self.hwnd), TRAY_ANIMATION);
            let _ = PostMessageW(Some(self.hwnd), WM_APP + 93, WPARAM(0), LPARAM(0));
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    fn volume_at(&mut self, x: f32) {
        let value = (((x - 69.0) / (self.dimensions().0 - 132.0)).clamp(0.0, 1.0) * 100.0).round() as u8;
        self.snapshot.volume = Some(value);
        self.snapshot.muted = value == 0;
        self.post(Request::Volume(value));
        self.render();
    }

    fn activate(&mut self, hit: Hit) {
        match hit {
            Hit::Block(Block::Microphone) => {
                if self.snapshot.microphone_muted.is_some() {
                    self.post(Request::Microphone);
                }
            }
            Hit::Block(Block::Keyboard) => self.post(Request::Keyboard),
            Hit::Block(Block::Workspaces) => self.post(Request::Workspaces),
            Hit::TrayIcon(index) => self.tray_action(index, false),
            Hit::TrayPage => {
                self.tray_page = if self.tray_page + self.tray_capacity >= self.tray_icons.len() {
                    0
                } else {
                    self.tray_page + self.tray_capacity
                };
                self.animate_tray(true);
            }
            Hit::Block(block) => self.post(Request::Open(block)),
            Hit::Settings => {
                self.post(Request::Settings);
                self.hide();
            }
            Hit::Close => self.hide(),
            Hit::TrayRetry => {
                self.post(Request::Open(Block::Tray));
                self.hide();
            }
            Hit::Toggle => {
                if let Role::Card(block) = self.role {
                    self.post(if block == Block::Wifi { Request::ToggleWifi } else { Request::ToggleBluetooth });
                }
            }
            Hit::Mute => self.post(Request::Mute),
            Hit::Mixer => {
                self.post(Request::Mixer);
                self.hide();
            }
            Hit::WindowsSettings => {
                if let Role::Card(block) = self.role {
                    self.post(Request::WindowsSettings(block));
                    self.hide();
                }
            }
            Hit::Minimize | Hit::Maximize | Hit::CloseWindow => {
                if let Some(window) = self.snapshot.active.clone() {
                    let action = match hit {
                        Hit::Minimize => desktop::WindowAction::Minimize,
                        Hit::CloseWindow => desktop::WindowAction::Close,
                        _ => {
                            if unsafe { IsZoomed(HWND(window.handle as *mut _)).as_bool() } {
                                desktop::WindowAction::Restore
                            } else {
                                desktop::WindowAction::Maximize
                            }
                        }
                    };
                    self.post(Request::Window(window, action));
                    self.hide();
                }
            }
            Hit::Window(index) => {
                if let Some(window) = self.snapshot.windows.get(index).cloned() {
                    self.post(Request::Activate(window));
                    self.hide();
                }
            }
            Hit::Volume => {}
        }
    }
}

extern "system" fn proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, message, w, l);
        }
        match message {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                (*ptr).render();
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let state = &*ptr;
                let screen_x = l.0 as i16 as f32;
                let clipped =
                    state.boundary.is_some_and(|(edge, left)| if left { screen_x >= edge } else { screen_x < edge });
                if state.click_through || state.alpha == 0 || clipped {
                    return LRESULT(HTTRANSPARENT as isize);
                }
                let point = (
                    (l.0 as i16 as i32 - state.origin.0) as f32 / state.scale,
                    ((l.0 >> 16) as i16 as i32 - state.origin.1) as f32 / state.scale,
                );
                let (width, height) = state.dimensions();
                let hit = crate::ui::popup::hit_panel(point, (width, height), 1.0);
                LRESULT(if hit { HTCLIENT as isize } else { HTTRANSPARENT as isize })
            }
            WM_MOUSEMOVE => {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    ..Default::default()
                });
                let state = &mut *ptr;
                let point = (l.0 as i16 as f32 / state.scale, (l.0 >> 16) as i16 as f32 / state.scale);
                if state.pressed == Some(Hit::Volume) {
                    state.volume_at(point.0);
                }
                let hover = state.buttons.iter().find(|b| inside(b.rect, point)).map(|b| b.hit);
                if state.hover != hover {
                    state.hover = hover;
                    if let Some(Hit::TrayIcon(index)) = hover {
                        if let Some(entry) = state.tray_icons.get(index) {
                            crate::ui::tip::show(
                                &state.gpu,
                                &entry.name,
                                (
                                    state.origin.0 + (point.0 * state.scale) as i32,
                                    state.origin.1 + (54.0 * state.scale) as i32,
                                ),
                                state.scale,
                            );
                        }
                    } else if let Some(Hit::Block(block)) = hover {
                        let label = match block {
                            Block::Microphone => Some(if state.snapshot.microphone_muted == Some(true) {
                                "Unmute microphone"
                            } else if state.snapshot.microphone_muted == Some(false) {
                                "Mute microphone"
                            } else {
                                "Microphone unavailable"
                            }),
                            Block::Keyboard => Some("Switch keyboard layout"),
                            Block::Workspaces => Some("Windows and desktops"),
                            _ => None,
                        };
                        if let Some(label) = label {
                            crate::ui::tip::show(
                                &state.gpu,
                                &tr(label),
                                (
                                    state.origin.0 + (point.0 * state.scale) as i32,
                                    state.origin.1 + (54.0 * state.scale) as i32,
                                ),
                                state.scale,
                            );
                        } else {
                            crate::ui::tip::hide();
                        }
                    } else {
                        crate::ui::tip::hide();
                    }
                    state.render();
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                let state = &mut *ptr;
                if state.hover.is_some() {
                    crate::ui::tip::hide();
                }
                if state.hover.take().is_some() {
                    state.render();
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let state = &mut *ptr;
                let point = (l.0 as i16 as f32 / state.scale, (l.0 >> 16) as i16 as f32 / state.scale);
                state.pressed = state.buttons.iter().find(|b| b.enabled && inside(b.rect, point)).map(|b| b.hit);
                if state.pressed == Some(Hit::Volume) {
                    state.volume_at(point.0);
                    SetCapture(hwnd);
                }
                state.render();
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let state = &mut *ptr;
                let point = (l.0 as i16 as f32 / state.scale, (l.0 >> 16) as i16 as f32 / state.scale);
                let hit = state.buttons.iter().find(|b| b.enabled && inside(b.rect, point)).map(|b| b.hit);
                if let Some(hit) = hit.filter(|hit| Some(*hit) == state.pressed.take()) {
                    state.activate(hit);
                }
                state.pressed = None;
                if state.visible {
                    state.render();
                }
                if GetCapture() == hwnd {
                    let _ = ReleaseCapture();
                }
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                let state = &mut *ptr;
                let point = (l.0 as i16 as f32 / state.scale, (l.0 >> 16) as i16 as f32 / state.scale);
                if let Some(Hit::TrayIcon(index)) =
                    state.buttons.iter().find(|b| b.enabled && inside(b.rect, point)).map(|b| b.hit)
                {
                    state.tray_action(index, true);
                } else {
                    state.post(Request::Settings);
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                (*ptr).pressed = None;
                LRESULT(0)
            }
            m if m == WM_APP + 93 => {
                if GetCapture() == hwnd {
                    let _ = ReleaseCapture();
                }
                LRESULT(0)
            }
            WM_TIMER => {
                let state = &mut *ptr;
                if w.0 == TRAY_ANIMATION {
                    state.tray_frame();
                } else if w.0 == FRAME {
                    state.motion_frame();
                    if !state.travel.shown() && state.alpha == 0 {
                        state.hide();
                        return LRESULT(0);
                    }
                    state.render();
                    if !state.opening && !state.feedback.active() {
                        state.stop_frames();
                    }
                } else if w.0 == DISMISS {
                    let mut point = POINT::default();
                    let _ = GetCursorPos(&mut point);
                    let outside = !inside(
                        (
                            state.origin.0 as f32,
                            state.origin.1 as f32,
                            (state.origin.0 + state.size.0) as f32,
                            (state.origin.1 + state.size.1) as f32,
                        ),
                        (point.x as f32, point.y as f32),
                    );
                    if GetAsyncKeyState(VK_ESCAPE.0 as i32) < 0
                        || (outside && GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 && state.pressed.is_none())
                    {
                        state.hide();
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                (*ptr).hide();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, w, l),
        }
    }
}

impl Wing {
    fn new(parent: HWND, gpu: &Gpu, role: Role, id: usize) -> Result<Self> {
        static CLASS: Once = Once::new();
        let instance = unsafe { GetModuleHandleW(None)? };
        CLASS.call_once(|| unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("NightIsland.TopPanel"),
                ..Default::default()
            });
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("NightIsland.TopPanel"),
                w!("Night Island Top Panel"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        let mut state = Box::new(State {
            hwnd,
            parent,
            id,
            gpu: gpu.popup(),
            role,
            settings: Default::default(),
            snapshot: Default::default(),
            icon: None,
            surface: RefCell::new(None),
            size: (1, 1),
            origin: (0, 0),
            scale: 1.0,
            buttons: Vec::new(),
            hover: None,
            pressed: None,
            requests: VecDeque::new(),
            visible: false,
            opening: false,
            travel: Default::default(),
            relocation: Default::default(),
            displacement: (0.0, 0.0),
            frame_timer: Cell::new(false),
            reveal: RefCell::new(None),
            revealing: Cell::new(false),
            destination: (0, 0),
            departure: (0, 0),
            boundary: None,
            island_motion: None,
            motion_changed: false,
            alpha: 255,
            ghost_alpha: 255,
            click_through: false,
            dirty: true,
            feedback: Default::default(),
            tray_icons: Vec::new(),
            tray_open: false,
            tray_requested: false,
            tray_page: 0,
            tray_capacity: 6,
            tray_budget: 210.0,
            tray_width: 0.0,
            tray_from: 0.0,
            tray_started: Instant::now(),
        });
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&raw mut *state) as isize);
        }
        Ok(Self { state })
    }

    fn place(&mut self, origin: (i32, i32), scale: f32, visible: bool) {
        let now = Instant::now();
        let animated = self.state.settings.animations && crate::ui::feedback::motion_allowed();
        let ms = if animated { self.state.settings.motion_ms } else { 0 };
        if !visible {
            if self.state.visible {
                let changed = self.state.travel.shown();
                self.state.travel.show(false, ms, now);
                if self.state.travel.active(now) {
                    if changed {
                        self.state.motion_frame();
                        self.state.animate_reveal();
                    }
                    self.state.start_frames();
                } else {
                    self.state.hide();
                }
            }
            return;
        }
        let fresh = !self.state.visible;
        let old_origin = self.state.origin;
        let (w, h) = self.state.dimensions();
        let size = ((w * scale).ceil() as i32, (h * scale).ceil() as i32);
        let moved = size != self.state.size || origin != self.state.destination;
        self.state.scale = scale;
        self.state.size = size;
        self.state.destination = origin;
        if matches!(self.state.role, Role::Card(_)) {
            self.state.origin = origin;
            self.state.departure = origin;
        }
        let reversed = !self.state.travel.shown();
        self.state.travel.show(true, ms, now);
        if fresh || (moved && !matches!(self.state.role, Role::Card(_))) {
            self.state.displacement = (0.0, 0.0);
            self.state.relocation.show(true, 0, now);
            self.state.motion_frame();
            if !fresh && ms > 0 && self.state.island_motion.is_none() {
                self.state.displacement =
                    ((old_origin.0 - self.state.origin.0) as f32, (old_origin.1 - self.state.origin.1) as f32);
                self.state.relocation.show(false, 0, now);
                self.state.relocation.show(true, ms.min(280), now);
            }
        }
        self.state.motion_frame();
        self.state.visible = true;
        self.state.layout();
        if self.state.opening && (fresh || moved || reversed || self.state.motion_changed) {
            self.state.animate_reveal();
        }
        self.state.motion_changed = false;
        unsafe {
            if fresh && matches!(self.state.role, Role::Card(_)) {
                SetTimer(Some(self.state.hwnd), DISMISS, 50, None);
            }
            if self.state.opening {
                self.state.start_frames();
            }
            if fresh || moved || old_origin != self.state.origin || self.state.dirty {
                self.state.render();
                self.state.dirty = false;
            }
            if fresh && !self.state.revealing.get() {
                let _ = ShowWindow(self.state.hwnd, SW_SHOWNOACTIVATE);
            }
        }
    }
}

impl Drop for Wing {
    fn drop(&mut self) {
        self.state.hide();
        unsafe {
            if GetCapture() == self.state.hwnd {
                let _ = ReleaseCapture();
            }
            SetWindowLongPtrW(self.state.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.state.hwnd);
        }
    }
}

impl TopPanel {
    pub(crate) fn sync_island_motion(&mut self, motion: Option<IslandMotion>) {
        let linked = self.settings.entrance == PanelEntrance::Island
            && self.settings.animations
            && crate::ui::feedback::motion_allowed();
        for wing in [&mut self.left, &mut self.right] {
            let next = motion.clone().filter(|_| linked);
            wing.state.motion_changed |= wing.state.island_motion.as_ref().map(|m| m.progress.started())
                != next.as_ref().map(|m| m.progress.started());
            wing.state.island_motion = next;
        }
    }
    pub fn animating(&self) -> bool {
        let now = Instant::now();
        [&self.left, &self.right, &self.card]
            .iter()
            .any(|wing| wing.state.visible && !wing.state.remaining(now).is_zero())
            || (self.right.state.tray_width - self.right.state.tray_target()).abs() > 0.1
    }

    pub fn interacting_at(&self, island: Rect, point: (f32, f32)) -> bool {
        if !self.shown {
            return false;
        }
        let wings = [&self.left, &self.right, &self.card];
        let interactive = |state: &State| state.visible && state.travel.shown() && !state.click_through;
        if wings.iter().any(|wing| interactive(&wing.state)) && crate::platform::system_tray::is_open() {
            return true;
        }
        wings.iter().any(|wing| {
            let state = &wing.state;
            if !interactive(state) {
                return false;
            }
            let bounds = (
                state.origin.0 as f32,
                state.origin.1 as f32,
                (state.origin.0 + state.size.0) as f32,
                (state.origin.1 + state.size.1) as f32,
            );
            let destination = (
                state.destination.0 as f32,
                state.destination.1 as f32,
                (state.destination.0 + state.size.0) as f32,
                (state.destination.1 + state.size.1) as f32,
            );
            let connected = |bounds| {
                inside(bounds, point)
                    || (self.settings.entrance == PanelEntrance::Island
                        && !matches!(state.role, Role::Card(_))
                        && across_gap(island, bounds, point, 12.0 * self.scale))
            };
            state.pressed.is_some() || connected(bounds) || (state.opening && connected(destination))
        })
    }

    pub(super) fn parent(&self) -> HWND {
        self.left.state.parent
    }

    pub fn new(parent: HWND, gpu: &Gpu, settings: &TopPanelSettings) -> Result<Self> {
        let monitor = desktop::monitors().into_iter().find(|m| m.primary).ok_or_else(windows::core::Error::empty)?;
        Ok(Self {
            left: Wing::new(parent, gpu, Role::Left, 0)?,
            right: Wing::new(parent, gpu, Role::Right, 1)?,
            card: Wing::new(parent, gpu, Role::Card(Block::Volume), 2)?,
            settings: settings.clone(),
            icons: apps::DockIconCache::new(),
            reservation: None,
            monitor,
            scale: 1.0,
            snapshot: Default::default(),
            shown: false,
            last_inside: Instant::now(),
            revealed: false,
        })
    }

    pub fn suppressed(&self) -> bool {
        self.settings.hide_fullscreen && desktop::fullscreen(&self.monitor)
    }

    pub fn repaint(&mut self) {
        for wing in [&mut self.left, &mut self.right, &mut self.card] {
            wing.state.dirty = true;
            if wing.state.visible {
                wing.state.render();
            }
        }
    }

    pub fn configure(&mut self, settings: &TopPanelSettings, scale: f32) {
        self.card.state.hide();
        self.settings = settings.clone();
        self.revealed = !settings.auto_hide;
        self.last_inside = Instant::now();
        self.scale = scale;
        self.reflow();
        for wing in [&mut self.left, &mut self.right, &mut self.card] {
            wing.state.settings = settings.clone();
            wing.state.feedback.set_enabled(settings.animations && crate::ui::feedback::motion_allowed());
            wing.state.dirty = true;
        }
    }

    pub fn top(&self) -> f32 {
        self.reservation.as_ref().map_or(self.monitor.work.top, |r| r.rect().top) as f32
    }

    pub fn update(&mut self, snapshot: Snapshot, island: Rect, hidden: bool) {
        let active_changed =
            snapshot.active.as_ref().map(|w| w.handle) != self.snapshot.active.as_ref().map(|w| w.handle);
        if active_changed {
            self.card.state.hide();
        }
        self.snapshot = snapshot;
        let fullscreen = self.suppressed();
        let mut point = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut point);
        }
        let at_edge = point.x >= self.monitor.bounds.left
            && point.x < self.monitor.bounds.right
            && point.y >= self.monitor.bounds.top
            && point.y <= self.monitor.bounds.top + 3;
        let over_panel = [&self.left, &self.right, &self.card].iter().any(|wing| {
            let state = &wing.state;
            state.visible
                && inside(
                    (
                        state.origin.0 as f32,
                        state.origin.1 as f32,
                        (state.origin.0 + state.size.0) as f32,
                        (state.origin.1 + state.size.1) as f32,
                    ),
                    (point.x as f32, point.y as f32),
                )
        });
        let interacting = self.card.state.pressed.is_some()
            || crate::platform::system_tray::is_open()
            || (self.card.state.visible && matches!(self.card.state.role, Role::Card(Block::Tray)));
        let over_island =
            self.settings.entrance == PanelEntrance::Island && inside(island, (point.x as f32, point.y as f32));
        let over_panel = over_panel || self.interacting_at(island, (point.x as f32, point.y as f32));
        if !fullscreen && (at_edge || over_panel || over_island || interacting) {
            self.last_inside = Instant::now();
            self.revealed = true;
        }
        if self.last_inside.elapsed().as_millis() > 650 && !over_panel && !over_island && !interacting {
            self.revealed = false;
        }
        let suppressed = hidden || fullscreen || (self.settings.auto_hide && !self.revealed);
        self.shown = !suppressed;
        let icons_changed = self.icons.poll();
        let icon =
            self.snapshot.active.as_ref().and_then(|w| self.icons.get(&w.path, (20.0 * self.scale).ceil() as u32));
        let scale = self.scale;
        let y = if self.settings.entrance == PanelEntrance::Island {
            (island.1 - 10.0 * scale).round() as i32
        } else {
            self.top() as i32 - (2.0 * scale) as i32
        };
        for wing in [&mut self.left, &mut self.right, &mut self.card] {
            let dragging = wing.state.pressed == Some(Hit::Volume);
            let volume = wing.state.snapshot.volume;
            wing.state.dirty |= wing.state.snapshot != self.snapshot || icons_changed;
            wing.state.snapshot = self.snapshot.clone();
            if dragging {
                wing.state.snapshot.volume = volume;
            }
            wing.state.icon = icon.clone();
        }
        let mut left_x = self.monitor.work.left + (6.0 * scale) as i32;
        let controls = if self.settings.clear_window_controls { 144.0 * scale } else { 0.0 };
        let edge = self.monitor.work.right - ((6.0 * scale) + controls) as i32;
        let left_bound = if (y as f32) < island.3 + 8.0 * scale && y as f32 + 54.0 * scale > island.1 - 8.0 * scale {
            (island.2 + 16.0 * scale).max(self.monitor.work.left as f32)
        } else {
            self.monitor.work.left as f32
        };
        let base = self.right.state.dimensions().0 - self.right.state.tray_width;
        self.right.state.fit_tray((edge as f32 - left_bound) / scale - base);
        let right_w = self.right.state.dimensions().0 * scale;
        let mut right_x = self.monitor.work.right - right_w.ceil() as i32 - ((6.0 * scale) + controls) as i32;
        let left_w = self.left.state.dimensions().0 * scale;
        if self.settings.entrance == PanelEntrance::Island {
            left_x = (island.0 - self.settings.island_gap as f32 * scale - left_w).round() as i32;
            right_x = (island.2 + self.settings.island_gap as f32 * scale).round() as i32;
        }
        for (wing, x, left) in [(&mut self.left, left_x, true), (&mut self.right, right_x, false)] {
            let width = (wing.state.dimensions().0 * scale).ceil() as i32;
            wing.state.boundary = (self.settings.entrance == PanelEntrance::Island)
                .then_some((if left { island.0 } else { island.2 }, left));
            wing.state.departure = match self.settings.entrance {
                PanelEntrance::Island => (if left { island.0 as i32 } else { island.2 as i32 - width }, y),
                PanelEntrance::Sides => {
                    (if left { self.monitor.bounds.left - width } else { self.monitor.bounds.right }, y)
                }
                PanelEntrance::Top => (x, self.monitor.bounds.top - (54.0 * scale).ceil() as i32),
            };
            if (hidden || fullscreen) && wing.state.visible {
                wing.state.hide();
            }
        }
        let collision = |x: i32, width: f32| {
            overlaps(
                (x as f32, y as f32, x as f32 + width * scale, y as f32 + 54.0 * scale),
                (island.0 - 8.0 * scale, island.1 - 8.0 * scale, island.2 + 8.0 * scale, island.3 + 8.0 * scale),
            )
        };
        self.left.place(
            (left_x, y),
            scale,
            !suppressed
                && (self.settings.application || !left_blocks(&self.settings).is_empty())
                && left_x >= self.monitor.work.left
                && !collision(left_x, self.left.state.dimensions().0),
        );
        self.right.place(
            (right_x, y),
            scale,
            !suppressed
                && right_x + right_w.ceil() as i32 <= edge
                && !blocks(&self.settings).is_empty()
                && !collision(right_x, self.right.state.dimensions().0),
        );
        if suppressed {
            self.card.state.hide();
        } else if self.card.state.visible {
            let card = &self.card.state;
            let bounds = (
                card.origin.0 as f32,
                card.origin.1 as f32,
                (card.origin.0 + card.size.0) as f32,
                (card.origin.1 + card.size.1) as f32,
            );
            if overlaps(bounds, island) {
                self.card.state.hide();
            } else {
                self.card.place(self.card.state.origin, self.scale, true);
            }
        }
    }

    pub fn open(&mut self, block: Block) {
        if !self.shown {
            return;
        }
        if block == Block::Tray {
            self.card.state.hide();
            if self.right.state.tray_requested {
                self.right.state.tray_requested = false;
                self.right.state.animate_tray(false);
                return;
            }
            self.right.state.tray_requested = true;
            self.right.state.tray_page = 0;
            self.right.state.animate_tray(true);
            self.right.state.post(Request::Tray((
                self.right.state.origin.0 + self.right.state.size.0,
                self.top() as i32 + (52.0 * self.scale) as i32,
            )));
            return;
        }
        if matches!(self.card.state.role, Role::Card(current) if current == block) && self.card.state.visible {
            self.card.state.hide();
            return;
        }
        self.show_card(block);
    }

    pub fn tray_help(&mut self) {
        if self.suppressed() {
            return;
        }
        self.revealed = true;
        self.last_inside = Instant::now();
        self.show_card(Block::Tray);
    }

    pub fn tray_failed(&mut self) {
        if self.right.state.tray_requested {
            self.tray_help();
        }
    }

    pub fn set_tray_icons(&mut self, entries: Vec<crate::platform::system_tray::inline::Entry>) {
        let target = self.right.state.tray_target();
        self.right.state.tray_icons = entries;
        self.right.state.fit_tray(self.right.state.tray_budget);
        if self.right.state.tray_page >= self.right.state.tray_icons.len() {
            self.right.state.tray_page = 0;
        }
        self.right.state.dirty = true;
        if self.right.state.tray_requested && (!self.right.state.tray_open || self.right.state.tray_target() != target)
        {
            self.right.state.animate_tray(true);
        }
    }

    fn show_card(&mut self, block: Block) {
        self.card.state.hide();
        self.card.state.role = Role::Card(block);
        self.card.state.dirty = true;
        self.card.state.snapshot = self.snapshot.clone();
        let width = self.card.state.dimensions().0 * self.scale;
        let x = if block == Block::Application {
            self.left.state.origin.0
        } else {
            self.right.state.origin.0 + self.right.state.size.0 - width as i32
        };
        let x = x.clamp(self.monitor.work.left, (self.monitor.work.right - width as i32).max(self.monitor.work.left));
        let y =
            if block == Block::Application { self.left.state.destination.1 } else { self.right.state.destination.1 }
                + (54.0 * self.scale) as i32;
        self.card.place((x, y), self.scale, true);
    }

    pub fn take_request(&mut self, id: usize) -> Option<Request> {
        match id {
            0 => self.left.state.requests.pop_front(),
            1 => self.right.state.requests.pop_front(),
            2 => self.card.state.requests.pop_front(),
            _ => None,
        }
    }

    pub fn set_ghost(&mut self, opacity: f32, through: bool) {
        for wing in [&mut self.left, &mut self.right, &mut self.card] {
            let state = &mut wing.state;
            let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            if state.ghost_alpha == alpha && state.click_through == through {
                continue;
            }
            state.ghost_alpha = alpha;
            state.click_through = through;
            if through {
                state.hover = None;
                crate::ui::tip::hide();
                state.pressed = None;
                if unsafe { GetCapture() } == state.hwnd {
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                }
            }
            unsafe {
                let style = GetWindowLongPtrW(state.hwnd, GWL_EXSTYLE);
                let next = if through {
                    style | WS_EX_TRANSPARENT.0 as isize
                } else {
                    style & !(WS_EX_TRANSPARENT.0 as isize)
                };
                if style != next {
                    SetWindowLongPtrW(state.hwnd, GWL_EXSTYLE, next);
                }
            }
            if state.visible {
                state.render();
            }
        }
    }

    pub fn set_capture_hidden(&self, hidden: bool) {
        for wing in [&self.left, &self.right, &self.card] {
            if let Some(visual) = &*wing.state.reveal.borrow() {
                visual.capture_hidden(hidden);
            }
            unsafe {
                let _ =
                    SetWindowDisplayAffinity(wing.state.hwnd, if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
            }
        }
    }
}

impl Drop for TopPanel {
    fn drop(&mut self) {
        self.reservation.take();
    }
}

#[cfg(test)]
#[path = "top_panel_tests.rs"]
mod tests;
