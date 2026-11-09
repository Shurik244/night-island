use std::sync::Once;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW, HBITMAP, HDC,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ReleaseDC, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Error, Result, w};

use super::companion_controls::Controls;
use crate::ui::characters::{self, Character, Pose};
use crate::ui::gfx::{Gpu, rgba};

pub const WM_COMPANION: u32 = WM_APP + 45;
const SIDE: f32 = 88.0;

pub fn dock_hit(pill: (f32, f32, f32, f32), point: (f32, f32), scale: f32) -> bool {
    let (x, y, w, h) = pill;
    let radius = (w.min(h) / 2.0).min(24.0 * scale);
    let cx = point.0.clamp((x + radius).min(x + w - radius), x + w - radius);
    let cy = point.1.clamp((y + radius).min(y + h - radius), y + h - radius);
    (point.0 - cx).powi(2) + (point.1 - cy).powi(2) <= (radius + 6.0 * scale).powi(2)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Chat = 1,
    ToggleIsland = 2,
    Moved = 3,
    Dock = 4,
    Previous = 5,
    PlayPause = 6,
    Next = 7,
    Dropped = 8,
    Voice = 9,
}

impl Action {
    pub fn from_id(id: usize) -> Option<Self> {
        match id {
            1 => Some(Self::Chat),
            2 => Some(Self::ToggleIsland),
            3 => Some(Self::Moved),
            4 => Some(Self::Dock),
            5 => Some(Self::Previous),
            6 => Some(Self::PlayPause),
            7 => Some(Self::Next),
            8 => Some(Self::Dropped),
            9 => Some(Self::Voice),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Drag {
    down: POINT,
    origin: POINT,
    moved: bool,
}

struct UndockGuard {
    island: (f32, f32, f32, f32),
    scale: f32,
    left: bool,
}

impl UndockGuard {
    fn observe(&mut self, center: (f32, f32)) {
        self.left |= !dock_hit(self.island, center, self.scale);
    }
}

struct State {
    parent: HWND,
    drag: Option<Drag>,
    controls: HWND,
    docking: bool,
    returning: Option<f32>,
    undock_guard: Option<UndockGuard>,
}

fn notify(hwnd: HWND, state: &State, action: Action) {
    unsafe {
        let _ = PostMessageW(Some(state.parent), WM_COMPANION, WPARAM(action as usize), LPARAM(hwnd.0 as isize));
    }
}

fn clamp_position(pos: [i32; 2], side: i32, work: RECT) -> [i32; 2] {
    [
        pos[0].clamp(work.left, (work.right - side).max(work.left)),
        pos[1].clamp(work.top, (work.bottom - side).max(work.top)),
    ]
}

fn on_screen(pos: [i32; 2], side: i32) -> [i32; 2] {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: pos[0], y: pos[1] }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(monitor, &mut info).as_bool() { clamp_position(pos, side, info.rcWork) } else { pos }
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_MOUSEACTIVATE {
            return LRESULT(MA_NOACTIVATE as isize);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_NCHITTEST => {
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                let x = lparam.0 as i16 as i32;
                let y = (lparam.0 >> 16) as i16 as i32;
                let cx = (rect.left + rect.right) as f32 / 2.0;
                let cy = (rect.top + rect.bottom) as f32 / 2.0;
                let radius = (rect.right - rect.left) as f32 * 0.38;
                LRESULT(if (x as f32 - cx).powi(2) + (y as f32 - cy).powi(2) <= radius * radius {
                    HTCLIENT as isize
                } else {
                    HTTRANSPARENT as isize
                })
            }
            WM_LBUTTONDOWN => {
                let mut down = POINT::default();
                let mut rect = RECT::default();
                if GetCursorPos(&mut down).is_ok() && GetWindowRect(hwnd, &mut rect).is_ok() {
                    (*ptr).undock_guard = None;
                    (*ptr).drag = Some(Drag { down, origin: POINT { x: rect.left, y: rect.top }, moved: false });
                    SetCapture(hwnd);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(mut drag) = (*ptr).drag {
                    let mut point = POINT::default();
                    if GetCursorPos(&mut point).is_ok() {
                        let (dx, dy) = (point.x - drag.down.x, point.y - drag.down.y);
                        drag.moved |= dx.abs().max(dy.abs()) >= 6;
                        (*ptr).drag = Some(drag);
                        if drag.moved {
                            if let Some(guard) = &mut (*ptr).undock_guard {
                                let mut rect = RECT::default();
                                if GetWindowRect(hwnd, &mut rect).is_ok() {
                                    let half = (rect.right - rect.left) as f32 / 2.0;
                                    guard.observe((
                                        drag.origin.x as f32 + dx as f32 + half,
                                        drag.origin.y as f32 + dy as f32 + half,
                                    ));
                                }
                            }
                            let _ = ShowWindow((*ptr).controls, SW_HIDE);
                            let _ = SetWindowPos(
                                hwnd,
                                None,
                                drag.origin.x + dx,
                                drag.origin.y + dy,
                                0,
                                0,
                                SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                            );
                        }
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let drag = (*ptr).drag.take();
                let _ = ReleaseCapture();
                if let Some(drag) = drag {
                    notify(hwnd, &*ptr, if drag.moved { Action::Dropped } else { Action::Chat });
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                (*ptr).drag = None;
                LRESULT(0)
            }
            WM_MBUTTONUP => {
                notify(hwnd, &*ptr, Action::Voice);
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                notify(hwnd, &*ptr, Action::ToggleIsland);
                LRESULT(0)
            }
            WM_CLOSE => {
                notify(hwnd, &*ptr, Action::Dock);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                notify(hwnd, &*ptr, Action::Moved);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

struct Surface {
    mem: HDC,
    dib: HBITMAP,
    side: i32,
}

impl Surface {
    fn new(side: i32) -> Result<Self> {
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            if mem.0.is_null() {
                return Err(Error::from_thread());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: side,
                    biHeight: -side,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let dib = match CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(dib) => dib,
                Err(error) => {
                    let _ = DeleteDC(mem);
                    return Err(error);
                }
            };
            SelectObject(mem, dib.into());
            Ok(Self { mem, dib, side })
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.mem);
            let _ = DeleteObject(self.dib.into());
        }
    }
}

pub struct CompanionWindow {
    hwnd: HWND,
    _state: Box<State>,
    surface: Surface,
    controls: Option<Controls>,
    hover_until: Option<Instant>,
}

impl CompanionWindow {
    pub fn new(parent: HWND, position: Option<[i32; 2]>, scale: f32, gpu: &Gpu) -> Result<Self> {
        static CLASS: Once = Once::new();
        unsafe {
            CLASS.call_once(|| {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(proc),
                    hInstance: GetModuleHandleW(None).unwrap_or_default().into(),
                    hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
                    lpszClassName: w!("NightIsland.Companion"),
                    ..Default::default()
                };
                RegisterClassW(&class);
            });
            let side = (SIDE * scale).round().max(32.0) as i32;
            let mut rect = RECT::default();
            let _ = GetWindowRect(parent, &mut rect);
            let position = on_screen(position.unwrap_or([rect.right + 12, rect.top]), side);
            let mut state = Box::new(State {
                parent,
                drag: None,
                controls: HWND::default(),
                docking: false,
                returning: None,
                undock_guard: None,
            });
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                w!("NightIsland.Companion"),
                w!("Night Island character"),
                WS_POPUP,
                position[0],
                position[1],
                side,
                side,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&raw mut *state) as isize);
            let surface = match Surface::new(side) {
                Ok(surface) => surface,
                Err(error) => {
                    let _ = DestroyWindow(hwnd);
                    return Err(error);
                }
            };
            let mut window = Self { hwnd, _state: state, surface, controls: None, hover_until: None };
            let controls = Controls::new(parent, hwnd, scale, gpu)?;
            window._state.controls = controls.hwnd();
            window.controls = Some(controls);
            Ok(window)
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn position(&self) -> [i32; 2] {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut rect);
        }
        [rect.left, rect.top]
    }

    pub fn center(&self) -> (f32, f32) {
        let [x, y] = self.position();
        (x as f32 + self.surface.side as f32 / 2.0, y as f32 + self.surface.side as f32 / 2.0)
    }

    pub fn notification_anchor(&self) -> (crate::view::Rect, crate::view::Rect) {
        let (x, y) = self.center();
        let side = self.surface.side as f32;
        let source = (x - side * 0.29, y - side * 0.25, side * 0.58, side * 0.6);
        let [left, top, right, bottom] = self.walking_bounds();
        (source, (left, top, right + side, bottom + side))
    }

    pub fn walking_bounds(&self) -> [f32; 4] {
        let [x, y] = self.position();
        unsafe {
            let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                let work = info.rcWork;
                return [
                    work.left as f32,
                    work.top as f32,
                    (work.right - self.surface.side).max(work.left) as f32,
                    (work.bottom - self.surface.side).max(work.top) as f32,
                ];
            }
        }
        [x as f32, y as f32, x as f32, y as f32]
    }

    pub fn controls_visible(&self) -> bool {
        self.controls.as_ref().is_some_and(|controls| unsafe { IsWindowVisible(controls.hwnd()).as_bool() })
    }

    pub fn pointer_near(&self, cursor: (i32, i32)) -> bool {
        let (cx, cy) = self.center();
        let reach = self.surface.side as f32 * 0.8;
        (cursor.0 as f32 - cx).abs() <= reach && (cursor.1 as f32 - cy).abs() <= reach
    }

    pub fn walk_to(&self, position: [f32; 2]) {
        if self.grabbed() || self.controls_visible() {
            return;
        }
        let bounds = self.walking_bounds();
        let target = [
            position[0].clamp(bounds[0], bounds[2]).round() as i32,
            position[1].clamp(bounds[1], bounds[3]).round() as i32,
        ];
        if target != self.position() {
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    target[0],
                    target[1],
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
    }

    pub fn begin_drag(&mut self, cursor: POINT, island: (f32, f32, f32, f32)) {
        let side = self.surface.side;
        let origin = POINT { x: cursor.x - side / 2, y: cursor.y - side / 2 };
        self._state.drag = Some(Drag { down: cursor, origin, moved: true });
        let mut guard = UndockGuard { island, scale: side as f32 / SIDE, left: false };
        guard.observe((cursor.x as f32, cursor.y as f32));
        self._state.undock_guard = Some(guard);
        self.hover_until = None;
        if let Some(controls) = &mut self.controls {
            controls.hide();
        }
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, origin.x, origin.y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER);
            SetCapture(self.hwnd);
        }
    }

    pub fn dragging(&self) -> bool {
        self._state.drag.is_some_and(|drag| drag.moved)
    }

    pub fn grabbed(&self) -> bool {
        self._state.drag.is_some()
    }

    pub fn can_dock(&self) -> bool {
        self._state.undock_guard.as_ref().is_none_or(|guard| guard.left)
    }

    pub fn return_to(&mut self, center: (f32, f32), progress: f32) {
        self._state.returning = Some(progress.clamp(0.0, 1.0));
        self._state.docking = false;
        let half = self.surface.side / 2;
        unsafe {
            let _ = ShowWindow(self._state.controls, SW_HIDE);
            SetWindowLongPtrW(
                self.hwnd,
                GWL_EXSTYLE,
                GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) | WS_EX_TRANSPARENT.0 as isize,
            );
            let _ = SetWindowPos(
                self.hwnd,
                None,
                center.0.round() as i32 - half,
                center.1.round() as i32 - half,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    pub fn cancel_return(&mut self) {
        if self._state.returning.take().is_some() {
            unsafe {
                SetWindowLongPtrW(
                    self.hwnd,
                    GWL_EXSTYLE,
                    GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) & !(WS_EX_TRANSPARENT.0 as isize),
                );
            }
        }
    }

    pub fn set_docking(&mut self, docking: bool) -> bool {
        let changed = self._state.docking != docking;
        self._state.docking = docking;
        changed
    }

    pub fn update_controls(
        &mut self,
        available: bool,
        media: &crate::platform::media::Media,
        progress: Option<(f32, f32)>,
        scale: f32,
    ) -> Result<()> {
        let mut point = POINT::default();
        if unsafe { GetCursorPos(&mut point) }.is_err() {
            return Ok(());
        }
        self.update_controls_at(point, available, media, progress, scale)
    }

    fn update_controls_at(
        &mut self,
        point: POINT,
        available: bool,
        media: &crate::platform::media::Media,
        progress: Option<(f32, f32)>,
        scale: f32,
    ) -> Result<()> {
        if !available || self.dragging() {
            self.hover_until = None;
            if let Some(controls) = &mut self.controls {
                controls.hide();
            }
            return Ok(());
        }
        let Some(controls) = &mut self.controls else {
            return Ok(());
        };
        unsafe {
            let mut rect = RECT::default();
            GetWindowRect(self.hwnd, &mut rect)?;
            let monitor = MonitorFromPoint(POINT { x: rect.left, y: rect.top }, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return Ok(());
            }
            let panel = controls.rect_at(rect, info.rcWork, scale);
            let center = POINT { x: (rect.left + rect.right) / 2, y: (rect.top + rect.bottom) / 2 };
            let radius = self.surface.side as f32 * 0.38;
            let over_character =
                ((point.x - center.x) as f32).powi(2) + ((point.y - center.y) as f32).powi(2) <= radius * radius;
            let bridge = RECT {
                left: rect.left.min(panel.left),
                top: rect.top.min(panel.top),
                right: rect.right.max(panel.right),
                bottom: rect.bottom.max(panel.bottom),
            };
            let inside_bridge =
                point.x >= bridge.left && point.x < bridge.right && point.y >= bridge.top && point.y < bridge.bottom;
            let now = Instant::now();
            if over_character || (self.hover_until.is_some_and(|until| now < until) && inside_bridge) {
                self.hover_until = Some(now + Duration::from_millis(300));
            }
            if self.hover_until.is_some_and(|until| now < until) {
                controls.show(panel, point, media, progress, scale)?;
            } else {
                controls.hide();
            }
        }
        Ok(())
    }

    pub fn keep_on_screen(&self) {
        let pos = on_screen(self.position(), self.surface.side);
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, pos[0], pos[1], 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER);
        }
    }

    pub fn hide(&mut self) {
        self._state.drag = None;
        unsafe {
            if GetCapture() == self.hwnd {
                let _ = ReleaseCapture();
            }
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            if let Some(controls) = &mut self.controls {
                controls.hide();
            }
        }
    }

    pub fn set_capture_hidden(&self, hidden: bool) {
        if let Some(controls) = &self.controls {
            controls.set_capture_hidden(hidden);
        }
        unsafe {
            let _ = SetWindowDisplayAffinity(self.hwnd, if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
        }
    }

    #[cfg(test)]
    pub fn render(&mut self, gpu: &Gpu, character: Character, pose: &Pose, scale: f32) -> Result<()> {
        self.render_designed(gpu, character, pose, scale, None)
    }

    pub fn render_designed(
        &mut self,
        gpu: &Gpu,
        character: Character,
        pose: &Pose,
        scale: f32,
        design: Option<&super::pet_design::PetDesign>,
    ) -> Result<()> {
        let side = (SIDE * scale).round().max(32.0) as i32;
        if self.surface.side != side {
            self.surface = Surface::new(side)?;
            self.keep_on_screen();
        }
        let mut drawn = Ok(());
        gpu.paint_dc(self.surface.mem, (side, side), |p| {
            if self._state.docking && self._state.returning.is_none() {
                let inset = side as f32 * 0.12;
                let rect = (inset, inset, side as f32 - inset, side as f32 - inset);
                let _ = p.fill_round_rect(rect, side as f32 / 2.0, rgba(0.4, 0.9, 0.7, 0.15));
                let _ = p.stroke_round_rect(rect, side as f32 / 2.0, 1.5 * scale, rgba(0.4, 0.95, 0.75, 0.85));
            }
            let factor = 1.0 - 0.7 * self._state.returning.unwrap_or(0.0);
            let shift = side as f32 * (1.0 - factor) / 2.0;
            let old = p.push_offset(0.0, 0.0);
            p.set_transform(&windows_numerics::Matrix3x2 {
                M11: factor,
                M22: factor,
                M31: shift + side as f32 * 0.05 * factor,
                M32: shift + side as f32 * 0.1625 * factor,
                ..Default::default()
            });
            drawn = characters::draw_with_design(p, character, side as f32 * 0.9, pose, design);
            p.set_transform(&old);
        })?;
        drawn?;
        unsafe {
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: ((1.0 - ((self._state.returning.unwrap_or(0.0) - 0.65) / 0.35).clamp(0.0, 1.0))
                    * 255.0)
                    .round() as u8,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(
                self.hwnd,
                None,
                None,
                Some(&SIZE { cx: side, cy: side }),
                Some(self.surface.mem),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )?;
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(())
    }
}

impl Drop for CompanionWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docking_never_panics_on_rounding_of_the_clamp_bounds() {
        for i in 0..5000 {
            let x = i as f32 * 0.0731;
            let w = 40.0 + (i % 7) as f32 * 0.37;
            dock_hit((x, 5.0, w, w * 3.0), (x, 0.0), 1.0);
        }
    }

    #[test]
    fn a_newly_released_pet_must_leave_the_island_before_it_can_dock() {
        for scale in [0.75, 1.0, 2.0] {
            let island = (100.0 * scale, 50.0 * scale, 200.0 * scale, 34.0 * scale);
            let mut guard = UndockGuard { island, scale, left: false };
            for center in [(120.0, 67.0), (160.0, 67.0), (250.0, 67.0)] {
                guard.observe((center.0 * scale, center.1 * scale));
                assert!(!guard.left);
            }
            guard.observe((200.0 * scale, 100.0 * scale));
            assert!(guard.left);
            guard.observe((120.0 * scale, 67.0 * scale));
            assert!(guard.left);
        }
    }

    #[test]
    fn docking_uses_the_visible_rounded_island_instead_of_its_window_padding() {
        let pill = (100.0, 50.0, 300.0, 40.0);
        assert!(dock_hit(pill, (250.0, 70.0), 1.0));
        assert!(dock_hit(pill, (101.0, 70.0), 1.0));
        assert!(!dock_hit(pill, (100.0, 50.0), 1.0));
        assert!(!dock_hit(pill, (250.0, 120.0), 1.0));
        assert!(dock_hit((-1400.0, 50.0, 300.0, 40.0), (-1250.0, 70.0), 1.0));
        assert!(dock_hit((200.0, 100.0, 600.0, 80.0), (500.0, 180.0), 2.0));
        assert!(dock_hit((50.0, 100.0, 28.0, 300.0), (64.0, 250.0), 1.0));
    }

    fn pixels(surface: &Surface) -> Vec<u8> {
        use windows::Win32::Graphics::Gdi::{BITMAP, GetObjectW};
        let mut bitmap = BITMAP::default();
        unsafe {
            assert_ne!(GetObjectW(surface.dib.into(), size_of::<BITMAP>() as i32, Some((&raw mut bitmap).cast())), 0);
            std::slice::from_raw_parts(bitmap.bmBits.cast::<u8>(), (surface.side * surface.side * 4) as usize).to_vec()
        }
    }

    fn save_preview(path: &std::path::Path, width: u32, height: u32, bgra: &[u8]) {
        let rgba: Vec<_> = bgra
            .chunks_exact(4)
            .flat_map(|p| {
                let alpha = p[3] as u32;
                [p[2], p[1], p[0]]
                    .map(|v| (u32::from(v) + (255 - alpha) * 22 / 255).min(255) as u8)
                    .into_iter()
                    .chain([255])
            })
            .collect();
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    }

    #[test]
    fn saved_positions_remain_reachable_after_a_monitor_change() {
        let work = RECT { left: 0, top: 0, right: 1920, bottom: 1040 };
        assert_eq!(clamp_position([-5000, -3000], 88, work), [0, 0]);
        assert_eq!(clamp_position([5000, 3000], 88, work), [1832, 952]);
        assert_eq!(clamp_position([300, 200], 88, work), [300, 200]);
        let left = RECT { left: -1920, top: 0, right: 0, bottom: 1040 };
        assert_eq!(clamp_position([-2500, 200], 88, left), [-1920, 200]);
    }

    #[test]
    #[ignore = "Requires an interactive Windows desktop and an isolated Composition runtime"]
    fn desktop_characters_keep_their_shape_with_music_and_a_hidden_island() -> Result<()> {
        use windows::UI::Composition::Compositor;
        use windows::Win32::System::WinRT::{
            CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
            RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
        };
        struct Runtime;
        impl Drop for Runtime {
            fn drop(&mut self) {
                unsafe { RoUninitialize() };
            }
        }
        unsafe {
            RoInitialize(RO_INIT_SINGLETHREADED)?;
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
                w!("Spark test owner"),
                WS_POPUP,
                0,
                0,
                200,
                50,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?
        };
        let mut spark = CompanionWindow::new(parent, Some([320, 320]), 1.0, &gpu)?;
        let pose = Pose {
            motion: Default::default(),
            t: 0.0,
            reaction: characters::Reaction::Idle,
            look: (0.0, 0.0),
            blink: false,
            accent: (0.5, 0.6, 1.0),
            beat: 0.0,
        };
        spark.render(&gpu, Character::Spark, &pose, 1.0)?;
        unsafe {
            assert!(IsWindowVisible(spark.hwnd()).as_bool());
            assert!(GetWindow(spark.hwnd(), GW_OWNER).is_err());
            let _ = ShowWindow(parent, SW_HIDE);
            assert!(IsWindowVisible(spark.hwnd()).as_bool());
        }
        let mut sheet = vec![0; (Character::ALL.len() - 1) * 88 * 3 * 88 * 4];
        for (column, character) in Character::ALL.into_iter().filter(|c| *c != Character::None).enumerate() {
            for (row, beat) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                let music = Pose { reaction: characters::Reaction::Music, t: row as f32 * 0.3, beat, ..pose };
                unsafe {
                    let _ = ShowWindow(parent, SW_SHOWNOACTIVATE);
                }
                spark.render(&gpu, character, &music, 1.0)?;
                let before = pixels(&spark.surface);
                unsafe {
                    let _ = ShowWindow(parent, SW_HIDE);
                }
                spark.render(&gpu, character, &music, 1.0)?;
                let after = pixels(&spark.surface);
                assert_eq!(before, after, "{character:?}: hiding the island changed the canvas");
                let mut rect = RECT::default();
                unsafe {
                    GetWindowRect(spark.hwnd(), &mut rect)?;
                }
                assert_eq!((rect.right - rect.left, rect.bottom - rect.top), (88, 88));
                let mut bounds = (88usize, 88usize, 0usize, 0usize);
                for (i, pixel) in after.chunks_exact(4).enumerate() {
                    let (x, y) = (i % 88, i / 88);
                    if pixel[3] > 0 {
                        assert!(x > 0 && x < 87 && y > 0 && y < 87, "clipped {character:?} at beat {beat}");
                    }
                    if pixel[3] >= 250 {
                        bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
                    }
                }
                assert!(bounds.2 > bounds.0 && bounds.3 > bounds.1);
                if character == Character::Spark {
                    assert!(
                        ((bounds.2 - bounds.0) as i32 - (bounds.3 - bounds.1) as i32).abs() <= 3,
                        "Spark lost its circular shape: {bounds:?}"
                    );
                }
                for y in 0..88 {
                    let dst = ((row * 88 + y) * (Character::ALL.len() - 1) * 88 + column * 88) * 4;
                    sheet[dst..dst + 88 * 4].copy_from_slice(&after[y * 88 * 4..(y + 1) * 88 * 4]);
                }
            }
        }
        let media = crate::platform::media::Media {
            title: "Midnight City".into(),
            artist: "M83".into(),
            playing: true,
            ..Default::default()
        };
        let (cx, cy) = spark.center();
        spark.update_controls_at(POINT { x: cx as i32, y: cy as i32 }, true, &media, Some((84.0, 244.0)), 1.0)?;
        let controls = spark.controls.as_ref().unwrap();
        let panel = controls.hwnd();
        assert!(unsafe { IsWindowVisible(panel) }.as_bool());
        assert_eq!(unsafe { GetWindow(panel, GW_OWNER) }?, spark.hwnd());
        controls.verify_actions(parent);
        let mut character_rect = RECT::default();
        unsafe {
            GetWindowRect(spark.hwnd(), &mut character_rect)?;
        }
        let mut panel_rect = RECT::default();
        unsafe {
            GetWindowRect(panel, &mut panel_rect)?;
        }
        assert_eq!((character_rect.right - character_rect.left, character_rect.bottom - character_rect.top), (88, 88));
        spark.update_controls_at(
            POINT { x: panel_rect.left + 148, y: panel_rect.top + 98 },
            true,
            &media,
            Some((84.0, 244.0)),
            1.0,
        )?;
        assert!(unsafe { IsWindowVisible(panel) }.as_bool());
        let before = spark.position();
        spark.walk_to([before[0] as f32 + 40.0, before[1] as f32]);
        assert_eq!(spark.position(), before, "An open player must keep its pet anchored");
        spark.update_controls_at(POINT::default(), false, &media, None, 1.0)?;
        assert!(!unsafe { IsWindowVisible(panel) }.as_bool());
        let bounds = spark.walking_bounds();
        let foreground = unsafe { GetForegroundWindow() };
        spark.walk_to([bounds[2] + 200.0, bounds[3] + 200.0]);
        assert_eq!(spark.position(), [bounds[2] as i32, bounds[3] as i32]);
        assert_eq!(unsafe { GetForegroundWindow() }, foreground, "Walking must never steal focus");
        spark.walk_to(before.map(|v| v as f32));
        for work in
            [RECT { left: 0, top: 0, right: 500, bottom: 500 }, RECT { left: -1920, top: 0, right: 0, bottom: 1040 }]
        {
            let near_bottom =
                RECT { left: work.left, top: work.bottom - 88, right: work.left + 88, bottom: work.bottom };
            let r = spark.controls.as_ref().unwrap().rect_at(near_bottom, work, 1.0);
            assert!(r.left >= work.left && r.right <= work.right);
            assert!(r.top >= work.top && r.bottom <= work.bottom);
            assert!(r.bottom <= near_bottom.top + 4);
        }
        if let Some(dir) = std::env::var_os("NI_COMPANION_PREVIEW") {
            let dir = std::path::PathBuf::from(dir);
            save_preview(&dir.join("characters.png"), (Character::ALL.len() as u32 - 1) * 88, 264, &sheet);
            spark.update_controls_at(POINT { x: cx as i32, y: cy as i32 }, true, &media, Some((84.0, 244.0)), 1.0)?;
            let controls = spark.controls.as_ref().unwrap();
            save_preview(&dir.join("companion-controls.png"), 300, 140, &controls.preview());
            let surface = Surface::new(640)?;
            let ui = crate::view::UiState::new();
            gpu.paint_dc(surface.mem, (640, 640), |p| {
                crate::view::paint_expanded(p, (0.0, 0.0, 620.0, 400.0), 1.0, &ui, &[]).unwrap();
            })?;
            save_preview(&dir.join("header.png"), 640, 80, &pixels(&surface)[..640 * 80 * 4]);
        }
        let mut message = MSG::default();
        let source = spark.hwnd().0 as isize;
        let mut next = |expected: Action| {
            assert!(
                unsafe { PeekMessageW(&mut message, Some(parent), WM_COMPANION, WM_COMPANION, PM_REMOVE) }.as_bool()
            );
            assert_eq!(Action::from_id(message.wParam.0), Some(expected));
            assert_eq!(message.lParam.0, source);
        };
        proc(spark.hwnd(), WM_RBUTTONUP, WPARAM(0), LPARAM(0));
        next(Action::ToggleIsland);
        spark._state.drag = Some(Drag { down: POINT::default(), origin: POINT::default(), moved: false });
        proc(spark.hwnd(), WM_LBUTTONUP, WPARAM(0), LPARAM(0));
        next(Action::Chat);
        spark._state.drag = Some(Drag { down: POINT::default(), origin: POINT::default(), moved: true });
        proc(spark.hwnd(), WM_LBUTTONUP, WPARAM(0), LPARAM(0));
        next(Action::Dropped);
        spark.begin_drag(POINT { x: 420, y: 380 }, (400.0, 363.0, 200.0, 34.0));
        assert!(spark.dragging());
        assert!(spark.grabbed());
        let held_position = spark.position();
        spark.walk_to([held_position[0] as f32 + 100.0, held_position[1] as f32]);
        assert_eq!(spark.position(), held_position);
        assert!(!spark.can_dock());
        assert_eq!(spark.center(), (420.0, 380.0));
        assert_eq!(unsafe { GetCapture() }, spark.hwnd());
        proc(spark.hwnd(), WM_LBUTTONUP, WPARAM(0), LPARAM(0));
        next(Action::Dropped);
        assert!(!spark.dragging());
        assert!(!spark.grabbed());
        assert!(!spark.can_dock());
        spark._state.undock_guard.as_mut().unwrap().observe((420.0, 450.0));
        assert!(spark.can_dock());
        proc(spark.hwnd(), WM_CLOSE, WPARAM(0), LPARAM(0));
        next(Action::Dock);
        spark.hide();
        assert!(!unsafe { IsWindowVisible(spark.hwnd()) }.as_bool());
        spark.render(&gpu, Character::Spark, &pose, 1.25)?;
        let mut rect = RECT::default();
        unsafe {
            GetWindowRect(spark.hwnd(), &mut rect)?;
        }
        assert_eq!(rect.right - rect.left, 110);
        let (cx, cy) = spark.center();
        assert!(spark.pointer_near((cx as i32 + 70, cy as i32)));
        assert!(!spark.pointer_near((cx as i32 + 140, cy as i32)));
        let hwnd = spark.hwnd();
        drop(spark);
        assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
        unsafe {
            DestroyWindow(parent)?;
        }
        Ok(())
    }
}
