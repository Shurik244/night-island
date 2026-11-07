use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{CompositionRoundedRectangleGeometry, Compositor, ContainerVisual, SpriteVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::DirectWrite::{DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER};
use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRoundRectRgn, DeleteObject, RGN_OR, SetWindowRgn};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_MENU,
};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, Result, w};
use windows_numerics::{Vector2, Vector3};

use crate::data::desktop_dock::{DockSettings, Pin, key};
use crate::platform::{apps, desktop_dock as desktop};
use crate::ui::feedback::{Feedback, motion_allowed};
use crate::ui::gfx::{Gpu, Layer, rgba};
use crate::view::Rect;

pub const WM_DESKTOP_DOCK: u32 = WM_APP + 70;
const PULSE: usize = 1;
const FRAME: usize = 2;
const BACKDROP_REFRESH: usize = 3;
const PREVIEW_HOVER: usize = 4;

#[derive(Clone)]
pub struct Entry {
    pub pin: Pin,
    pub pinned: bool,
    pub windows: Vec<desktop::Window>,
    pub(super) icon: Option<Arc<apps::Icon>>,
}

pub enum Request {
    Save,
    Launch(Entry),
    Menu(Option<Entry>),
    Capture(Entry),
    Add,
}

struct State {
    hwnd: HWND,
    parent: HWND,
    config: DockSettings,
    gpu: Gpu,
    content: Layer,
    backdrop: Option<(SpriteVisual, CompositionRoundedRectangleGeometry)>,
    icon_bitmaps: RefCell<crate::ui::gfx::IconBitmapCache>,
    icons: apps::DockIconCache,
    _target: DesktopWindowTarget,
    _root: ContainerVisual,
    entries: Vec<Entry>,
    scan: Vec<desktop::Window>,
    targets: HashMap<String, String>,
    monitor: desktop::Monitor,
    scale: f32,
    size: (i32, i32),
    origin: (i32, i32),
    rects: Vec<(usize, Rect)>,
    panel: Rect,
    region_shapes: Vec<[i32; 6]>,
    feedback: Feedback,
    frame_pending: bool,
    hint_label: Option<(usize, String)>,
    hint_alpha: f32,
    pointer: Option<(f32, f32)>,
    hover: Option<usize>,
    pressed: Option<(usize, f32)>,
    dragging: bool,
    drag_snapshot: Option<Vec<Pin>>,
    drop_slots: Vec<(usize, f32)>,
    leave_at: Instant,
    scanned_at: Instant,
    hidden: bool,
    suppressed: bool,
    offset: usize,
    request: Option<Request>,
    menu_open: bool,
    allowed_visible: bool,
    resumed: bool,
    preview: Option<crate::ui::dock_preview::Preview>,
    actions: Arc<crate::platform::dock_actions::Client>,
    preview_candidate: Option<(String, Instant)>,
    capture_hidden: std::cell::Cell<bool>,
}

pub struct Dock {
    state: Box<State>,
}

fn inside(rect: Rect, point: (f32, f32)) -> bool {
    point.0 >= rect.0 && point.0 < rect.2 && point.1 >= rect.1 && point.1 < rect.3
}

pub fn capacity(width: f32, scale: f32, icon: f32, zoom: f32) -> usize {
    (((width / scale - 48.0 - icon * (zoom - 1.0) * 3.0) / (icon + 12.0)).floor() as usize).clamp(2, 64)
}

fn motion_key(target: &str, channel: u64) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut hash);
    (hash.finish() | (1 << 63)) ^ (channel << 56)
}

fn region_shape(rect: Rect, radius: f32, padding: f32) -> [i32; 6] {
    let diameter = ((radius + padding) * 2.0).ceil() as i32;
    [
        (rect.0 - padding).floor() as i32,
        (rect.1 - padding).floor() as i32,
        (rect.2 + padding).ceil() as i32 + 1,
        (rect.3 + padding).ceil() as i32 + 1,
        diameter,
        diameter,
    ]
}

pub(super) fn create_backdrop(
    hwnd: HWND,
    compositor: &Compositor,
    root: &ContainerVisual,
) -> Option<(SpriteVisual, CompositionRoundedRectangleGeometry)> {
    use windows::Win32::Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute};
    let enabled = windows::core::BOOL::from(true);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_HOSTBACKDROPBRUSH,
            (&raw const enabled).cast(),
            size_of::<windows::core::BOOL>() as u32,
        );
    }
    crate::island::appearance::set_host_backdrop(hwnd, true);
    let sprite = compositor.CreateSpriteVisual().ok()?;
    sprite.SetBrush(&compositor.CreateHostBackdropBrush().ok()?).ok()?;
    let geometry = compositor.CreateRoundedRectangleGeometry().ok()?;
    sprite.SetClip(&compositor.CreateGeometricClipWithGeometry(&geometry).ok()?).ok()?;
    root.Children().ok()?.InsertAtBottom(&sprite).ok()?;
    Some((sprite, geometry))
}

pub(super) fn paint_material(
    p: &crate::ui::gfx::Painter,
    panel: Rect,
    radius: f32,
    s: f32,
    config: &DockSettings,
) -> Result<()> {
    let (r, g, b) = crate::ui::theme::hsv(config.tint_hue, config.tint_saturation, config.tint_brightness);
    let alpha = if config.blur { config.blur_opacity } else { config.opacity };
    p.fill_round_gradient(
        panel,
        radius,
        rgba((r + 0.045).min(1.0), (g + 0.045).min(1.0), (b + 0.05).min(1.0), alpha),
        rgba(r, g, b, alpha),
    )?;
    p.stroke_round_rect(panel, radius, s, rgba(1.0, 1.0, 1.0, 0.19))?;
    p.stroke_round_rect(
        (panel.0 + s, panel.1 + s, panel.2 - s, panel.3 - s),
        (radius - s).max(0.0),
        s,
        rgba(0.0, 0.0, 0.0, 0.12),
    )
}

impl State {
    fn post(&mut self, request: Request) {
        if self.request.is_none() {
            self.request = Some(request);
            unsafe {
                let _ = PostMessageW(Some(self.parent), WM_DESKTOP_DOCK, WPARAM(0), LPARAM(0));
            }
        }
    }

    fn schedule_backdrop_refresh(&self) {
        if self.config.blur && !self.suppressed && self.backdrop.is_some() {
            unsafe {
                SetTimer(Some(self.hwnd), BACKDROP_REFRESH, 120, None);
            }
        }
    }

