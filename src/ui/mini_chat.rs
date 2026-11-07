use std::cell::RefCell;
use std::sync::Once;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, Result, w};

use crate::data::clips;
use crate::services::ai::ChatTurn;
use crate::ui::characters::Character;
use crate::ui::gfx::{Gpu, make_layout};
use crate::ui::popup::PopupSurface;
use crate::view::{self, CompanionChatHit as Hit, CompanionChatView};

pub const WM_MINI_CHAT: u32 = WM_APP + 46;
pub const SEND: usize = 1;
pub const SETUP: usize = 3;
pub const PET: usize = 4;
pub const PET_APPLY: usize = 5;
pub const PET_PHOTO: usize = 6;
pub const PET_TEXT: usize = 7;
pub const PET_BODY: usize = 8;
pub const VOICE: usize = 9;
pub const VOICE_CANCEL: usize = 10;

fn pet_action(action: view::PetStudioHit) -> usize {
    match action {
        view::PetStudioHit::Body => PET_BODY,
        view::PetStudioHit::Apply => PET_APPLY,
        view::PetStudioHit::Photo => PET_PHOTO,
        view::PetStudioHit::TextOnly => PET_TEXT,
    }
}
const CARET: usize = 1;
const ANIMATE: usize = 2;
const PREVIEW: usize = 3;
const WM_MOUSELEAVE: u32 = 0x02A3;

struct State {
    parent: HWND,
    gpu: Gpu,
    surface: PopupSurface,
    size: (i32, i32),
    scale: f32,
    ui: CompanionChatView,
    pressed: Option<Hit>,
    drag: Option<(POINT, RECT, bool)>,
    selecting: bool,
    reading: bool,
    surrogate: Option<u16>,
    opened: Option<Instant>,
    closing: Option<Instant>,
}

impl State {
    fn sync_preview_timer(&self, hwnd: HWND) {
        unsafe {
            if self.ui.active
                && self.drag.is_none()
                && self.closing.is_none()
                && IsWindowVisible(hwnd).as_bool()
                && let Some(interval) = self.ui.chat.pet.preview_interval()
            {
                SetTimer(Some(hwnd), PREVIEW, interval, None);
            } else {
                let _ = KillTimer(Some(hwnd), PREVIEW);
            }
        }
    }

    fn layout(&self) -> view::CompanionChatLayout {
        view::companion_chat_layout((self.size.0 as f32, self.size.1 as f32), self.scale, self.ui.chat.input_h.get())
    }

    fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        let layout = self.layout();
        if self.ui.chat.pet.enabled
            && (x >= layout.history.0 && x < layout.history.2 && y >= layout.history.1 && y < layout.history.3)
        {
            return view::hit_pet_studio(layout.history, self.scale, (x, y), &self.ui.chat.pet).map(Hit::PetAction);
        }
        view::hit_companion_chat(&layout, x, y).filter(|hit| *hit != Hit::Voice || self.ui.voice_enabled)
    }

    fn can_send(&self) -> bool {
        !self.ui.chat.pending
            && !self.ui.chat.pet.pending
            && (!self.ui.chat.input.text().trim().is_empty()
                || (self.ui.chat.pet.enabled && !self.ui.chat.pet.text_only && !self.ui.chat.pet.files.is_empty()))
    }

    fn input_layout(&self) -> Option<windows::Win32::Graphics::DirectWrite::IDWriteTextLayout> {
        let r = self.layout().input;
        make_layout(self.gpu.dwrite(), &self.ui.chat.input.text(), r.2 - r.0, view::AGY_FONT * self.scale).ok()
    }

    fn keep_caret_visible(&mut self) {
        if let Some(layout) = self.input_layout() {
            let mut metrics = windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
            unsafe {
                let _ = layout.GetMetrics(&mut metrics);
            }
            self.ui.chat.input_h.set(metrics.height + 24.0 * self.scale);
            let (_, y, h) =
                crate::tools::edit::caret_xy(&layout, self.ui.chat.input.utf16_at(self.ui.chat.input.caret));
            let r = self.layout().input;
            let height = r.3 - r.1;
            let input = &mut self.ui.chat.input;
            if y < input.scroll {
                input.scroll = y;
            }
            if y + h > input.scroll + height {
                input.scroll = y + h - height;
            }
            input.scroll = input.scroll.max(0.0);
        }
    }

    fn render(&mut self, hwnd: HWND) -> Result<()> {
        let mut rect = RECT::default();
        unsafe {
            GetClientRect(hwnd, &mut rect)?;
        }
        if rect.right <= 1 || rect.bottom <= 1 {
            return Ok(());
        }
        let size = (rect.right.max(1), rect.bottom.max(1));
        if self.size != size {
            self.surface = PopupSurface::new(size)?;
            self.size = size;
        }
        self.surface.paint(&self.gpu, |p| {
            view::paint_companion_chat(p, (size.0 as f32, size.1 as f32), self.scale, &self.ui)
        })?;
        let ease = |at: Instant| {
            let t = (at.elapsed().as_secs_f32() / 0.18).clamp(0.0, 1.0);
            1.0 - (1.0 - t).powi(3)
        };
        let alpha = if let Some(at) = self.closing { 1.0 - ease(at) } else { self.opened.map_or(1.0, ease) };
        self.surface.present(hwnd, None, (alpha * 255.0).round() as u8)
    }

    fn redraw(&mut self, hwnd: HWND) {
        if let Err(error) = self.render(hwnd) {
            crate::log_warn!("could not draw companion chat: {error}");
        }
    }

    fn notify(&self, hwnd: HWND, action: usize) {
        unsafe {
            let _ = PostMessageW(Some(self.parent), WM_MINI_CHAT, WPARAM(action), LPARAM(hwnd.0 as isize));
        }
    }

    fn close(&mut self, hwnd: HWND) {
        self.closing = Some(Instant::now());
        self.sync_preview_timer(hwnd);
        unsafe {
            SetTimer(Some(hwnd), ANIMATE, 16, None);
        }
    }

    fn key(&mut self, hwnd: HWND, vk: u16) {
        let down = |key: VIRTUAL_KEY| unsafe { GetKeyState(key.0 as i32) < 0 };
        let (ctrl, shift) = (down(VK_CONTROL), down(VK_SHIFT));
        if vk == VK_ESCAPE.0 {
            self.notify(hwnd, VOICE_CANCEL);
            self.close(hwnd);
            return;
        }
        if vk == VK_TAB.0 {
            let mut order = vec![Hit::Input, Hit::Send, Hit::Voice, Hit::Pet, Hit::Setup, Hit::Close];
            if !self.ui.voice_enabled {
                order.retain(|hit| *hit != Hit::Voice);
            }
            if self.ui.chat.pet.enabled {
                order.extend([
                    Hit::PetAction(view::PetStudioHit::Body),
                    Hit::PetAction(view::PetStudioHit::Apply),
                    Hit::PetAction(view::PetStudioHit::Photo),
                    Hit::PetAction(view::PetStudioHit::TextOnly),
                ]);
            }
            let i = order.iter().position(|hit| *hit == self.ui.focus).unwrap_or(0);
            self.ui.focus = order[if shift { (i + order.len() - 1) % order.len() } else { (i + 1) % order.len() }];
            return;
        }
        if vk == VK_RETURN.0 && self.ui.focus != Hit::Input {
            match self.ui.focus {
                Hit::Send if self.can_send() => {
                    self.ui.focus = Hit::Input;
                    self.notify(hwnd, SEND);
                }
                Hit::Voice => self.notify(hwnd, VOICE),
                Hit::Setup => self.notify(hwnd, SETUP),
                Hit::Pet => self.notify(hwnd, PET),
                Hit::PetAction(action) => self.notify(hwnd, pet_action(action)),
                Hit::Close => self.close(hwnd),
                _ => {}
            }
            return;
        }
        if self.ui.focus == Hit::History {
            if ctrl && vk == b'A' as u16 {
                self.ui.chat.select_all();
            }
            if ctrl
                && vk == b'C' as u16
                && let Some(text) = self.ui.chat.selected_text()
            {
                clips::sys::write_text(hwnd, &text);
            }
            return;
        }
        if self.ui.focus != Hit::Input {
            return;
        }
        if vk == VK_RETURN.0 && !shift {
            if self.can_send() {
                self.notify(hwnd, SEND);
            }
            return;
        }
        let layout = self.input_layout();
        let input = &mut self.ui.chat.input;
        match vk {
            v if v == VK_LEFT.0 => input.left(shift, ctrl),
            v if v == VK_RIGHT.0 => input.right(shift, ctrl),
            v if v == VK_UP.0 || v == VK_DOWN.0 => {
                if let Some(l) = &layout {
                    input.vertical(l, if v == VK_UP.0 { -1 } else { 1 }, shift);
                }
            }
            v if v == VK_HOME.0 => {
                if ctrl {
                    input.caret_to_start(shift);
                } else if let Some(l) = &layout {
                    input.line_edge(l, false, shift);
                }
            }
            v if v == VK_END.0 => {
                if ctrl {
                    input.caret_to_end(shift);
                } else if let Some(l) = &layout {
                    input.line_edge(l, true, shift);
                }
            }
            v if v == VK_DELETE.0 => input.delete(ctrl),
            v if v == VK_BACK.0 => input.backspace(ctrl),
            v if ctrl && v == b'A' as u16 => input.select_all(),
            v if ctrl && (v == b'C' as u16 || v == b'X' as u16) => {
                if let Some(text) = input.selected_text() {
                    clips::sys::write_text(hwnd, &text);
                    if v == b'X' as u16 {
                        input.delete_selection();
                    }
                }
            }
            v if ctrl && v == b'V' as u16 => {
                if let Some(text) = clips::sys::read_text(hwnd) {
                    input.insert(&text.replace("\r\n", "\n"));
                }
            }
            _ => {}
        }
        self.keep_caret_visible();
        self.ui.caret_on = true;
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<State>;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        let Ok(mut state) = (*ptr).try_borrow_mut() else {
            return DefWindowProcW(hwnd, msg, wp, lp);
        };
        let point = || (lp.0 as i16 as f32, (lp.0 >> 16) as i16 as f32);
        match msg {
            WM_CLOSE => {
                state.close(hwnd);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                state.redraw(hwnd);
                LRESULT(0)
            }
            WM_SETFOCUS => {
                state.ui.active = true;
                state.ui.caret_on = true;
                SetTimer(Some(hwnd), CARET, 500, None);
                state.sync_preview_timer(hwnd);
                state.redraw(hwnd);
                LRESULT(0)
            }
            WM_KILLFOCUS => {
                state.ui.active = false;
                state.sync_preview_timer(hwnd);
                let _ = KillTimer(Some(hwnd), CARET);
                state.redraw(hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
                if state.drag.is_some() && matches!(wp.0, CARET | PREVIEW) {
                    return LRESULT(0);
                }
                if wp.0 == PREVIEW
                    && (!state.ui.active
                        || state.ui.chat.pet.preview_interval().is_none()
                        || !IsWindowVisible(hwnd).as_bool())
                {
                    state.sync_preview_timer(hwnd);
                    return LRESULT(0);
                }
                if wp.0 == CARET {
                    state.ui.caret_on = !state.ui.caret_on;
                }
                if wp.0 == ANIMATE {
                    if state.closing.is_some_and(|at| at.elapsed() >= Duration::from_millis(180)) {
                        state.closing = None;
                        state.opened = None;
                        let _ = KillTimer(Some(hwnd), ANIMATE);
                        let _ = KillTimer(Some(hwnd), CARET);
                        let _ = ShowWindow(hwnd, SW_HIDE);
                        return LRESULT(0);
                    }
                    if state.opened.is_some_and(|at| at.elapsed() >= Duration::from_millis(180)) {
                        state.opened = None;
                    }
                    if state.opened.is_none() && state.closing.is_none() {
                        let _ = KillTimer(Some(hwnd), ANIMATE);
                    }
                }
                state.redraw(hwnd);
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                let (x, y) = point();
                LRESULT(
                    if crate::ui::popup::hit_panel(
                        (x - rect.left as f32, y - rect.top as f32),
                        (state.size.0 as f32, state.size.1 as f32),
                        state.scale,
                    ) {
                        HTCLIENT as isize
                    } else {
                        HTTRANSPARENT as isize
                    },
                )
            }
            WM_LBUTTONDOWN => {
                let _ = SetFocus(Some(hwnd));
                state.ui.active = true;
                SetTimer(Some(hwnd), CARET, 500, None);
                let (x, y) = point();
                let hit = state.hit(x, y);
                state.pressed = hit;
                match hit {
                    Some(Hit::Header | Hit::Resize) => {
                        let mut cursor = POINT::default();
                        let mut rect = RECT::default();
                        if GetCursorPos(&mut cursor).is_ok() && GetWindowRect(hwnd, &mut rect).is_ok() {
                            state.drag = Some((cursor, rect, hit == Some(Hit::Resize)));
                            state.sync_preview_timer(hwnd);
                            SetCapture(hwnd);
                        }
                    }
                    Some(Hit::Input) => {
                        state.ui.focus = Hit::Input;
                        state.ui.chat.sel = None;
                        if let Some(layout) = state.input_layout() {
                            let r = state.layout().input;
                            let scroll = state.ui.chat.input.scroll;
                            state.ui.chat.input.click(
                                &layout,
                                x - r.0,
                                y - r.1 + scroll,
                                GetKeyState(VK_SHIFT.0 as i32) < 0,
                            );
                        }
                        state.selecting = true;
                        SetCapture(hwnd);
                    }
                    Some(Hit::History) => {
                        state.ui.focus = Hit::History;
                        state.ui.chat.sel = view::chat_model_text_at(&state.ui.chat, x, y).map(|at| (at, at));
                        state.reading = true;
                        SetCapture(hwnd);
                    }
                    Some(hit) => {
                        state.ui.focus = hit;
                        SetCapture(hwnd);
                    }
                    None => {}
                }
                state.ui.caret_on = true;
                state.redraw(hwnd);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let (x, y) = point();
                if let Some((start, rect, resize)) = state.drag {
                    let mut cursor = POINT::default();
                    if GetCursorPos(&mut cursor).is_ok() {
                        if resize {
                            let width = (rect.right - rect.left + cursor.x - start.x).max((360.0 * state.scale) as i32);
                            let height =
                                (rect.bottom - rect.top + cursor.y - start.y).max((360.0 * state.scale) as i32);
                            let _ = SetWindowPos(
                                hwnd,
                                None,
                                0,
                                0,
                                width,
                                height,
                                SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
                            );
                            state.redraw(hwnd);
                        } else {
                            let _ = SetWindowPos(
                                hwnd,
                                None,
                                rect.left + cursor.x - start.x,
                                rect.top + cursor.y - start.y,
                                0,
                                0,
                                SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                            );
                        }
                    }
                    return LRESULT(0);
                }
                if state.selecting
                    && let Some(layout) = state.input_layout()
                {
                    let r = state.layout().input;
                    let scroll = state.ui.chat.input.scroll;
                    state.ui.chat.input.click(&layout, x - r.0, y - r.1 + scroll, true);
                    state.keep_caret_visible();
                }
                if state.reading
                    && let Some(at) = view::chat_model_text_at(&state.ui.chat, x, y)
                    && let Some((anchor, _)) = state.ui.chat.sel
                {
                    state.ui.chat.sel = Some((anchor, at));
                }
                let hover = state.hit(x, y);
                if hover != state.ui.hover || state.selecting || state.reading || state.drag.is_some() {
                    state.ui.hover = hover;
                    state.redraw(hwnd);
                }
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut track);
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                state.ui.hover = None;
                state.redraw(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let pressed = state.pressed.take();
                let dragged = state.drag.take().is_some();
                state.selecting = false;
                state.reading = false;
                let _ = ReleaseCapture();
                state.sync_preview_timer(hwnd);
                let (x, y) = point();
                if !dragged && pressed == state.hit(x, y) {
                    match pressed {
                        Some(Hit::Close) => state.close(hwnd),
                        Some(Hit::Voice) => state.notify(hwnd, VOICE),
                        Some(Hit::Setup) => state.notify(hwnd, SETUP),
                        Some(Hit::Pet) => state.notify(hwnd, PET),
                        Some(Hit::PetAction(action)) => state.notify(hwnd, pet_action(action)),
                        Some(Hit::Send) if state.can_send() => {
                            state.ui.focus = Hit::Input;
                            state.notify(hwnd, SEND)
                        }
                        _ => {}
                    }
                }
                state.redraw(hwnd);
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                state.drag = None;
                state.selecting = false;
                state.reading = false;
                state.pressed = None;
                state.sync_preview_timer(hwnd);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = (wp.0 >> 16) as i16 as f32;
                state.ui.chat.hist =
                    (state.ui.chat.hist + delta / 120.0 * 48.0 * state.scale).clamp(0.0, state.ui.chat.max.get());
                state.redraw(hwnd);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                state.key(hwnd, wp.0 as u16);
                state.redraw(hwnd);
                LRESULT(0)
            }
            WM_CHAR => {
                let value = wp.0 as u16;
                if state.ui.focus == Hit::Input {
                    if (0xD800..=0xDBFF).contains(&value) {
                        state.surrogate = Some(value);
                    } else {
                        let text = if let Some(high) = state.surrogate.take() {
                            String::from_utf16_lossy(&[high, value])
                        } else {
                            String::from_utf16_lossy(&[value])
                        };
                        if value == 13 && GetKeyState(VK_SHIFT.0 as i32) < 0 {
                            state.ui.chat.input.insert("\n");
                        } else if value >= 32 {
                            state.ui.chat.input.insert(&text);
                        }
                        state.keep_caret_visible();
                        state.ui.caret_on = true;
                        state.redraw(hwnd);
                    }
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                let cursor = match state.ui.hover {
                    Some(Hit::Input | Hit::History) => IDC_IBEAM,
                    Some(Hit::Resize) => IDC_SIZENWSE,
                    Some(Hit::Send | Hit::Voice | Hit::Setup | Hit::Close) => IDC_HAND,
                    _ => IDC_ARROW,
                };
                SetCursor(LoadCursorW(None, cursor).ok());
                LRESULT(1)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

pub struct MiniChatWindow {
    hwnd: HWND,
    state: Box<RefCell<State>>,
}

impl MiniChatWindow {
    pub fn new(parent: HWND, scale: f32, gpu: &Gpu) -> Result<Self> {
        static CLASS: Once = Once::new();
        unsafe {
            let instance = GetModuleHandleW(None)?;
            CLASS.call_once(|| {
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(proc),
                    hInstance: instance.into(),
                    hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                    lpszClassName: w!("NightIsland.MiniChat"),
                    ..Default::default()
                });
            });
            let size = (
                (view::COMPANION_CHAT_SIZE.0 * scale).round() as i32,
                (view::COMPANION_CHAT_SIZE.1 * scale).round() as i32,
            );
            let surface = PopupSurface::new(size)?;
            let mut state = Box::new(RefCell::new(State {
                parent,
                gpu: gpu.popup(),
                surface,
                size,
                scale,
                ui: CompanionChatView::default(),
                pressed: None,
                drag: None,
                selecting: false,
                reading: false,
                surrogate: None,
                opened: None,
                closing: None,
            }));
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("NightIsland.MiniChat"),
                w!("AI"),
                WS_POPUP,
                0,
                0,
                size.0,
                size.1,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&raw mut *state) as isize);
            Ok(Self { hwnd, state })
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn show(&mut self, anchor: (f32, f32), character: Character) {
        let mut state = self.state.borrow_mut();
        state.ui.character = character;
        state.ui.active = true;
        unsafe {
            let _ = SetWindowTextW(self.hwnd, &HSTRING::from(format!("{} · AI", character.name())));
            if !IsWindowVisible(self.hwnd).as_bool() {
                let (width, height) = state.size;
                let monitor =
                    MonitorFromPoint(POINT { x: anchor.0 as i32, y: anchor.1 as i32 }, MONITOR_DEFAULTTONEAREST);
                let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
                if GetMonitorInfoW(monitor, &mut info).as_bool() {
                    let r = info.rcWork;
                    let gap = (40.0 * state.scale) as i32;
                    let x = if anchor.0 as i32 + gap + width <= r.right {
                        anchor.0 as i32 + gap
                    } else {
                        anchor.0 as i32 - gap - width
                    };
                    let x = x.clamp(r.left, (r.right - width).max(r.left));
                    let y = (anchor.1 as i32 - height / 2).clamp(r.top, (r.bottom - height).max(r.top));
                    let _ = SetWindowPos(self.hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                }
                state.opened = Some(Instant::now());
                SetTimer(Some(self.hwnd), ANIMATE, 16, None);
            }
            state.closing = None;
            state.redraw(self.hwnd);
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
            SetTimer(Some(self.hwnd), CARET, 500, None);
            state.sync_preview_timer(self.hwnd);
        }
    }

    pub fn input(&self) -> String {
        self.state.borrow().ui.chat.input.text()
    }
    pub fn clear_input(&mut self) {
        self.set_input("");
    }
    pub fn set_input(&mut self, text: &str) {
        let mut state = self.state.borrow_mut();
        state.ui.chat.input.set_text(text);
        state.keep_caret_visible();
        state.redraw(self.hwnd);
    }

    pub fn update(&mut self, turns: &[ChatTurn], error: Option<&str>, pending: bool, provider: &str) {
        let mut state = self.state.borrow_mut();
        let chat = &mut state.ui.chat;
        chat.turns = turns.to_vec();
        chat.error = error.map(str::to_string);
        chat.pending = pending;
        chat.rebuild();
        state.ui.provider = provider.to_string();
        state.redraw(self.hwnd);
    }

    pub fn update_pet(
        &mut self,
        studio: &view::PetStudio,
        current: &super::pet_design::PetDesign,
        character: Character,
    ) {
        let mut state = self.state.borrow_mut();
        if state.ui.chat.pet.enabled != studio.enabled {
            state.ui.chat.sel = None;
            state.ui.focus = Hit::Input;
        }
        state.ui.chat.pet = studio.clone();
        state.ui.custom_pet = current.clone();
        state.ui.character = character;
        state.sync_preview_timer(self.hwnd);
        state.redraw(self.hwnd);
    }

    pub fn set_capture_hidden(&self, hidden: bool) {
        unsafe {
            let _ = SetWindowDisplayAffinity(self.hwnd, if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
        }
    }

    pub fn set_voice_enabled(&mut self, enabled: bool) {
        let mut state = self.state.borrow_mut();
        state.ui.voice_enabled = enabled;
        if !enabled && state.ui.focus == Hit::Voice {
            state.ui.focus = Hit::Input;
        }
        state.redraw(self.hwnd);
    }

    pub fn repaint(&mut self) {
        self.state.borrow_mut().redraw(self.hwnd);
    }
}

impl Drop for MiniChatWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), CARET);
            let _ = KillTimer(Some(self.hwnd), ANIMATE);
            let _ = KillTimer(Some(self.hwnd), PREVIEW);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save(path: &std::path::Path, size: (u32, u32), bgra: &[u8]) {
        let rgba: Vec<u8> = bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), size.0, size.1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    }

    #[test]
    #[ignore = "Requires an interactive Windows desktop and an isolated Composition runtime"]
    fn island_chat_renders_and_preserves_drafts_without_native_controls() -> Result<()> {
        use windows::UI::Composition::Compositor;
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
        let settings = crate::data::settings::Settings::default();
        let (panel, accent) = crate::ui::theme::colors(&settings);
        crate::ui::theme::set(panel, accent);
        let parent = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Companion chat test"),
                WS_POPUP,
                0,
                0,
                100,
                40,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?
        };
        let mut chat = MiniChatWindow::new(parent, 1.0, &gpu)?;
        let draft = "Привет, Spark! 🐈\nВторая строка";
        chat.set_input(draft);
        let turns = [
            ChatTurn::text(false, "Помоги спланировать вечер"),
            ChatTurn::text(
                true,
                "Конечно. Вот **короткий план**:\n\n- Закончить важные дела\n- Включить любимую музыку\n- Оставить время для себя\n\nС чего начнём?",
            ),
        ];
        chat.update(&turns, None, false, "OpenAI · gpt-5");
        let long_answer = [ChatTurn::text(
            true,
            format!(
                "## Проверка Markdown\n\n**Жирный текст**, `код` и русский язык 🐈\n\n```rust\nlet value = 42;\n```\n\n{}",
                "Длинная строка с переносами и деталями. ".repeat(150)
            ),
        )];
        chat.update(&long_answer, None, false, "OpenAI · gpt-5");
        assert!(chat.state.borrow().ui.chat.max.get() > 0.0);
        chat.set_input(&"Строка черновика 🐈\n".repeat(20));
        assert!(chat.state.borrow().ui.chat.input.scroll > 0.0);
        chat.set_input(draft);
        chat.update(&turns, None, false, "OpenAI · gpt-5");
        unsafe {
            assert!(GetWindow(chat.hwnd(), GW_OWNER).is_err());
            assert_eq!(GetWindowLongW(chat.hwnd(), GWL_STYLE) as u32 & WS_CAPTION.0, 0);
            assert!(GetWindow(chat.hwnd(), GW_CHILD).is_err());
        }
        chat.show((600.0, 300.0), Character::Spark);
        assert!(unsafe { IsWindowVisible(chat.hwnd()) }.as_bool());
        assert!(!unsafe { IsWindowVisible(parent) }.as_bool());
        assert_eq!(chat.input(), draft);
        for (size, scale) in [((416, 460), 1.0), ((360, 360), 1.0), ((624, 690), 1.5)] {
            unsafe {
                let _ =
                    SetWindowPos(chat.hwnd(), None, 0, 0, size.0, size.1, SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER);
            }
            let mut state = chat.state.borrow_mut();
            state.scale = scale;
            state.render(chat.hwnd())?;
            let l = state.layout();
            assert!(l.header.3 < l.history.1);
            assert!(l.history.3 < l.composer.1);
            assert!(l.input.2 < l.send.0 && l.send.2 < l.composer.2);
            assert_eq!(state.surface.pixels()[3], 0);
        }
        unsafe {
            let _ = SetWindowPos(chat.hwnd(), None, 0, 0, 416, 460, SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER);
        }
        {
            let mut state = chat.state.borrow_mut();
            state.scale = 1.0;
            state.opened = None;
        }
        chat.set_input("");
        for ch in "Тест 🐈".encode_utf16() {
            proc(chat.hwnd(), WM_CHAR, WPARAM(ch as usize), LPARAM(0));
        }
        assert_eq!(chat.input(), "Тест 🐈");
        proc(chat.hwnd(), WM_KEYDOWN, WPARAM(VK_RETURN.0 as usize), LPARAM(0));
        let mut message = MSG::default();
        assert!(unsafe { PeekMessageW(&mut message, Some(parent), WM_MINI_CHAT, WM_MINI_CHAT, PM_REMOVE) }.as_bool());
        assert_eq!(message.wParam.0, SEND);
        chat.update(&turns, None, true, "OpenAI · gpt-5");
        proc(chat.hwnd(), WM_KEYDOWN, WPARAM(VK_RETURN.0 as usize), LPARAM(0));
        assert!(!unsafe { PeekMessageW(&mut message, Some(parent), WM_MINI_CHAT, WM_MINI_CHAT, PM_REMOVE) }.as_bool());
        proc(chat.hwnd(), WM_CLOSE, WPARAM(0), LPARAM(0));
        chat.state.borrow_mut().closing = Some(Instant::now() - Duration::from_millis(200));
        proc(chat.hwnd(), WM_TIMER, WPARAM(ANIMATE), LPARAM(0));
        assert!(!unsafe { IsWindowVisible(chat.hwnd()) }.as_bool());
        assert_eq!(chat.input(), "Тест 🐈");
        chat.show((600.0, 300.0), Character::Cat);
        assert_eq!(chat.input(), "Тест 🐈");
        chat.set_input("А музыку включишь?");
        chat.update(&turns, None, false, "OpenAI · gpt-5");
        if let Some(dir) = std::env::var_os("NI_COMPANION_PREVIEW") {
            let dir = std::path::PathBuf::from(dir);
            let empty = PopupSurface::new((416, 460))?;
            let empty_ui = CompanionChatView { provider: "OpenAI · gpt-5".into(), ..Default::default() };
            empty.paint(&gpu, |p| view::paint_companion_chat(p, (416.0, 460.0), 1.0, &empty_ui))?;
            save(&dir.join("island-chat-empty.png"), (416, 460), &empty.pixels());
            {
                let state = chat.state.borrow();
                save(&dir.join("island-chat.png"), (416, 460), &state.surface.pixels());
            }
            let combined = PopupSurface::new((750, 480))?;
            let state = chat.state.borrow();
            let media = crate::platform::media::Media {
                title: "Midnight City".into(),
                artist: "M83".into(),
                playing: true,
                ..Default::default()
            };
            combined.paint(&gpu, |p| {
                view::paint_companion_chat(p, (416.0, 460.0), 1.0, &state.ui)?;
                let old = p.push_offset(430.0, 50.0);
                let result = view::paint_companion_player(p, 1.0, &media, Some((84.0, 244.0)), None, None);
                p.set_transform(&old);
                result
            })?;
            save(&dir.join("island-companion-redesign.png"), (750, 480), &combined.pixels());
        }
        let design = crate::ui::pet_design::PetDesign {
            name: "Мишка".into(),
            accessory: crate::ui::pet_design::PetAccessory::Scarf,
            accent: [103, 156, 239],
            ..Default::default()
        };
        let reference_path =
            std::env::temp_dir().join(format!("night-island-pet-reference-{}.png", std::process::id()));
        let reference = PopupSurface::new((96, 96))?;
        reference.paint(&gpu, |p| {
            p.fill_rect((0.0, 0.0, 96.0, 96.0), crate::ui::gfx::rgba(0.3, 0.32, 0.4, 1.0))?;
            let old = p.push_offset(6.0, 15.0);
            let pose = crate::ui::characters::Pose {
                t: 0.8,
                reaction: crate::ui::characters::Reaction::Idle,
                look: (0.0, 0.0),
                blink: false,
                accent: (0.6, 0.7, 1.0),
                beat: 0.0,
                motion: Default::default(),
            };
            let result = crate::ui::characters::draw(p, Character::Bear, 84.0, &pose);
            p.set_transform(&old);
            result
        })?;
        save(&reference_path, (96, 96), &reference.pixels());
        let thumbnail = crate::platform::apps::thumbnail(&reference_path.to_string_lossy());
        let attachment = crate::services::attach::load(&reference_path);
        std::fs::remove_file(&reference_path).unwrap();
        let thumbnail = thumbnail.expect("A selected PNG reference must have a Windows thumbnail");
        assert!(thumbnail.side <= 96 && thumbnail.side > 0);
        assert_eq!(thumbnail.bgra.len(), thumbnail.side as usize * thumbnail.side as usize * 4);
        let mut studio = view::PetStudio {
            enabled: true,
            draft: Some(design.clone()),
            reference: Some(thumbnail),
            files: vec![attachment.unwrap()],
            performance: crate::ui::pet_motion::PetPerformance::Still,
            preview_started: Some(Instant::now() - Duration::from_secs(1)),
            ..Default::default()
        };
        let preview = PopupSurface::new((400, 250))?;
        preview.paint(&gpu, |p| view::paint_pet_studio(p, (8.0, 8.0, 392.0, 242.0), 1.0, &studio, None, None))?;
        let still = preview.pixels();
        studio.preview_started = Some(Instant::now() - Duration::from_secs(3));
        preview.paint(&gpu, |p| view::paint_pet_studio(p, (8.0, 8.0, 392.0, 242.0), 1.0, &studio, None, None))?;
        assert_eq!(still, preview.pixels(), "Reduced motion must also stop the studio portrait");
        studio.performance = crate::ui::pet_motion::PetPerformance::Balanced;
        preview.paint(&gpu, |p| view::paint_pet_studio(p, (8.0, 8.0, 392.0, 242.0), 1.0, &studio, None, None))?;
        assert_ne!(still, preview.pixels(), "The studio portrait should come alive when animation is enabled");
        studio.preview_started = None;
        chat.update_pet(&studio, &design, Character::Custom);
        chat.set_input("Добавь синий шарф");
        let (before, storage) = {
            let mut state = chat.state.borrow_mut();
            let mut cursor = POINT::default();
            let mut rect = RECT::default();
            unsafe {
                GetCursorPos(&mut cursor)?;
                GetWindowRect(chat.hwnd(), &mut rect)?;
            }
            state.drag = Some((cursor, rect, false));
            (state.surface.pixels(), state.surface.storage_id())
        };
        for _ in 0..20 {
            proc(chat.hwnd(), WM_MOUSEMOVE, WPARAM(0), LPARAM(0));
            proc(chat.hwnd(), WM_TIMER, WPARAM(PREVIEW), LPARAM(0));
        }
        assert_eq!(
            chat.state.borrow().surface.pixels(),
            before,
            "Dragging must reuse the existing portrait and text bitmap"
        );
        assert_eq!(chat.state.borrow().surface.storage_id(), storage);
        let old_size = chat.state.borrow().size;
        {
            let mut state = chat.state.borrow_mut();
            let mut cursor = POINT::default();
            let mut rect = RECT::default();
            unsafe {
                GetCursorPos(&mut cursor)?;
                GetWindowRect(chat.hwnd(), &mut rect)?;
            }
            rect.right += 20;
            state.drag = Some((cursor, rect, true));
        }
        proc(chat.hwnd(), WM_MOUSEMOVE, WPARAM(0), LPARAM(0));
        assert_eq!(chat.state.borrow().size.0, old_size.0 + 20, "Resizing must still redraw at the new dimensions");
        proc(chat.hwnd(), WM_CAPTURECHANGED, WPARAM(0), LPARAM(0));
        assert!(chat.state.borrow().drag.is_none());
        for (size, scale) in [((416, 460), 1.0), ((360, 360), 1.0), ((624, 690), 1.5)] {
            unsafe {
                let _ =
                    SetWindowPos(chat.hwnd(), None, 0, 0, size.0, size.1, SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER);
            }
            let mut state = chat.state.borrow_mut();
            state.scale = scale;
            state.render(chat.hwnd())?;
            let [apply, photo, text] = view::pet_studio_buttons(state.layout().history, scale);
            for (rect, expected) in [
                (apply, view::PetStudioHit::Apply),
                (photo, view::PetStudioHit::Photo),
                (text, view::PetStudioHit::TextOnly),
            ] {
                assert_eq!(state.hit((rect.0 + rect.2) / 2.0, (rect.1 + rect.3) / 2.0), Some(Hit::PetAction(expected)));
            }
            state.ui.focus = Hit::PetAction(view::PetStudioHit::Apply);
            state.key(chat.hwnd(), VK_RETURN.0);
            let mut message = MSG::default();
            assert!(
                unsafe { PeekMessageW(&mut message, Some(parent), WM_MINI_CHAT, WM_MINI_CHAT, PM_REMOVE) }.as_bool()
            );
            assert_eq!(message.wParam.0, PET_APPLY);
            if let Some(dir) = std::env::var_os("NI_COMPANION_PREVIEW") {
                save(
                    &std::path::PathBuf::from(dir).join(format!("pet-studio-{}x{}.png", size.0, size.1)),
                    (size.0 as u32, size.1 as u32),
                    &state.surface.pixels(),
                );
            }
        }
        if let Some(dir) = std::env::var_os("NI_COMPANION_PREVIEW") {
            let surface = PopupSurface::new((640, 410))?;
            let mut ui = view::UiState::new();
            ui.tab = view::Tab::Chat;
            ui.chat.keyed = true;
            ui.chat.pet = studio.clone();
            ui.chat.input.set_text("Добавь синий шарф");
            surface.paint(&gpu, |p| view::paint_expanded(p, (0.0, 0.0, 640.0, 410.0), 1.0, &ui, &[]))?;
            save(&std::path::PathBuf::from(dir).join("pet-studio-island.png"), (640, 410), &surface.pixels());
        }
        for (name, pending, applied, text_only, error) in [
            ("creating", true, false, false, None),
            ("applied", false, true, false, None),
            ("text-only", false, false, true, None),
            ("error", false, false, false, Some("This model did not accept the photo. Select Text only and describe your pet, or choose a vision model.".to_string())),
        ] {
            studio.pending = pending;
            studio.applied = applied;
            studio.text_only = text_only;
            studio.error = error;
            let mut ui = CompanionChatView::default();
            ui.chat.pet = studio.clone();
            let surface = PopupSurface::new((360, 360))?;
            surface.paint(&gpu, |p| view::paint_companion_chat(p, (360.0, 360.0), 1.0, &ui))?;
            if let Some(dir) = std::env::var_os("NI_COMPANION_PREVIEW") {
                save(
                    &std::path::PathBuf::from(dir).join(format!("pet-studio-{name}.png")),
                    (360, 360),
                    &surface.pixels(),
                );
            }
        }
        let hwnd = chat.hwnd();
        drop(chat);
        assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
        unsafe {
            DestroyWindow(parent)?;
        }
        Ok(())
    }
}
