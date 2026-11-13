use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PostMessageW,
    PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::tools::macro_script::{Button, MAX_STEPS, Step};

pub const WM_RECORD_STOP: u32 = WM_APP + 131;

const MAX_EVENTS: usize = 60_000;
const MIN_MOVE_PX: i32 = 4;
const MOVE_SPACING: Duration = Duration::from_millis(25);
const MIN_DELAY: Duration = Duration::from_millis(10);
const STOP_KEY: u32 = 0x7B;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Key { vk: u16, down: bool },
    Button { button: Button, down: bool },
    Move,
    Wheel(i32),
    PadButton { mask: u16, down: bool },
    PadStick { right: bool, x: i8, y: i8 },
    PadTrigger { right: bool, value: u8 },
}

#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub kind: Kind,
    pub at: Instant,
    pub x: i32,
    pub y: i32,
}

static MAIN: AtomicIsize = AtomicIsize::new(0);
static THREAD: Mutex<Option<u32>> = Mutex::new(None);
static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());
static POLLING: AtomicBool = AtomicBool::new(false);

const STICK_DEADZONE: i32 = 7849;
const PAD_CHANGE_PERCENT: i32 = 4;

fn stick_percent(value: i16) -> i8 {
    let v = value as i32;
    if v.abs() < STICK_DEADZONE {
        return 0;
    }
    (v * 100 / 32767).clamp(-100, 100) as i8
}

fn poll_pad() {
    use windows::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};
    let mut last_buttons = 0u16;
    let mut last_sticks = [0i8; 4];
    let mut last_triggers = [0u8; 2];
    while POLLING.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(8));
        let mut state = XINPUT_STATE::default();
        let connected = (0..4).any(|user| unsafe { XInputGetState(user, &mut state) } == 0);
        if !connected {
            continue;
        }
        let pad = state.Gamepad;
        let buttons = pad.wButtons.0;
        let changed = buttons ^ last_buttons;
        if changed != 0 {
            for bit in 0..16 {
                let mask = 1u16 << bit;
                if changed & mask != 0 && crate::tools::macro_script::PAD_BUTTONS.iter().any(|(_, m)| *m == mask) {
                    push(Kind::PadButton { mask, down: buttons & mask != 0 }, 0, 0);
                }
            }
            last_buttons = buttons;
        }
        let sticks = [
            stick_percent(pad.sThumbLX),
            stick_percent(pad.sThumbLY),
            stick_percent(pad.sThumbRX),
            stick_percent(pad.sThumbRY),
        ];
        for (side, right) in [(0usize, false), (2, true)] {
            let moved = (sticks[side] as i32 - last_sticks[side] as i32).abs() >= PAD_CHANGE_PERCENT
                || (sticks[side + 1] as i32 - last_sticks[side + 1] as i32).abs() >= PAD_CHANGE_PERCENT
                || ((sticks[side] == 0 && sticks[side + 1] == 0)
                    && (last_sticks[side] != 0 || last_sticks[side + 1] != 0));
            if moved {
                push(Kind::PadStick { right, x: sticks[side], y: sticks[side + 1] }, 0, 0);
                last_sticks[side] = sticks[side];
                last_sticks[side + 1] = sticks[side + 1];
            }
        }
        let triggers =
            [pad.bLeftTrigger, pad.bRightTrigger].map(|t| if t < 30 { 0 } else { (t as u32 * 100 / 255) as u8 });
        for (i, right) in [(0usize, false), (1, true)] {
            if (triggers[i] as i32 - last_triggers[i] as i32).abs() >= PAD_CHANGE_PERCENT
                || (triggers[i] == 0 && last_triggers[i] != 0)
            {
                push(Kind::PadTrigger { right, value: triggers[i] }, 0, 0);
                last_triggers[i] = triggers[i];
            }
        }
    }
}

fn push(kind: Kind, x: i32, y: i32) {
    if let Ok(mut events) = EVENTS.lock()
        && events.len() < MAX_EVENTS
    {
        if matches!(kind, Kind::Move)
            && let Some(last) = events.last()
            && matches!(last.kind, Kind::Move)
            && (last.x - x).abs().max((last.y - y).abs()) < MIN_MOVE_PX
        {
            return;
        }
        events.push(Event { kind, at: Instant::now(), x, y });
    }
}

fn held(vk: i32) -> bool {
    let state = unsafe { GetAsyncKeyState(vk) };
    state < 0
}

extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let message = wparam.0 as u32;
        let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let up = message == WM_KEYUP || message == WM_SYSKEYUP;
        if info.flags.0 & 0x10 == 0 && (down || up) {
            if info.vkCode == STOP_KEY && held(0x11) && held(0x12) {
                if down {
                    let main = MAIN.load(Ordering::Relaxed);
                    if main != 0 {
                        unsafe {
                            let _ = PostMessageW(Some(HWND(main as *mut c_void)), WM_RECORD_STOP, WPARAM(0), LPARAM(0));
                        }
                    }
                }
                return LRESULT(1);
            }
            push(Kind::Key { vk: info.vkCode as u16, down }, 0, 0);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if info.flags & 1 == 0 {
            let (x, y) = (info.pt.x, info.pt.y);
            let x_button = if (info.mouseData >> 16) & 0xFFFF == 2 { Button::X2 } else { Button::X1 };
            match wparam.0 as u32 {
                WM_MOUSEMOVE => push(Kind::Move, x, y),
                WM_LBUTTONDOWN => push(Kind::Button { button: Button::Left, down: true }, x, y),
                WM_LBUTTONUP => push(Kind::Button { button: Button::Left, down: false }, x, y),
                WM_RBUTTONDOWN => push(Kind::Button { button: Button::Right, down: true }, x, y),
                WM_RBUTTONUP => push(Kind::Button { button: Button::Right, down: false }, x, y),
                WM_MBUTTONDOWN => push(Kind::Button { button: Button::Middle, down: true }, x, y),
                WM_MBUTTONUP => push(Kind::Button { button: Button::Middle, down: false }, x, y),
                WM_XBUTTONDOWN => push(Kind::Button { button: x_button, down: true }, x, y),
                WM_XBUTTONUP => push(Kind::Button { button: x_button, down: false }, x, y),
                WM_MOUSEWHEEL => push(Kind::Wheel(((info.mouseData >> 16) as u16 as i16) as i32), x, y),
                _ => {}
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub fn start(main: HWND) -> bool {
    let Ok(mut thread) = THREAD.lock() else { return false };
    if thread.is_some() {
        return false;
    }
    MAIN.store(main.0 as isize, Ordering::Relaxed);
    if let Ok(mut events) = EVENTS.lock() {
        events.clear();
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0).ok();
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None, 0).ok();
        let (Some(keyboard), Some(mouse)) = (keyboard, mouse) else {
            crate::log_warn!("could not install the recording hooks");
            if let Some(h) = keyboard {
                let _ = UnhookWindowsHookEx(h);
            }
            if let Some(h) = mouse {
                let _ = UnhookWindowsHookEx(h);
            }
            let _ = tx.send(None);
            return;
        };
        let _ = tx.send(Some(GetCurrentThreadId()));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(keyboard);
        let _ = UnhookWindowsHookEx(mouse);
    });
    *thread = rx.recv().ok().flatten();
    if thread.is_some() {
        POLLING.store(true, Ordering::Relaxed);
        std::thread::spawn(poll_pad);
    }
    thread.is_some()
}