    fn refresh_backdrop(&self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), BACKDROP_REFRESH);
        }
        if !self.config.blur || self.suppressed {
            return;
        }
        if let Some((backdrop, _)) = &self.backdrop {
            let _ = backdrop
                .Compositor()
                .and_then(|compositor| compositor.CreateHostBackdropBrush())
                .and_then(|brush| backdrop.SetBrush(&brush));
        }
    }

    fn refresh(&mut self, force: bool) {
        if self.suppressed || self.pressed.is_some() || self.menu_open {
            return;
        }
        let force = force || std::mem::take(&mut self.resumed);
        let scan = desktop::cached_windows();
        self.scanned_at = Instant::now();
        if !force && scan == self.scan {
            return;
        }
        self.scan = scan;
        let previous: Vec<String> = self.entries.iter().map(|e| key(&e.pin.target)).collect();
        let mut entries: Vec<Entry> = self
            .config
            .pins
            .iter()
            .map(|pin| {
                let resolved = self
                    .targets
                    .entry(pin.target.clone())
                    .or_insert_with(|| key(&desktop::resolved_target(&pin.target)));
                Entry {
                    pin: pin.clone(),
                    pinned: true,
                    windows: self.scan.iter().filter(|w| key(&w.path) == *resolved).cloned().collect(),
                    icon: None,
                }
            })
            .collect();
        if self.config.show_running {
            for window in &self.scan {
                if entries.iter().any(|e| e.windows.iter().any(|w| w.handle == window.handle)) {
                    continue;
                }
                let target = key(&window.path);
                if let Some(entry) = entries.iter_mut().find(|e| key(&e.pin.target) == target) {
                    entry.windows.push(window.clone());
                } else {
                    entries.push(Entry {
                        pin: if window.island_settings {
                            Pin { name: window.title.clone(), target: window.path.clone(), icon: String::new() }
                        } else {
                            Pin::from_path(&window.path)
                        },
                        pinned: false,
                        windows: vec![window.clone()],
                        icon: None,
                    });
                }
            }
            let pinned = self.config.pins.len();
            entries[pinned..]
                .sort_by_key(|e| previous.iter().position(|k| *k == key(&e.pin.target)).unwrap_or(usize::MAX));
        }
        if previous != entries.iter().map(|entry| key(&entry.pin.target)).collect::<Vec<_>>() {
            self.hover = None;
            self.pointer = None;
            self.preview_candidate = None;
        }
        self.entries = entries;
        if let Some(preview) = &mut self.preview {
            if let Some(entry) = self.entries.iter().find(|e| e.pin.target == preview.target()) {
                preview.sync_entry(entry);
            } else {
                preview.hide();
            }
        }
        self.targets.retain(|target, _| self.config.pins.iter().any(|p| p.target == *target));
        self.offset = self.offset.min(self.entries.len().saturating_sub(1));
        self.layout();
        self.load_icons();
        self.render();
    }

    fn load_icons(&mut self) {
        let requested = (self.config.icon_size as f32 * self.config.zoom * self.scale).ceil() as u32;
        let slots = capacity(
            (self.monitor.work.right - self.monitor.work.left) as f32,
            self.scale,
            self.config.icon_size as f32,
            self.config.zoom,
        )
        .saturating_sub(usize::from(self.config.show_add));
        let targets: Vec<_> = self
            .entries
            .iter()
            .flat_map(|e| [e.pin.target.clone(), e.pin.icon.clone()])
            .filter(|path| !path.is_empty())
            .collect();
        self.icons.retain(&targets);
        for (i, entry) in self.entries.iter_mut().enumerate() {
            entry.icon = if i >= self.offset && i < self.offset + slots {
                self.icons
                    .get(if entry.pin.icon.is_empty() { &entry.pin.target } else { &entry.pin.icon }, requested)
                    .or_else(|| self.icons.get(&entry.pin.target, requested))
            } else {
                None
            };
        }
        self.icon_bitmaps
            .borrow_mut()
            .retain(self.entries.iter().filter_map(|e| e.icon.as_ref()).map(|icon| Arc::as_ptr(icon) as usize));
    }

    fn update_suppression(&mut self) -> bool {
        let suppressed = (self.config.hide_fullscreen && desktop::fullscreen(&self.monitor))
            || desktop::foreground_app_hidden(&self.monitor, &self.config.hidden_apps);
        self.set_suppressed(suppressed);
        suppressed
    }

    fn set_suppressed(&mut self, suppressed: bool) {
        if suppressed != self.suppressed {
            self.suppressed = suppressed;
            self.hide_preview();
            self.pointer = None;
            self.hover = None;
            self.pressed = None;
            self.dragging = false;
            self.frame_pending = false;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), FRAME);
                if suppressed {
                    let _ = KillTimer(Some(self.hwnd), BACKDROP_REFRESH);
                    let _ = PostMessageW(Some(self.hwnd), WM_APP + 72, WPARAM(0), LPARAM(0));
                }
                SetTimer(Some(self.hwnd), PULSE, if suppressed { 500 } else { 200 }, None);
                if self.allowed_visible {
                    let _ = ShowWindow(self.hwnd, if suppressed { SW_HIDE } else { SW_SHOWNOACTIVATE });
                }
            }
            if let Some((backdrop, _)) = &self.backdrop {
                let _ = backdrop.SetIsVisible(!suppressed && self.config.blur);
            }
            if !suppressed {
                self.schedule_backdrop_refresh();
                self.resumed = true;
                self.scanned_at = Instant::now() - Duration::from_secs(3);
                self.leave_at = Instant::now();
            }
        }
    }

    fn layout(&mut self) {
        let monitors = desktop::monitors();
        if let Some(monitor) = monitors.iter().find(|m| m.device == self.config.monitor).or_else(|| monitors.first()) {
            self.monitor = monitor.clone();
        }
        let area = self.monitor.work;
        let icon = self.config.icon_size as f32;
        let count = (self.entries.len() + usize::from(self.config.show_add)).max(1).min(capacity(
            (area.right - area.left) as f32,
            self.scale,
            icon,
            self.config.zoom,
        ));
        let width =
            ((count as f32 * (icon + 12.0) + 40.0 + icon * (self.config.zoom - 1.0) * 3.0) * self.scale).ceil() as i32;
        let height = ((icon * self.config.zoom + 64.0 + self.config.margin as f32) * self.scale).ceil() as i32;
        self.size = (width.min(area.right - area.left).max(1), height);
        self.origin = (area.left + (area.right - area.left - self.size.0) / 2, area.bottom - height);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                self.origin.0,
                self.origin.1,
                self.size.0,
                self.size.1,
                SWP_NOACTIVATE,
            );
        }
        let _ = self.content.resize(self.size);
    }

    fn hit(&self, point: (f32, f32)) -> Option<usize> {
        self.rects
            .iter()
            .find(|(_, rect)| {
                inside(
                    (
                        rect.0 - 4.0 * self.scale,
                        rect.1 - 3.0 * self.scale,
                        rect.2 + 4.0 * self.scale,
                        rect.3 + 6.0 * self.scale,
                    ),
                    point,
                )
            })
            .map(|(index, _)| *index)
    }

    fn request_frame(&mut self) {
        if !self.frame_pending && !self.suppressed {
            self.frame_pending = true;
            unsafe {
                SetTimer(Some(self.hwnd), FRAME, if self.config.economy { 33 } else { 10 }, None);
            }
        }
    }

    fn render(&mut self) {
        if self.suppressed {
            self.frame_pending = false;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), FRAME);
            }
            return;
        }
        let s = self.scale;
        let icon = self.config.icon_size as f32 * s;
        let width = self.size.0 as f32;
        let height = self.size.1 as f32;
        let slots = capacity(
            (self.monitor.work.right - self.monitor.work.left) as f32,
            s,
            self.config.icon_size as f32,
            self.config.zoom,
        )
        .saturating_sub(usize::from(self.config.show_add));
        let count = self.entries.len().saturating_sub(self.offset).min(slots);
        let pitch = icon + 10.0 * s;
        let shown = count + usize::from(self.config.show_add || self.entries.is_empty());
        let boundary = (0..count).find(|n| {
            let i = self.offset + n;
            i > 0 && self.entries[i - 1].pinned && !self.entries[i].pinned
        });
        let gap = if self.config.separator && boundary.is_some() { 20.0 * s } else { 0.0 };
        let base_left = (width - shown as f32 * pitch - gap) / 2.0;
        let hide = self.feedback.follow(10_000, if self.hidden { 1.0 } else { 0.0 }, 0.0, 280).clamp(0.0, 1.0);
        let bottom = height - (self.config.margin as f32 + 12.0) * s;
        let shift = hide * (icon * self.config.zoom + (self.config.margin as f32 + 24.0) * s);
        let mut sizes = Vec::new();
        for n in 0..shown {
            let i = if n == count { self.entries.len() } else { self.offset + n };
            let strength = self.pointer.filter(|_| self.config.zoom > 1.0).map_or(0.0, |p| {
                let center =
                    base_left + (n as f32 + 0.5) * pitch + if boundary.is_some_and(|b| n >= b) { gap } else { 0.0 };
                let distance = ((p.0 - center) / (pitch * 1.8)).abs().min(1.0);
                let horizontal = 0.5 + 0.5 * (std::f32::consts::PI * distance).cos();
                let vertical =
                    crate::ui::reveal::smooth_range(p.1, bottom - icon * self.config.zoom - 28.0 * s, bottom - icon);
                horizontal * vertical
            });
            let id = self.entries.get(i).map_or(i as u64, |entry| motion_key(&entry.pin.target, 1));
            let zoom = self.feedback.follow(id, strength, 0.0, 180).clamp(0.0, 1.0);
            sizes.push(icon * (1.0 + (self.config.zoom - 1.0) * zoom));
        }
        let total = sizes.iter().sum::<f32>() + (shown as f32 + 1.0) * 10.0 * s + gap;
        let left = (width - total) / 2.0;
        self.panel =
            (left - 4.0 * s, bottom - icon - 10.0 * s + shift, left + total + 4.0 * s, bottom + 10.0 * s + shift);
        self.rects.clear();
        self.drop_slots.clear();
        let mut x = left + 10.0 * s;
        for (n, side) in sizes.iter().enumerate() {
            let i = if n == count { self.entries.len() } else { self.offset + n };
            if boundary == Some(n) {
                x += gap;
            }
            self.drop_slots.push((i, x + side * 0.5));
            let animated_x = self.entries.get(i).map_or(x, |entry| {
                let rank = self.feedback.follow(motion_key(&entry.pin.target, 2), n as f32, n as f32, 220);
                x + (rank - n as f32) * pitch
            });
            let rect = if self.dragging && self.pressed.is_some_and(|(index, _)| index == i) {
                let point = self.pointer.unwrap_or((animated_x + side * 0.5, bottom - side * 0.5));
                let drag_x = (point.0 - side * 0.5).clamp(2.0 * s, width - side - 2.0 * s);
                (drag_x, bottom - side - 10.0 * s + shift, drag_x + side, bottom - 10.0 * s + shift)
            } else {
                (animated_x, bottom - side + shift, animated_x + side, bottom + shift)
            };
            self.rects.push((i, rect));
            x += side + 10.0 * s;
        }
        let hint = self.hover.map(|i| {
            self.entries.get(i).map_or_else(
                || crate::ui::i18n::tr("Drop an app here or right-click to add").into_owned(),
                |entry| entry.pin.name.clone(),
            )
        });
        if let Some(text) = hint.as_ref() {
            self.hint_label = self.hover.map(|index| (index, text.clone()));
        }
        let show_hint =
            hint.is_some() && !self.dragging && hide < 0.01 && !self.preview.as_ref().is_some_and(|p| p.is_open());
        self.hint_alpha = self.feedback.follow(10_001, f32::from(show_hint), 0.0, 160).clamp(0.0, 1.0);
        let label = self.label_rect();
        if let Some((backdrop, geometry)) = &self.backdrop {
            let _ = backdrop.SetIsVisible(self.config.blur && hide < 0.99);
            let _ = backdrop.SetOffset(Vector3 { X: self.panel.0, Y: self.panel.1, Z: 0.0 });
            let size = Vector2 { X: self.panel.2 - self.panel.0, Y: self.panel.3 - self.panel.1 };
            let _ = backdrop.SetSize(size);
            let _ = geometry.SetSize(size);
            let _ = geometry.SetCornerRadius(Vector2 { X: 22.0 * s, Y: 22.0 * s });
        }
        let _ = self.content.paint(&self.gpu, |p| self.paint_content(p, hide));
        let padding = 1.0 + s * 0.5;
        let mut shapes = vec![if hide > 0.99 {
            region_shape((0.0, height - 4.0 * s, width, height), 0.0, 0.0)
        } else {
            region_shape(self.panel, 22.0 * s, padding)
        }];
        if hide <= 0.99 {
            for (_, rect) in &self.rects {
                shapes.push(region_shape(*rect, 6.0 * s, 3.0 * s));
            }
            if self.hint_alpha > 0.001 {
                shapes.push(region_shape(label, 6.0 * s, padding));
            }
        }
        if shapes != self.region_shapes {
            unsafe {
                let [left, top, right, bottom, rx, ry] = shapes[0];
                let region = CreateRoundRectRgn(left, top, right, bottom, rx, ry);
                for &[left, top, right, bottom, rx, ry] in &shapes[1..] {
                    let part = CreateRoundRectRgn(left, top, right, bottom, rx, ry);
                    CombineRgn(Some(region), Some(part), Some(region), RGN_OR);
                    let _ = DeleteObject(part.into());
                }
                if SetWindowRgn(self.hwnd, Some(region), true) == 0 {
                    let _ = DeleteObject(region.into());
                } else {
                    self.region_shapes = shapes;
                }
            }
        }
        if self.feedback.active() {
            self.request_frame();
        } else {
            self.frame_pending = false;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), FRAME);
            }
        }
    }

    fn label_rect(&self) -> Rect {
        let s = self.scale;
        let width = self.size.0 as f32;
        let len = self.hint_label.as_ref().map_or(0, |(_, label)| label.chars().count());
        let index = self.hint_label.as_ref().map(|(index, _)| *index);
        let label_width = ((len as f32 * 7.0 + 24.0).clamp(64.0, 280.0) * s).min(width - 8.0 * s);
        let center = self.rects.iter().find(|(i, _)| Some(*i) == index).map_or(width / 2.0, |(_, r)| (r.0 + r.2) / 2.0);
        let left = (center - label_width / 2.0).clamp(4.0 * s, (width - label_width - 4.0 * s).max(4.0 * s));
        (left, 8.0 * s, left + label_width, 34.0 * s)
    }

    fn paint_content(&self, p: &crate::ui::gfx::Painter, hide: f32) {
        let s = self.scale;
        let icon = self.config.icon_size as f32 * s;
        let width = self.size.0 as f32;
        let height = self.size.1 as f32;
        let bottom = height - (self.config.margin as f32 + 12.0) * s;
        let shift = hide * (icon * self.config.zoom + (self.config.margin as f32 + 24.0) * s);
        let slots = capacity(
            (self.monitor.work.right - self.monitor.work.left) as f32,
            s,
            self.config.icon_size as f32,
            self.config.zoom,
        )
        .saturating_sub(usize::from(self.config.show_add));
        let label = self.label_rect();

        if hide > 0.99 {
            let _ = p.fill_round_rect(
                (width / 2.0 - 24.0 * s, height - 3.0 * s, width / 2.0 + 24.0 * s, height),
                1.5 * s,
                rgba(0.7, 0.7, 0.75, 0.45),
            );
            return;
        }
        let _ = paint_material(p, self.panel, 22.0 * s, s, &self.config);
        let mut draw_order = self.rects.clone();
        if self.dragging {
            draw_order.sort_by_key(|(i, _)| self.pressed.is_some_and(|(pressed, _)| pressed == *i));
        }
        for &(i, rect) in &draw_order {
            let active = self.entries.get(i).is_some_and(|e| e.windows.iter().any(|w| w.active));
            let target = self.entries.get(i).map_or("__add", |entry| entry.pin.target.as_str());
            let glow = self.feedback.sample(
                motion_key(target, 6),
                if active {
                    1.0
                } else if self.hover == Some(i) {
                    0.5
                } else {
                    0.0
                },
                0.0,
                140,
            );
            if glow > 0.001 {
                let _ = p.fill_round_rect(
                    (rect.0 - 3.0 * s, rect.1 - 3.0 * s, rect.2 + 3.0 * s, rect.3 + 3.0 * s),
                    13.0 * s,
                    rgba(1.0, 1.0, 1.0, 0.12 * glow),
                );
            }
            if self.dragging && self.pressed.is_some_and(|(index, _)| index == i) {
                let _ = p.line(
                    (rect.0 - 5.0 * s, bottom - icon + shift),
                    (rect.0 - 5.0 * s, bottom + shift),
                    2.0 * s,
                    rgba(0.65, 0.8, 1.0, 0.9),
                );
            }
            let shrink = self.feedback.sample(
                motion_key(target, 4),
                if self.pressed.is_some_and(|(index, _)| index == i) { 2.0 * s } else { 0.0 },
                0.0,
                90,
            );
            let image = (rect.0 + shrink, rect.1 + shrink, rect.2 - shrink, rect.3 - shrink);
            if let Some(entry) = self.entries.get(i) {
                if let Some(icon) = &entry.icon {
                    let reveal = self.feedback.sample(motion_key(target, 3), 1.0, 0.0, 180);
                    let _ = p.cached_icon(image, icon, &mut self.icon_bitmaps.borrow_mut(), reveal);
                } else {
                    let _ = p.glyph('\u{E8A5}', image, icon * 0.65, rgba(0.85, 0.85, 0.9, 1.0));
                }
                if entry.windows.iter().any(crate::platform::app_capture::marked) {
                    let badge = (rect.2 - 15.0 * s, rect.1 - 2.0 * s, rect.2 + 3.0 * s, rect.1 + 16.0 * s);
                    let _ = p.fill_round_rect(badge, 6.0 * s, rgba(0.08, 0.1, 0.16, 0.98));
                    let _ = p.glyph('\u{E72E}', badge, 11.0 * s, rgba(0.7, 0.8, 1.0, 1.0));
                }
                let running = self.feedback.sample(
                    motion_key(target, 5),
                    if entry.windows.is_empty() { 0.0 } else { 1.0 },
                    0.0,
                    160,
                );
                if running > 0.001 {
                    let cx = (rect.0 + rect.2) / 2.0;
                    let _ = p.fill_ellipse(
                        cx,
                        bottom + shift + 6.0 * s,
                        if active { 3.0 } else { 2.0 } * s,
                        2.0 * s,
                        rgba(1.0, 1.0, 1.0, (if active { 0.95 } else { 0.5 }) * running),
                    );
                }
                if self.config.separator && i > 0 && !entry.pinned && self.entries[i - 1].pinned {
                    let _ = p.line(
                        (rect.0 - 15.0 * s, bottom - icon * 0.85 + shift),
                        (rect.0 - 15.0 * s, bottom - icon * 0.15 + shift),
                        s,
                        rgba(1.0, 1.0, 1.0, 0.2),
                    );
                }
            } else {
                let _ = p.glyph(
                    if self.entries.is_empty() { '\u{E8B7}' } else { '\u{E710}' },
                    image,
                    icon * 0.42,
                    rgba(0.75, 0.75, 0.8, 0.9),
                );
            }
        }
        if let Some((_, text)) = &self.hint_label
            && self.hint_alpha > 0.001
            && hide < 0.99
        {
            let alpha = self.hint_alpha * (1.0 - hide);
            let _ = p.fill_round_rect(label, 9.0 * s, rgba(0.06, 0.06, 0.07, 0.97 * alpha));
            let _ = p.text(
                text,
                label,
                12.0 * s,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                rgba(0.95, 0.95, 0.97, alpha),
            );
        }
        if self.entries.len() > slots && hide < 0.01 {
            let _ = p.fill_ellipse(
                self.panel.2 - 5.0 * s,
                (self.panel.1 + self.panel.3) / 2.0,
                1.5 * s,
                5.0 * s,
                rgba(1.0, 1.0, 1.0, 0.4),
            );
        }
    }

    fn hide_preview(&mut self) {
        self.actions.clear();
        unsafe {
            let _ = KillTimer(Some(self.hwnd), PREVIEW_HOVER);
        }
        if let Some(preview) = &mut self.preview {
            preview.hide();
        }
        self.preview_candidate = None;
    }

    fn show_preview(&mut self, entry: &Entry) {
        let Some((_, rect)) =
            self.rects.iter().find(|(i, _)| self.entries.get(*i).is_some_and(|e| e.pin.target == entry.pin.target))
        else {
            return;
        };
        let anchor = (
            rect.0 + self.origin.0 as f32,
            rect.1 + self.origin.1 as f32,
            rect.2 + self.origin.0 as f32,
            rect.3 + self.origin.1 as f32,
        );
        if let Some(preview) = &mut self.preview {
            preview.set_capture_hidden(self.capture_hidden.get());
            preview.set_material(&self.config);
            preview.show(entry, anchor, self.monitor.work, self.scale);
        } else {
            self.preview = crate::ui::dock_preview::Preview::new(
                self.parent,
                &self.gpu,
                entry,
                anchor,
                self.monitor.work,
                self.scale,
                crate::ui::dock_preview::Options {
                    capture_hidden: self.capture_hidden.get(),
                    settings: self.config.clone(),
                    actions: self.actions.clone(),
                },
            )
            .ok();
        }
        if let Some(preview) = &self.preview {
            preview.set_capture_hidden(self.capture_hidden.get());
        }
        self.render();
    }

    fn update_preview(&mut self, point: POINT) {
        if self.suppressed || self.hidden || self.pressed.is_some() || self.menu_open || !self.allowed_visible {
            self.hide_preview();
            return;
        }
        let entry = self.hover.and_then(|i| self.entries.get(i)).filter(|e| !e.windows.is_empty()).cloned();
        if let Some(preview) = &mut self.preview {
            let was_open = preview.is_open();
            preview.tick(point, entry.as_ref().is_some_and(|e| e.pin.target == preview.target()));
            if was_open != preview.is_open() {
                self.render();
            }
        }
        let Some(entry) = entry else {
            if !self.preview.as_ref().is_some_and(|p| p.contains(point)) {
                self.preview_candidate = None;
                if !self.preview.as_ref().is_some_and(|p| p.is_open()) {
                    self.actions.clear();
                }
            }
            return;
        };
        if !self.config.hover_preview {
            return;
        }
        if self.preview_candidate.as_ref().is_none_or(|(target, _)| *target != entry.pin.target) {
            self.preview_candidate = Some((entry.pin.target.clone(), Instant::now()));
            let switching = self.preview.as_ref().is_some_and(|p| p.is_open());
            if let Some(preview) = &mut self.preview {
                preview.allow_reopen();
            }
            if !switching && let Some(window) = entry.windows.first() {
                self.actions.request(&window.path, &entry.windows.iter().map(|w| w.handle).collect::<Vec<_>>());
            }
            unsafe {
                SetTimer(Some(self.hwnd), PREVIEW_HOVER, if switching { 80 } else { 200 }, None);
            }
        }
        let delay = if self.preview.as_ref().is_some_and(|p| p.is_open()) { 70 } else { 180 };
        if self.preview_candidate.as_ref().is_some_and(|(_, at)| at.elapsed() >= Duration::from_millis(delay))
            && !self.preview.as_ref().is_some_and(|p| p.target() == entry.pin.target && (p.is_open() || p.dismissed()))
        {
            self.show_preview(&entry);
        }
    }

    fn pulse(&mut self) {
        if self.update_suppression() {
            return;
        }
        self.feedback.set_enabled(self.config.animations && motion_allowed());
        if self.scanned_at.elapsed() >= Duration::from_millis(200) {
            self.refresh(false);
        }
        if self.icons.poll() {
            self.load_icons();
            self.render();
        }
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        self.update_preview(pt);
        let preview_open = self.preview.as_ref().is_some_and(|p| p.is_open());
        let point = ((pt.x - self.origin.0) as f32, (pt.y - self.origin.1) as f32);
        let near = inside(
            (0.0, self.panel.1.min(self.size.1 as f32 - 12.0 * self.scale), self.size.0 as f32, self.size.1 as f32),
            point,
        );
        if near || preview_open || self.menu_open || self.pressed.is_some() {
            self.leave_at = Instant::now();
        }
        let hidden = self.config.auto_hide
            && !near
            && !preview_open
            && !self.menu_open
            && self.pressed.is_none()
            && self.leave_at.elapsed() > Duration::from_millis(650);
        if hidden != self.hidden {
            self.hidden = hidden;
            self.render();
        }
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        if (*ptr).suppressed
            && matches!(msg, WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONUP | WM_MOUSEWHEEL)
        {
            return LRESULT(0);
        }
        match msg {
            m if m == WM_APP + 72 => {
                if GetCapture() == hwnd && (*ptr).pressed.is_none() {
                    let _ = ReleaseCapture();
                }
                LRESULT(0)
            }
            m if m == crate::platform::dock_actions::WM_ACTIONS_READY => {
                if let Some(preview) = &mut (*ptr).preview {
                    preview.refresh_actions();
                }
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_TIMER => {
                let state = &mut *ptr;
                if wparam.0 == FRAME {
                    let _ = KillTimer(Some(hwnd), FRAME);
                    state.frame_pending = false;
                    state.render();
                } else if wparam.0 == PULSE {
                    state.pulse();
                } else if wparam.0 == BACKDROP_REFRESH {
                    state.refresh_backdrop();
                    if let Some(preview) = &state.preview {
                        preview.refresh_backdrop();
                    }
                } else if wparam.0 == PREVIEW_HOVER {
                    let _ = KillTimer(Some(hwnd), PREVIEW_HOVER);
                    let mut point = POINT::default();
                    let _ = GetCursorPos(&mut point);
                    state.update_preview(point);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let state = &mut *ptr;
                let point = (lparam.0 as i16 as f32, (lparam.0 >> 16) as i16 as f32);
                state.pointer = Some(point);
                state.hover = state.hit(point);
                state.leave_at = Instant::now();
                state.hidden = false;
                if let Some((from, start)) = state.pressed
                    && from < state.config.pins.len()
                    && !state.config.lock_pins
                {
                    state.dragging |= (point.0 - start).abs() > 7.0 * state.scale;
                    if state.dragging {
                        if state.drag_snapshot.is_none() {
                            state.drag_snapshot = Some(state.config.pins.clone());
                        }
                        let to = state
                            .drop_slots
                            .iter()
                            .filter(|(i, _)| *i < state.config.pins.len())
                            .min_by(|a, b| (a.1 - point.0).abs().total_cmp(&(b.1 - point.0).abs()))
                            .map(|(i, _)| *i)
                            .unwrap_or(from);
                        if from != to {
                            let pin = state.config.pins.remove(from);
                            state.config.pins.insert(to, pin);
                            let entry = state.entries.remove(from);
                            state.entries.insert(to, entry);
                            state.pressed = Some((to, start));
                            state.hover = Some(to);
                        }
                    }
                }
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                });
                if state.dragging {
                    state.hide_preview();
                } else {
                    state.update_preview(POINT {
                        x: state.origin.0 + point.0 as i32,
                        y: state.origin.1 + point.1 as i32,
                    });
                }
                state.request_frame();
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                let state = &mut *ptr;
                state.pointer = None;
                state.hover = None;
                state.leave_at = Instant::now();
                state.request_frame();
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let state = &mut *ptr;
                let point = (lparam.0 as i16 as f32, (lparam.0 >> 16) as i16 as f32);
                state.hide_preview();
                state.pressed = state.hit(point).map(|i| (i, point.0));
                state.dragging = false;
                state.render();
                SetCapture(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let state = &mut *ptr;
                let point = (lparam.0 as i16 as f32, (lparam.0 >> 16) as i16 as f32);
                let pressed = state.pressed.take();
                let hit = state.hit(point);
                if let Some((from, _)) = pressed {
                    if state.dragging {
                        let changed = state.drag_snapshot.take().is_some_and(|before| before != state.config.pins);
                        if changed {
                            state.post(Request::Save);
                        }
                        state.refresh(true);
                    } else if hit == Some(from) {
                        if let Some(entry) = state.entries.get(from).cloned() {
                            state.post(Request::Launch(entry));
                        } else {
                            state.post(Request::Add);
                        }
                    }
                }
                state.dragging = false;
                state.render();
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                let state = &mut *ptr;
                state.pressed = None;
                state.dragging = false;
                if let Some(before) = state.drag_snapshot.take() {
                    state.config.pins = before;
                    state.refresh(true);
                }
                state.render();
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                let state = &mut *ptr;
                let point = (lparam.0 as i16 as f32, (lparam.0 >> 16) as i16 as f32);
                let entry = state.hit(point).and_then(|i| state.entries.get(i)).cloned();
                state.hide_preview();
                if GetKeyState(VK_MENU.0 as i32) < 0 {
                    if let Some(entry) = entry {
                        state.post(Request::Capture(entry));
                    }
                } else {
                    state.menu_open = true;
                    state.post(Request::Menu(entry));
                }
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let state = &mut *ptr;
                state.hide_preview();
                let down = ((wparam.0 >> 16) as i16) < 0;
                let slots = capacity(
                    (state.monitor.work.right - state.monitor.work.left) as f32,
                    state.scale,
                    state.config.icon_size as f32,
                    state.config.zoom,
                )
                .saturating_sub(usize::from(state.config.show_add));
                state.offset = if down {
                    (state.offset + 1).min(state.entries.len().saturating_sub(slots))
                } else {
                    state.offset.saturating_sub(1)
                };
                state.hover = None;
                state.pointer = None;
                state.load_icons();
                state.render();
                LRESULT(0)
            }
            WM_DROPFILES => {
                let state = &mut *ptr;
                let drop = HDROP(wparam.0 as *mut _);
                let mut changed = false;
                for i in 0..DragQueryFileW(drop, u32::MAX, None).min(32) {
                    let n = DragQueryFileW(drop, i, None);
                    if n == 0 || n > 2048 {
                        continue;
                    }
                    let mut text = vec![0u16; n as usize + 1];
                    let len = DragQueryFileW(drop, i, Some(&mut text)) as usize;
                    changed |= state.config.add(Pin::from_path(&String::from_utf16_lossy(&text[..len])));
                }
                DragFinish(drop);
                if changed {
                    state.post(Request::Save);
                    state.refresh(true);
                }
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
                let state = &mut *ptr;
                state.layout();
                state.render();
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let _ = PostMessageW(Some(hwnd), WM_APP + 71, WPARAM(0), LPARAM(0));
                LRESULT(0)
            }
            m if m == WM_APP + 71 => {
                let state = &mut *ptr;
                let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
                if (scale - state.scale).abs() > 0.01 {
                    state.scale = scale;
                    state.layout();
                    state.load_icons();
                    state.render();
                }
                LRESULT(0)
            }
            WM_CLOSE => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

impl Dock {
    pub fn new(parent: HWND, compositor: &Compositor, gpu: &Gpu, config: &DockSettings) -> Result<Self> {
        Self::create(parent, compositor, gpu, config, true)
    }

    fn create(parent: HWND, compositor: &Compositor, gpu: &Gpu, config: &DockSettings, visible: bool) -> Result<Self> {
        static CLASS: Once = Once::new();
        let instance = unsafe { GetModuleHandleW(None)? };
        CLASS.call_once(|| unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("NightIsland.DesktopDock"),
                ..Default::default()
            });
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("NightIsland.DesktopDock"),
                w!("Night Island Dock"),
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
        struct Guard(HWND);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                }
            }
        }
        let guard = Guard(hwnd);
        let target = unsafe { compositor.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(hwnd, true)? };
        let root = compositor.CreateContainerVisual()?;
        root.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        let backdrop = create_backdrop(hwnd, compositor, &root);
        let content = gpu.layer(compositor, (1, 1))?;
        root.Children()?.InsertAtTop(&content.sprite)?;
        target.SetRoot(&root)?;
        let monitor = desktop::monitors().into_iter().next().ok_or_else(windows::core::Error::empty)?;
        let mut state = Box::new(State {
            hwnd,
            parent,
            config: config.clone(),
            gpu: gpu.popup(),
            content,
            backdrop,
            icon_bitmaps: RefCell::default(),
            icons: apps::DockIconCache::new(),
            _target: target,
            _root: root,
            entries: Vec::new(),
            scan: Vec::new(),
            targets: HashMap::new(),
            monitor,
            scale: unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0,
            size: (1, 1),
            origin: (0, 0),
            rects: Vec::new(),
            panel: (0.0, 0.0, 1.0, 1.0),
            region_shapes: Vec::new(),
            feedback: Feedback::with_capacity(512),
            frame_pending: false,
            hint_label: None,
            hint_alpha: 0.0,
            pointer: None,
            hover: None,
            pressed: None,
            dragging: false,
            drag_snapshot: None,
            drop_slots: Vec::new(),
            leave_at: Instant::now(),
            scanned_at: Instant::now(),
            hidden: false,
            suppressed: false,
            offset: 0,
            request: None,
            menu_open: false,
            allowed_visible: visible,
            resumed: false,
            preview: None,
            actions: Arc::new(crate::platform::dock_actions::Client::new()),
            preview_candidate: None,
            capture_hidden: std::cell::Cell::new(false),
        });
        state.actions.set_notify(hwnd);
        state.feedback.set_enabled(state.config.animations && motion_allowed());
        state.update_suppression();
        if state.suppressed {
            state.layout();
        } else {
            state.refresh(true);
        }
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&raw mut *state) as isize);
            DragAcceptFiles(hwnd, true);
            SetTimer(Some(hwnd), PULSE, if state.suppressed { 500 } else { 200 }, None);
            if visible && !state.suppressed {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
        std::mem::forget(guard);
        Ok(Self { state })
    }

    pub fn update(&mut self, config: &DockSettings) {
        let rebuild = self.state.config.pins != config.pins || self.state.config.show_running != config.show_running;
        self.state.hide_preview();
        self.state.config = config.clone();
        self.state.feedback.set_enabled(config.animations && motion_allowed());
        if self.state.update_suppression() {
            return;
        }
        if rebuild {
            self.state.refresh(true);
        } else {
            self.state.layout();
            self.state.load_icons();
            self.state.render();
        }
        self.state.pulse();
    }

    pub fn refresh_windows(&mut self) {
        if !self.state.update_suppression() {
            self.state.refresh(false);
            self.state.schedule_backdrop_refresh();
        }
    }

    pub fn repaint_icons(&mut self) {
        if !self.state.suppressed {
            self.state.render();
            if let Some(preview) = &self.state.preview {
                preview.repaint_icons();
            }
        }
    }

    pub fn show_preview(&mut self, entry: &Entry) {
        if !self.state.suppressed {
            self.state.show_preview(entry);
        }
    }

    pub fn set_capture_hidden(&self, hidden: bool) {
        self.state.capture_hidden.set(hidden);
        if let Some(preview) = &self.state.preview {
            preview.set_capture_hidden(hidden);
        }
        unsafe {
            let _ = SetWindowDisplayAffinity(self.state.hwnd, if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
        }
    }

    pub fn scale(&self) -> f32 {
        self.state.scale
    }

    pub fn config(&self) -> DockSettings {
        self.state.config.clone()
    }
    pub fn take_request(&mut self) -> Option<Request> {
        self.state.request.take()
    }
    pub fn close_menu(&mut self) {
        self.state.menu_open = false;
        self.state.leave_at = Instant::now();
        self.state.pointer = None;
        self.state.hover = None;
        self.state.render();
    }
}

