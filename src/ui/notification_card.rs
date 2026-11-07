use std::cell::Cell;
use std::sync::Once;
use std::time::{Duration, Instant};

use windows::UI::Color;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{CompositionRoundedRectangleGeometry, Compositor, ContainerVisual, SpriteVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, Result, h, w};
use windows_numerics::Vector2;

use crate::data::settings::{AnimCurve, Edge};
use crate::platform::notifications::Record;
use crate::ui::gfx::{Gpu, Layer};
use crate::view::{self, Rect};

pub const WM_NOTIFICATION_CARD: u32 = WM_APP + 57;
pub const OPEN_MS: u32 = 320;
pub const CLOSE_MS: u32 = 220;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Enter = 1,
    Leave = 2,
    Dismiss = 3,
    History = 4,
    Hover = 5,
    ScrollUp = 6,
    ScrollDown = 7,
}

struct State {
    parent: HWND,
    frame: (f32, f32),
    from: Rect,
    to: Rect,
    started: Instant,
    duration: u32,
    scale: f32,
    tracking: bool,
    hover_close: bool,
    closing: bool,
    scroll: Option<f32>,
    scroll_max: Cell<f32>,
}

impl State {
    fn rect(&self) -> Rect {
        let p = crate::ui::anim::ease(
            AnimCurve::Smooth,
            self.started.elapsed().as_millis() as f32 / self.duration.max(1) as f32,
        );
        let mix = |a: f32, b: f32| a + (b - a) * p;
        (
            mix(self.from.0, self.to.0),
            mix(self.from.1, self.to.1),
            mix(self.from.2, self.to.2),
            mix(self.from.3, self.to.3),
        )
    }
}

fn notify(hwnd: HWND, state: &State, action: Action) {
    unsafe {
        let _ =
            PostMessageW(Some(state.parent), WM_NOTIFICATION_CARD, WPARAM(action as usize), LPARAM(hwnd.0 as isize));
    }
}

fn inside(rect: Rect, scale: f32, point: (f32, f32)) -> bool {
    view::rounded_body_contains(rect, 26.0 * scale, point)
}

extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let state = &mut *ptr;
        match msg {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_NCHITTEST => {
                let point = (lparam.0 as i16 as f32, (lparam.0 >> 16) as i16 as f32);
                LRESULT(if inside(state.rect(), state.scale, point) {
                    HTCLIENT as isize
                } else {
                    HTTRANSPARENT as isize
                })
            }
            WM_MOUSEMOVE => {
                if !state.tracking {
                    state.tracking = true;
                    let mut tracking = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut tracking);
                    notify(hwnd, state, Action::Enter);
                }
                let point = (lparam.0 as i16 as f32 + state.frame.0, (lparam.0 >> 16) as i16 as f32 + state.frame.1);
                let close = view::notification_close(state.to, state.scale);
                let hover = !state.closing
                    && point.0 >= close.0
                    && point.0 < close.2
                    && point.1 >= close.1
                    && point.1 < close.3;
                if state.hover_close != hover {
                    state.hover_close = hover;
                    notify(hwnd, state, Action::Hover);
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                state.tracking = false;
                state.hover_close = false;
                notify(hwnd, state, Action::Leave);
                LRESULT(0)
            }
            WM_MOUSEWHEEL if state.scroll.is_some() && !state.closing => {
                let action = if (wparam.0 >> 16) as i16 > 0 { Action::ScrollUp } else { Action::ScrollDown };
                notify(hwnd, state, action);
                LRESULT(0)
            }
            WM_LBUTTONUP if !state.closing => {
                notify(hwnd, state, if state.hover_close { Action::Dismiss } else { Action::History });
                LRESULT(0)
            }
            WM_RBUTTONUP | WM_CLOSE => {
                notify(hwnd, state, Action::Dismiss);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub fn card_target(source: Rect, work: Rect, edge: Option<Edge>, scale: f32, record: Option<&Record>) -> Rect {
    let (left, top, right, bottom) = work;
    let width = (record.map(view::notification_card_width).unwrap_or(view::NOTIFICATION_SIZE.0) * scale)
        .min((right - left - 16.0 * scale).max(1.0));
    let height = record
        .map(|record| view::notification_card_height(record, width, scale))
        .unwrap_or(view::NOTIFICATION_SIZE.1 * scale)
        .min((bottom - top - 16.0 * scale).max(1.0));
    let x = match edge {
        Some(Edge::Left) => source.0,
        Some(Edge::Right) => source.0 + source.2 - width,
        _ => source.0 + (source.2 - width) / 2.0,
    };
    let y = match edge {
        Some(Edge::Bottom) => source.1 + source.3 - height,
        Some(Edge::Left | Edge::Right) => source.1 + (source.3 - height) / 2.0,
        _ => source.1,
    };
    (x.clamp(left, (right - width).max(left)), y.clamp(top, (bottom - height).max(top)), width, height)
}

pub fn pet_card_target(source: Rect, work: Rect, scale: f32, expanded: bool, record: Option<&Record>) -> Rect {
    let mut target = card_target(source, work, None, scale, record);
    let gap = 8.0 * scale;
    if expanded {
        let space = (source.1 - work.1).max(work.3 - source.1 - source.3) - gap;
        target.3 = target.3.max((330.0 * scale).min(space));
    }
    let above = source.1 - target.3 - gap;
    target.1 = if above >= work.1 { above } else { (source.1 + source.3 + gap).min(work.3 - target.3).max(work.1) };
    target
}

pub fn expanded_card_layout(island: Rect, pill: Rect, work: Rect, scale: f32, record: Option<&Record>) -> (Rect, Rect) {
    let target = pet_card_target(island, work, scale, false, record);
    let (x, y, width, height) = island;
    let w = pill.2.min(width);
    let h = pill.3.min(height);
    let anchor_y = if target.1 < y { y } else { y + height - h };
    ((x + (width - w) / 2.0, anchor_y, w, h), target)
}

pub struct NotificationCard {
    hwnd: HWND,
    state: Box<State>,
    compositor: Compositor,
    _target: DesktopWindowTarget,
    _root: ContainerVisual,
    background: SpriteVisual,
    geometry: CompositionRoundedRectangleGeometry,
    gpu: Gpu,
    content: Layer,
    card: Rect,
}

impl NotificationCard {
    pub fn new(parent: HWND, compositor: &Compositor, gpu: &Gpu, scale: f32) -> Result<Self> {
        static CLASS: Once = Once::new();
        let instance = unsafe { GetModuleHandleW(None)? };
        CLASS.call_once(|| unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("NightIsland.NotificationCard"),
                ..Default::default()
            });
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                w!("NightIsland.NotificationCard"),
                w!("Night Island notification"),
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
        struct WindowGuard(HWND);
        impl Drop for WindowGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                }
            }
        }
        let guard = WindowGuard(hwnd);
        let mut state = Box::new(State {
            parent,
            frame: (0.0, 0.0),
            from: (0.0, 0.0, 1.0, 1.0),
            to: (0.0, 0.0, 1.0, 1.0),
            started: Instant::now(),
            duration: OPEN_MS,
            scale,
            tracking: false,
            hover_close: false,
            closing: false,
            scroll: None,
            scroll_max: Cell::new(0.0),
        });
        let target = unsafe { compositor.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(hwnd, true)? };
        let root = compositor.CreateContainerVisual()?;
        root.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        let background = compositor.CreateSpriteVisual()?;
        background.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        let (r, g, b) = super::theme::panel_u8();
        background.SetBrush(&compositor.CreateColorBrushWithColor(Color { A: 255, R: r, G: g, B: b })?)?;
        let content = gpu.layer(compositor, (1, 1))?;
        root.Children()?.InsertAtTop(&background)?;
        root.Children()?.InsertAtTop(&content.sprite)?;
        let geometry = compositor.CreateRoundedRectangleGeometry()?;
        root.SetClip(&compositor.CreateGeometricClipWithGeometry(&geometry)?)?;
        target.SetRoot(&root)?;
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&raw mut *state) as isize);
        }
        std::mem::forget(guard);
        Ok(Self {
            hwnd,
            state,
            compositor: compositor.clone(),
            _target: target,
            _root: root,
            background,
            geometry,
            gpu: gpu.popup(),
            content,
            card: (0.0, 0.0, 1.0, 1.0),
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn target_rect(&self) -> Rect {
        self.card
    }

    pub fn screen_rect(&self) -> Rect {
        self.state.rect()
    }

    pub fn hovering_close(&self) -> bool {
        self.state.hover_close
    }

    pub fn set_capture_hidden(&self, hidden: bool) {
        unsafe {
            let _ = SetWindowDisplayAffinity(self.hwnd, if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
        }
    }

    fn reframe(&mut self, from: Rect, to: Rect) -> Result<()> {
        let margin = 8.0 * self.state.scale;
        let left = (from.0.min(to.0) - margin).floor();
        let top = (from.1.min(to.1) - margin).floor();
        let width = ((from.0 + from.2).max(to.0 + to.2) - left + margin).ceil() as i32;
        let height = ((from.1 + from.3).max(to.1 + to.3) - top + margin).ceil() as i32;
        self.content.resize((width.max(1), height.max(1)))?;
        self.state.frame = (left, top);
        unsafe {
            SetWindowPos(self.hwnd, Some(HWND_TOPMOST), left as i32, top as i32, width, height, SWP_NOACTIVATE)?;
        }
        Ok(())
    }

    fn transition(&mut self, from: Rect, to: Rect, duration: u32) -> Result<()> {
        self.reframe(from, to)?;
        self.state.from = from;
        self.state.to = to;
        self.state.started = Instant::now();
        self.state.duration = duration;
        let frame = self.state.frame;
        for name in [h!("Offset"), h!("Size"), h!("CornerRadius")] {
            self.geometry.StopAnimation(name)?;
        }
        self.geometry.SetOffset(Vector2 { X: from.0 - frame.0, Y: from.1 - frame.1 })?;
        self.geometry.SetSize(Vector2 { X: from.2, Y: from.3 })?;
        let start_radius = (if self.state.closing { 26.0 } else { 17.0 }) * self.state.scale;
        let end_radius = (if self.state.closing { 17.0 } else { 26.0 }) * self.state.scale;
        self.geometry.SetCornerRadius(Vector2 { X: start_radius, Y: start_radius })?;
        let easing = self
            .compositor
            .CreateCubicBezierEasingFunction(Vector2 { X: 0.45, Y: 0.0 }, Vector2 { X: 0.55, Y: 1.0 })?;
        for (name, value) in [
            (h!("Offset"), Vector2 { X: to.0 - frame.0, Y: to.1 - frame.1 }),
            (h!("Size"), Vector2 { X: to.2, Y: to.3 }),
            (h!("CornerRadius"), Vector2 { X: end_radius, Y: end_radius }),
        ] {
            let animation = self.compositor.CreateVector2KeyFrameAnimation()?;
            animation.InsertKeyFrameWithEasingFunction(1.0, value, &easing)?;
            animation.SetDuration(Duration::from_millis(u64::from(duration)).into())?;
            self.geometry.StartAnimation(name, &animation)?;
        }
        Ok(())
    }

    pub fn show(&mut self, record: &Record, pending: usize, source: Rect, target: Rect, scale: f32) -> Result<()> {
        self.state.scale = scale;
        self.state.closing = false;
        self.state.scroll = None;
        self.state.scroll_max.set(0.0);
        self.state.tracking = false;
        self.state.hover_close = false;
        self.card = target;
        self.transition(source, self.card, OPEN_MS)?;
        self.update(record, pending)?;
        let fade = self.compositor.CreateScalarKeyFrameAnimation()?;
        fade.InsertKeyFrame(0.0, 0.0)?;
        fade.InsertKeyFrame(0.25, 0.0)?;
        fade.InsertKeyFrame(1.0, 1.0)?;
        fade.SetDuration(Duration::from_millis(u64::from(OPEN_MS)).into())?;
        self.content.sprite.StartAnimation(h!("Opacity"), &fade)?;
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(())
    }

    pub fn expand(&mut self, source: Rect, work: Rect, record: &Record, pending: usize) -> Result<()> {
        if self.state.scroll.is_some() {
            return Ok(());
        }
        let from = self.state.rect();
        self.card = pet_card_target(source, work, self.state.scale, true, Some(record));
        self.state.scroll = Some(0.0);
        self.transition(from, self.card, OPEN_MS)?;
        self.update(record, pending)
    }

    pub fn scroll(&mut self, down: bool) {
        if let Some(scroll) = &mut self.state.scroll {
            let delta = if down { 44.0 } else { -44.0 } * self.state.scale;
            *scroll = (*scroll + delta).clamp(0.0, self.state.scroll_max.get());
        }
    }

    pub fn update(&self, record: &Record, pending: usize) -> Result<()> {
        let (r, g, b) = super::theme::panel_u8();
        self.background.SetBrush(&self.compositor.CreateColorBrushWithColor(Color { A: 255, R: r, G: g, B: b })?)?;
        let rect = (self.card.0 - self.state.frame.0, self.card.1 - self.state.frame.1, self.card.2, self.card.3);
        self.content.paint(&self.gpu, |p| {
            if let Ok(max) = view::paint_notification_card(
                p,
                rect,
                self.state.scale,
                record,
                pending,
                self.state.hover_close,
                self.state.scroll,
            ) {
                self.state.scroll_max.set(max);
            }
        })
    }

    pub fn close(&mut self, source: Rect, record: &Record, pending: usize) -> Result<()> {
        let from = self.state.rect();
        self.state.closing = true;
        self.transition(from, source, CLOSE_MS)?;
        self.update(record, pending)?;
        let fade = self.compositor.CreateScalarKeyFrameAnimation()?;
        fade.InsertKeyFrame(1.0, 0.0)?;
        fade.SetDuration(Duration::from_millis(110).into())?;
        self.content.sprite.StartAnimation(h!("Opacity"), &fade)?;
        Ok(())
    }

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        for name in [h!("Offset"), h!("Size"), h!("CornerRadius")] {
            let _ = self.geometry.StopAnimation(name);
        }
        let _ = self.content.sprite.StopAnimation(h!("Opacity"));
    }
}

impl Drop for NotificationCard {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinking_pet_card_keeps_hit_bounds_ordered_at_any_monitor_origin() {
        let frame = (2196.0686, 386.0, 51.86258, 53.1);
        assert!(inside(frame, 1.0, (2222.0, 412.0)));

        for (left, top, width, height) in [
            (0.0, 0.0, 1920.0, 1080.0),
            (0.0, 0.0, 2560.0, 1440.0),
            (1920.0, 0.0, 3840.0, 2160.0),
            (-5120.0, -1440.0, 5120.0, 1440.0),
            (3840.0, -2160.0, 7680.0, 4320.0),
        ] {
            for scale in [0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5] {
                for (w, h) in [(frame.2, frame.3), (frame.3, frame.2), (frame.2, frame.2)] {
                    let center = (left + width * 0.85, top + height * 0.37);
                    let (w, h) = (w * scale, h * scale);
                    let (x, y) = (center.0 - w / 2.0, center.1 - h / 2.0);
                    let frame = (x, y, w, h);
                    assert!(inside(frame, scale, center));
                    assert!(!inside(frame, scale, (x, y)));
                    assert!(!inside(frame, scale, (x + w + scale, center.1)));
                    assert!(!inside(frame, scale, (center.0, y + h + scale)));
                }
            }
        }
    }

    #[test]
    fn notification_cards_fit_every_edge_and_negative_monitor_coordinates() {
        for scale in [0.75, 1.0, 1.5, 2.0] {
            for work in [(0.0, 0.0, 1920.0, 1040.0), (-1920.0, -1080.0, 0.0, 0.0)] {
                for edge in Edge::ALL.into_iter().map(Some).chain([None]) {
                    let source = match edge {
                        Some(Edge::Bottom) => (work.0 + 800.0, work.3 - 34.0, 200.0, 34.0),
                        Some(Edge::Left) => (work.0, work.1 + 500.0, 42.0, 200.0),
                        Some(Edge::Right) => (work.2 - 42.0, work.1 + 500.0, 42.0, 200.0),
                        _ => (work.0 + 800.0, work.1, 200.0, 34.0),
                    };
                    let target = card_target(source, work, edge, scale, None);
                    assert!(target.0 >= work.0 && target.1 >= work.1);
                    assert!(target.0 + target.2 <= work.2 + 0.01 && target.1 + target.3 <= work.3 + 0.01);
                    assert!(inside(target, scale, (target.0 + target.2 / 2.0, target.1 + target.3 / 2.0)));
                    assert!(!inside(target, scale, (target.0 - 1.0, target.1 - 1.0)));
                    assert!(!inside(target, scale, (target.0, target.1)));
                }
            }
        }
    }
}