pub fn stop(trim_click: bool) -> Vec<Step> {
    POLLING.store(false, Ordering::Relaxed);
    if let Ok(mut thread) = THREAD.lock()
        && let Some(id) = thread.take()
    {
        unsafe {
            let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    let mut events = EVENTS.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default();
    if trim_click && let Some(end) = events.last().map(|e| e.at) {
        while events.last().is_some_and(|e| {
            matches!(e.kind, Kind::Button { .. } | Kind::Move) && end - e.at < Duration::from_millis(600)
        }) {
            events.pop();
        }
    }
    while events.last().is_some_and(|e| matches!(e.kind, Kind::Key { vk: 0x11 | 0x12 | 0xA2 | 0xA3 | 0xA4 | 0xA5, .. }))
    {
        events.pop();
    }
    to_steps(&events)
}

pub fn to_steps(events: &[Event]) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    let Some(first) = events.first() else { return steps };
    let mut clock = first.at;
    let mut position: Option<(i32, i32)> = None;
    let mut pending: Option<Event> = None;
    let mut last_move: Option<Instant> = None;

    fn emit(steps: &mut Vec<Step>, clock: &mut Instant, at: Instant, step: Step) {
        let gap = at.saturating_duration_since(*clock);
        if gap >= MIN_DELAY {
            steps.push(Step::Delay(gap.as_millis().min(3_600_000) as u32, gap.as_millis().min(3_600_000) as u32));
        }
        *clock = at;
        steps.push(step);
    }

    for event in events {
        if steps.len() >= MAX_STEPS - 2 {
            break;
        }
        match event.kind {
            Kind::Move => {
                if last_move.is_some_and(|t| event.at.saturating_duration_since(t) < MOVE_SPACING) {
                    pending = Some(*event);
                    continue;
                }
                pending = None;
                last_move = Some(event.at);
                position = Some((event.x, event.y));
                emit(&mut steps, &mut clock, event.at, Step::Move { x: event.x, y: event.y, relative: false });
            }
            other => {
                if let Some(p) = pending.take() {
                    position = Some((p.x, p.y));
                    emit(&mut steps, &mut clock, p.at, Step::Move { x: p.x, y: p.y, relative: false });
                }
                match other {
                    Kind::Key { vk, down } => {
                        let merged = !down
                            && matches!(steps.last(), Some(Step::Down(last)) if *last == vk)
                            && event.at.saturating_duration_since(clock) < MIN_DELAY;
                        if merged {
                            steps.pop();
                            steps.push(Step::Tap(vec![vk]));
                            clock = event.at;
                        } else {
                            let step = if down { Step::Down(vk) } else { Step::Up(vk) };
                            emit(&mut steps, &mut clock, event.at, step);
                        }
                    }
                    Kind::Button { button, down } => {
                        if position != Some((event.x, event.y)) {
                            position = Some((event.x, event.y));
                            emit(
                                &mut steps,
                                &mut clock,
                                event.at,
                                Step::Move { x: event.x, y: event.y, relative: false },
                            );
                        }
                        let merged = !down
                            && matches!(steps.last(), Some(Step::MouseDown(last)) if *last == button)
                            && event.at.saturating_duration_since(clock) < Duration::from_millis(400);
                        if merged {
                            steps.pop();
                            steps.push(Step::Click { button, at: None });
                            clock = event.at;
                        } else {
                            let step = if down { Step::MouseDown(button) } else { Step::MouseUp(button) };
                            emit(&mut steps, &mut clock, event.at, step);
                        }
                    }
                    Kind::Wheel(delta) => emit(&mut steps, &mut clock, event.at, Step::Wheel(delta)),
                    Kind::PadButton { mask, down } => {
                        let merged = !down
                            && matches!(steps.last(), Some(Step::PadDown(last)) if *last == mask)
                            && event.at.saturating_duration_since(clock) < Duration::from_millis(400);
                        if merged {
                            steps.pop();
                            steps.push(Step::PadTap(mask));
                            clock = event.at;
                        } else {
                            let step = if down { Step::PadDown(mask) } else { Step::PadUp(mask) };
                            emit(&mut steps, &mut clock, event.at, step);
                        }
                    }
                    Kind::PadStick { right, x, y } => {
                        emit(&mut steps, &mut clock, event.at, Step::PadStick { right, x, y })
                    }
                    Kind::PadTrigger { right, value } => {
                        emit(&mut steps, &mut clock, event.at, Step::PadTrigger { right, value })
                    }
                    Kind::Move => {}
                }
            }
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn taps_clicks_and_delays_are_merged() {
        let t = Instant::now();
        let events = [
            Event { kind: Kind::Key { vk: 0x41, down: true }, at: at(t, 0), x: 0, y: 0 },
            Event { kind: Kind::Key { vk: 0x41, down: false }, at: at(t, 5), x: 0, y: 0 },
            Event { kind: Kind::Button { button: Button::Left, down: true }, at: at(t, 505), x: 10, y: 20 },
            Event { kind: Kind::Button { button: Button::Left, down: false }, at: at(t, 540), x: 10, y: 20 },
        ];
        let steps = to_steps(&events);
        assert_eq!(steps[0], Step::Tap(vec![0x41]));
        assert!(matches!(steps[1], Step::Delay(..)));
        assert_eq!(steps[2], Step::Move { x: 10, y: 20, relative: false });
        assert_eq!(steps[3], Step::Click { button: Button::Left, at: None });
    }

    #[test]
    fn dense_movement_is_thinned() {
        let t = Instant::now();
        let events: Vec<Event> =
            (0..100).map(|i| Event { kind: Kind::Move, at: at(t, i * 5), x: i as i32 * 5, y: 0 }).collect();
        assert!(to_steps(&events).len() < 60);
    }
}