impl Drop for Dock {
    fn drop(&mut self) {
        self.state.actions.set_notify(HWND::default());
        self.state.actions.clear();
        self.state.preview.take();
        unsafe {
            let _ = KillTimer(Some(self.state.hwnd), PULSE);
            let _ = KillTimer(Some(self.state.hwnd), FRAME);
            let _ = KillTimer(Some(self.state.hwnd), BACKDROP_REFRESH);
            let _ = KillTimer(Some(self.state.hwnd), PREVIEW_HOVER);
            SetWindowLongPtrW(self.state.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.state.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify_live_backdrop(dock: &mut Dock, folder: &std::path::Path) -> Result<()> {
        use windows::Win32::Foundation::{COLORREF, RECT};
        use windows::Win32::Graphics::Dwm::DwmFlush;
        use windows::Win32::Graphics::Gdi::*;
        use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
        struct Fixture(HWND, windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);
        impl Drop for Fixture {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                    SetThreadDpiAwarenessContext(self.1);
                }
            }
        }
        assert!(dock.state.backdrop.is_some(), "Windows host backdrop is unavailable");
        std::fs::create_dir_all(folder).unwrap();
        let old_config = dock.state.config.clone();
        let old_scale = dock.state.scale;
        let old_dpi = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        dock.state.scale = unsafe { GetDpiForWindow(dock.state.hwnd) } as f32 / 96.0;
        dock.state.feedback.set_enabled(false);
        dock.state.pointer = None;
        dock.state.hover = None;
        dock.state.layout();
        let (width, height) = dock.state.size;
        let origin = dock.state.origin;
        let backdrop = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("Night Island blur fixture"),
                WS_POPUP,
                origin.0,
                origin.1,
                width,
                height,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?
        };
        let _fixture = Fixture(backdrop, old_dpi);
        unsafe {
            let _ = ShowWindow(backdrop, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(backdrop, Some(HWND_TOPMOST), origin.0, origin.1, width, height, SWP_NOACTIVATE);
            let dc = GetDC(Some(backdrop));
            for x in (0..width).step_by(6) {
                let brush = CreateSolidBrush(COLORREF(if (x / 6) % 2 == 0 { 0x00ffffff } else { 0x00000000 }));
                FillRect(dc, &RECT { left: x, top: 0, right: (x + 6).min(width), bottom: height }, brush);
                let _ = DeleteObject(brush.into());
            }
            ReleaseDC(Some(backdrop), dc);
            let _ = ValidateRect(Some(backdrop), None);
            let _ =
                SetWindowPos(dock.state.hwnd, Some(HWND_TOPMOST), origin.0, origin.1, width, height, SWP_NOACTIVATE);
            let _ = ShowWindow(dock.state.hwnd, SW_SHOWNOACTIVATE);
            let _ = KillTimer(Some(dock.state.hwnd), PULSE);
        }
        let mut energies = Vec::new();
        for blur in [false, true] {
            dock.state.config.blur = blur;
            dock.state.config.opacity = 0.0;
            dock.state.config.blur_opacity = 0.0;
            dock.state.render();
            let until = Instant::now() + Duration::from_millis(250);
            while Instant::now() < until {
                unsafe {
                    let mut msg = MSG::default();
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    let _ = DwmFlush();
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            unsafe {
                let screen = GetDC(None);
                let mem = CreateCompatibleDC(Some(screen));
                let mut bitmap = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: width,
                        biHeight: -height,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut bits = std::ptr::null_mut();
                let image = CreateDIBSection(Some(mem), &bitmap, DIB_RGB_COLORS, &mut bits, None, 0)?;
                let previous = SelectObject(mem, image.into());
                BitBlt(mem, 0, 0, width, height, Some(screen), origin.0, origin.1, SRCCOPY | CAPTUREBLT)?;
                SelectObject(mem, previous);
                GetDIBits(mem, image, 0, height as u32, Some(pixels.as_mut_ptr().cast()), &mut bitmap, DIB_RGB_COLORS);
                let _ = DeleteObject(image.into());
                let _ = DeleteDC(mem);
                ReleaseDC(None, screen);
            }
            let y = (dock.state.panel.3 - 8.0 * dock.state.scale) as usize;
            let x0 = (dock.state.panel.0 + 26.0 * dock.state.scale) as usize;
            let x1 = (dock.state.panel.2 - 26.0 * dock.state.scale) as usize;
            let lum = |x: usize| {
                let i = (y * width as usize + x) * 4;
                (pixels[i] as f32 + pixels[i + 1] as f32 + pixels[i + 2] as f32) / 3.0
            };
            let energy: f32 = (x0 + 1..x1).map(|x| (lum(x) - lum(x - 1)).abs()).sum::<f32>() / (x1 - x0 - 1) as f32;
            energies.push(energy);
            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
                px[3] = 255;
            }
            let mut encoder = png::Encoder::new(
                std::fs::File::create(folder.join(if blur { "blur-on.png" } else { "blur-off.png" })).unwrap(),
                width as u32,
                height as u32,
            );
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        let foreground = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("Night Island moving backdrop fixture"),
                WS_POPUP | WS_MINIMIZEBOX,
                origin.0,
                origin.1,
                width,
                height,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?
        };
        struct Foreground(HWND);
        impl Drop for Foreground {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                }
            }
        }
        let _foreground = Foreground(foreground);
        let settle = || {
            let until = Instant::now() + Duration::from_millis(500);
            while Instant::now() < until {
                unsafe {
                    let mut msg = MSG::default();
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    let _ = DwmFlush();
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        };
        let panel_sample =
            ((dock.state.panel.0 + dock.state.panel.2) / 2.0, dock.state.panel.3 - 8.0 * dock.state.scale);
        let sample = || unsafe {
            let screen = GetDC(None);
            let color = GetPixel(screen, origin.0 + panel_sample.0 as i32, origin.1 + panel_sample.1 as i32).0;
            ReleaseDC(None, screen);
            [color & 255, (color >> 8) & 255, (color >> 16) & 255]
        };
        let mut colors = Vec::new();
        unsafe {
            let _ = ShowWindow(foreground, SW_SHOWNOACTIVATE);
            let _ =
                SetWindowPos(dock.state.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE);
            let _ = KillTimer(Some(dock.state.hwnd), FRAME);
        }
        for color in [0x000000ff, 0x00ff0000] {
            unsafe {
                let dc = GetDC(Some(foreground));
                let brush = CreateSolidBrush(COLORREF(color));
                FillRect(dc, &RECT { left: 0, top: 0, right: width, bottom: height }, brush);
                let _ = DeleteObject(brush.into());
                ReleaseDC(Some(foreground), dc);
                let _ = ValidateRect(Some(foreground), None);
            }
            settle();
            colors.push(sample());
        }
        let previous_brush = dock.state.backdrop.as_ref().unwrap().0.Brush()?;
        unsafe {
            let _ = ShowWindow(foreground, SW_MINIMIZE);
        }
        dock.refresh_windows();
        dock.refresh_windows();
        settle();
        colors.push(sample());
        let refreshed_brush = dock.state.backdrop.as_ref().unwrap().0.Brush()?;
        assert_ne!(
            previous_brush.as_raw(),
            refreshed_brush.as_raw(),
            "Scene change must replace cached backdrop sample"
        );
        settle();
        assert_eq!(
            refreshed_brush.as_raw(),
            dock.state.backdrop.as_ref().unwrap().0.Brush()?.as_raw(),
            "Backdrop refresh must stop after one update"
        );
        unsafe {
            let _ = ShowWindow(dock.state.hwnd, SW_HIDE);
            SetTimer(Some(dock.state.hwnd), PULSE, 200, None);
        }
        dock.state.config = old_config;
        dock.state.scale = old_scale;
        dock.state.layout();
        dock.state.render();
        assert!(colors[0][0] > colors[0][2] + 80, "Red scene must reach live backdrop: {colors:?}");
        assert!(colors[1][2] > colors[1][0] + 80, "Changed blue scene must refresh without dock drawing: {colors:?}");
        assert!(colors[2][0].abs_diff(colors[2][2]) < 30, "Minimized scene must reveal neutral background: {colors:?}");
        assert!(energies[0] > 15.0, "Backdrop fixture must be visible: {energies:?}");
        assert!(energies[1] < energies[0] * 0.6, "Host backdrop must reduce stripe detail: {energies:?}");
        Ok(())
    }

    #[test]
    #[ignore = "Requires an isolated Windows Composition runtime"]
    fn native_dock_regions_dpi_reorder_import_and_cleanup() -> Result<()> {
        use windows::Win32::Graphics::Gdi::{CreateRectRgn, GetWindowRgn, PtInRegion};
        use windows::Win32::System::WinRT::{
            CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
            RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
        };
        unsafe {
            RoInitialize(RO_INIT_SINGLETHREADED)?;
        }
        struct Runtime;
        impl Drop for Runtime {
            fn drop(&mut self) {
                unsafe {
                    RoUninitialize();
                }
            }
        }
        let _runtime = Runtime;
        let _queue = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_NONE,
            })?
        };
        let compositor = Compositor::new()?;
        let gpu = Gpu::new(&compositor)?;
        let parent = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Dock test owner"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?
        };
        let mut config =
            DockSettings { enabled: true, show_running: false, hide_fullscreen: false, ..Default::default() };
        if let Some(path) = std::env::var_os("NI_DOCK_IMPORT_FILE") {
            config.pins = crate::data::desktop_dock::import_mydock(&std::fs::read(path).unwrap()).unwrap();
            assert!(config.pins.len() >= 2);
        } else {
            config.add(Pin::from_path(r"C:\Windows\System32\notepad.exe"));
            config.add(Pin::from_path(r"C:\Windows\System32\calc.exe"));
        }
        let mut dock = Dock::create(parent, &compositor, &gpu, &config, false)?;
        let hwnd = dock.state.hwnd;
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            dock.state.scale = scale;
            dock.state.feedback.set_enabled(false);
            dock.state.layout();
            dock.state.load_icons();
            let deadline = Instant::now() + Duration::from_secs(5);
            while dock.state.icons.pending() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
                dock.state.icons.poll();
            }
            dock.state.load_icons();
            dock.state.render();
            let panel = dock.state.panel;
            let bitmaps = dock.state.icon_bitmaps.borrow().identities();
            assert!(!bitmaps.is_empty(), "Native icons should be uploaded after loading");
            for _ in 0..4 {
                dock.state.render();
                assert_eq!(
                    dock.state.icon_bitmaps.borrow().identities(),
                    bitmaps,
                    "Settled rendering must reuse native bitmap uploads"
                );
            }
            unsafe {
                let region = CreateRectRgn(0, 0, 0, 0);
                assert!(GetWindowRgn(hwnd, region).0 > 0);
                assert!(!PtInRegion(region, 0, 0).as_bool());
                assert!(
                    PtInRegion(region, ((panel.0 + panel.2) / 2.0) as i32, (panel.3 - 4.0 * scale) as i32).as_bool()
                );
                let surface = crate::ui::popup::PopupSurface::new(dock.state.size)?;
                surface.paint(&gpu, |p| {
                    dock.state.paint_content(p, 0.0);
                    Ok(())
                })?;
                for (index, pixel) in surface.pixels().chunks_exact(4).enumerate() {
                    if pixel[3] >= 8 {
                        let x = index as i32 % dock.state.size.0;
                        let y = index as i32 / dock.state.size.0;
                        assert!(
                            PtInRegion(region, x, y).as_bool(),
                            "Native region clips visible edge at {x},{y}, scale={scale}, alpha={}",
                            pixel[3]
                        );
                    }
                }
                let _ = DeleteObject(region.into());
            }
            let rect = dock.state.rects[0].1;
            let x = (rect.0 + rect.2) / 2.0;
            let y = (rect.1 + rect.3) / 2.0;
            dock.state.pointer = Some((x, y));
            dock.state.hover = Some(0);
            dock.state.render();
            assert!(dock.state.rects[0].1.2 - dock.state.rects[0].1.0 > config.icon_size as f32 * scale);
            if let Some(folder) = std::env::var_os("NI_DOCK_PREVIEW") {
                let folder = std::path::PathBuf::from(folder);
                std::fs::create_dir_all(&folder).unwrap();
                let surface = crate::ui::popup::PopupSurface::new(dock.state.size)?;
                surface.paint(&gpu, |p| {
                    p.fill_rect(
                        (0.0, 0.0, dock.state.size.0 as f32, dock.state.size.1 as f32),
                        rgba(0.025, 0.06, 0.12, 1.0),
                    )?;
                    dock.state.paint_content(p, 0.0);
                    Ok(())
                })?;
                let mut pixels = surface.pixels();
                for px in pixels.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }
                let mut encoder = png::Encoder::new(
                    std::fs::File::create(folder.join(format!("dock-{scale:.1}.png"))).unwrap(),
                    dock.state.size.0 as u32,
                    dock.state.size.1 as u32,
                );
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
            }
            dock.state.pointer = None;
            dock.state.hover = None;
            dock.state.hidden = true;
            dock.state.render();
            unsafe {
                let region = CreateRectRgn(0, 0, 0, 0);
                assert!(GetWindowRgn(hwnd, region).0 > 0);
                assert!(!PtInRegion(region, dock.state.size.0 / 2, dock.state.size.1 - 20).as_bool());
                let mut bounds = windows::Win32::Foundation::RECT::default();
                windows::Win32::Graphics::Gdi::GetRgnBox(region, &mut bounds);
                assert!(
                    PtInRegion(region, dock.state.size.0 / 2, dock.state.size.1 - 2).as_bool(),
                    "scale={scale} size={:?} region={bounds:?}",
                    dock.state.size
                );
                let _ = DeleteObject(region.into());
            }
            assert!(!dock.state.feedback.active());
            if let Some((backdrop, _)) = &dock.state.backdrop {
                assert!(!backdrop.IsVisible()?);
            }
            dock.state.hidden = false;
            dock.state.render();
        }
        if let Some(folder) = std::env::var_os("NI_DOCK_PREVIEW") {
            let mut ui = crate::view::UiState::new();
            ui.cfg.desktop_dock = config.clone();
            ui.sw.group = crate::view::GROUPS.iter().position(|g| g.name == "Desktop dock").unwrap();
            let size = crate::view::SW_SIZE;
            for (name, scroll) in [("settings", 0.0), ("settings-bottom", crate::view::max_scroll(size, 1.0, &ui))] {
                ui.sw.scroll = scroll;
                let surface = crate::ui::popup::PopupSurface::new((size.0 as i32, size.1 as i32))?;
                surface.paint(&gpu, |p| crate::view::paint_settings_window(p, size, 1.0, &ui))?;
                let mut pixels = surface.pixels();
                for px in pixels.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }
                let file = std::path::PathBuf::from(&folder).join(format!("{name}.png"));
                let mut encoder = png::Encoder::new(std::fs::File::create(file).unwrap(), size.0 as u32, size.1 as u32);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
            }
        }
        dock.state.hover = Some(0);
        dock.state.pressed = Some((0, 10.0));
        unsafe {
            SetCapture(hwnd);
        }
        dock.state.set_suppressed(true);
        unsafe {
            SendMessageW(hwnd, WM_LBUTTONDOWN, Some(WPARAM(0)), Some(LPARAM(0)));
            SendMessageW(hwnd, WM_LBUTTONUP, Some(WPARAM(0)), Some(LPARAM(0)));
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            assert_ne!(GetCapture(), hwnd);
            assert!(!IsWindowVisible(hwnd).as_bool());
        }
        assert!(dock.state.request.is_none());
        assert!(dock.state.hover.is_none() && dock.state.pressed.is_none());
        dock.state.set_suppressed(false);
        assert!(dock.state.resumed);
        dock.state.refresh(false);
        assert!(!dock.state.resumed);
        assert!(dock.state.hover.is_none());
        if let Some(folder) = std::env::var_os("NI_DOCK_LIVE_PREVIEW") {
            verify_live_backdrop(&mut dock, std::path::Path::new(&folder))?;
        }
        let before = dock.config().pins;
        let a = dock.state.rects[0].1;
        let b = dock.state.rects[1].1;
        let pack = |r: Rect| {
            LPARAM(((((r.1 + r.3) / 2.0) as i32 as u32) << 16 | (((r.0 + r.2) / 2.0) as i32 as u32 & 0xffff)) as isize)
        };
        unsafe {
            SendMessageW(hwnd, WM_LBUTTONDOWN, Some(WPARAM(0)), Some(pack(a)));
            SendMessageW(hwnd, WM_MOUSEMOVE, Some(WPARAM(0)), Some(pack(b)));
            assert_eq!(dock.config().pins[1], before[0], "Pinned entries must move before mouse release");
            SendMessageW(hwnd, WM_LBUTTONUP, Some(WPARAM(0)), Some(pack(b)));
        }
        assert!(matches!(dock.take_request(), Some(Request::Save)));
        assert_eq!(dock.config().pins[1], before[0]);
        assert_eq!(dock.config().pins[0], before[1]);
        let stable = dock.config().pins;
        dock.state.feedback.set_enabled(false);
        dock.state.render();
        let a = dock.state.rects[0].1;
        let b = dock.state.rects[1].1;
        unsafe {
            SendMessageW(hwnd, WM_LBUTTONDOWN, Some(WPARAM(0)), Some(pack(a)));
            SendMessageW(hwnd, WM_MOUSEMOVE, Some(WPARAM(0)), Some(pack(b)));
            SendMessageW(hwnd, WM_CAPTURECHANGED, Some(WPARAM(0)), Some(LPARAM(0)));
            let _ = ReleaseCapture();
        }
        assert_eq!(dock.config().pins, stable, "Cancelling a drag must restore the previous order");
        assert!(dock.take_request().is_none());
        dock.state.config.lock_pins = true;
        dock.state.render();
        let a = dock.state.rects[0].1;
        let b = dock.state.rects[1].1;
        unsafe {
            SendMessageW(hwnd, WM_LBUTTONDOWN, Some(WPARAM(0)), Some(pack(a)));
            SendMessageW(hwnd, WM_MOUSEMOVE, Some(WPARAM(0)), Some(pack(b)));
            SendMessageW(hwnd, WM_LBUTTONUP, Some(WPARAM(0)), Some(pack(b)));
        }
        assert_eq!(dock.config().pins, stable);
        assert!(dock.take_request().is_none());
        dock.state.scale = 1.0;
        dock.state.layout();
        dock.state.feedback.set_enabled(true);
        dock.state.pointer = None;
        dock.state.render();
        let left = dock.state.rects.first().unwrap().1.0;
        let right = dock.state.rects.last().unwrap().1.2;
        for frame in 0..90 {
            let x = left + (right - left) * frame as f32 / 89.0;
            dock.state.pointer = Some((x, dock.state.panel.3 - 20.0));
            dock.state.render();
            for pair in dock.state.rects.windows(2) {
                assert!(pair[1].1.0 - pair[0].1.2 >= 9.0, "Magnification must not overlap neighboring icons");
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        dock.state.pointer = None;
        dock.state.hover = None;
        let deadline = Instant::now() + Duration::from_secs(2);
        while dock.state.feedback.active() && Instant::now() < deadline {
            dock.state.render();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!dock.state.feedback.active());
        assert!(!dock.state.frame_pending);
        let mut config = dock.config();
        config.pins.clear();
        config.monitor = "disconnected-monitor".into();
        dock.update(&config);
        assert_eq!(dock.state.rects.len(), 1);
        assert!(dock.state.monitor.primary);
        drop(dock);
        unsafe {
            assert!(!IsWindow(Some(hwnd)).as_bool());
            DestroyWindow(parent)?;
        }
        Ok(())
    }

    #[test]
    fn overflow_capacity_leaves_room_for_magnification_and_add_button() {
        for scale in [1.0, 1.5, 2.0] {
            for icon in [32.0, 48.0, 72.0] {
                let count = capacity(1280.0, scale, icon, 1.7);
                let width = (count as f32 * (icon + 12.0) + 40.0 + icon * 0.7 * 3.0) * scale;
                assert!(width <= 1280.0);
                assert!(count >= 2);
            }
        }
    }
}
