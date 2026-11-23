use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PostMessageW,
    PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_QUIT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::data::keys::{ALT, CTRL, SHIFT, WIN};
use crate::tools::macro_script::{Button, Input, Scope};

pub const WM_TRIGGER: u32 = WM_APP + 133;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub mods: u32,
    pub input: Input,
    pub swallow: bool,
    pub scope: Scope,
}

static MAIN: AtomicIsize = AtomicIsize::new(0);
static THREAD: Mutex<Option<u32>> = Mutex::new(None);
static TRIGGERS: Mutex<Vec<Trigger>> = Mutex::new(Vec::new());
static HELD: Mutex<Vec<(Input, bool)>> = Mutex::new(Vec::new());

fn held(vk: i32) -> bool {
    let state = unsafe { GetAsyncKeyState(vk) };
    state < 0
}

fn modifiers_match(required: u32) -> bool {
    required == 0
        || ([(CTRL, 0x11), (ALT, 0x12), (SHIFT, 0x10), (WIN, 0x5B)]
            .iter()
            .all(|(bit, vk)| required & bit == 0 || held(*vk) || (*bit == WIN && held(0x5C))))
}

fn handle(input: Input, down: bool) -> bool {
    let Ok(mut pressed) = HELD.try_lock() else { return false };
    if !down {
        let at = pressed.iter().position(|(i, _)| *i == input);
        return at.map(|i| pressed.remove(i).1).unwrap_or(false);
    }
    if let Some((_, swallowed)) = pressed.iter().find(|(i, _)| *i == input) {
        return *swallowed;
    }
    let Ok(triggers) = TRIGGERS.try_lock() else { return false };
    let matching: Vec<(usize, &Trigger)> =
        triggers.iter().enumerate().filter(|(_, t)| t.input == input && modifiers_match(t.mods)).collect();
    if matching.is_empty() {
        return false;
    }
    let foreground = if matching.iter().any(|(_, t)| !t.scope.is_global()) {
        crate::platform::input_inject::foreground_info()
    } else {
        None
    };
    let scoped_here = |t: &Trigger| {
        !t.scope.is_global() && foreground.as_ref().is_some_and(|(exe, title)| t.scope.matches(exe, title))
    };
    let chosen = matching
        .iter()
        .find(|(_, t)| scoped_here(t))
        .or_else(|| matching.iter().find(|(_, t)| t.scope.is_global()))
        .copied();
    let Some((index, trigger)) = chosen else { return false };
    let swallow = trigger.swallow;
    drop(triggers);
    if pressed.len() < 32 {
        pressed.push((input, swallow));
    }
    let main = MAIN.load(Ordering::Relaxed);
    if main != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(main as *mut c_void)), WM_TRIGGER, WPARAM(index), LPARAM(0));
        }
    }
    swallow
}

fn watched(input: Input) -> bool {
    TRIGGERS.try_lock().is_ok_and(|t| t.iter().any(|t| t.input == input))
}

extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let message = wparam.0 as u32;
        let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let up = message == WM_KEYUP || message == WM_SYSKEYUP;
        if info.flags.0 & 0x10 == 0 && (down || up) {
            let input = Input::Key(info.vkCode as u16);
            if (watched(input) || !down) && handle(input, down) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if info.flags & 1 == 0 {
            let x_button = if (info.mouseData >> 16) & 0xFFFF == 2 { Button::X2 } else { Button::X1 };
            let event = match wparam.0 as u32 {
                WM_LBUTTONDOWN => Some((Button::Left, true)),
                WM_LBUTTONUP => Some((Button::Left, false)),
                WM_RBUTTONDOWN => Some((Button::Right, true)),
                WM_RBUTTONUP => Some((Button::Right, false)),
                WM_MBUTTONDOWN => Some((Button::Middle, true)),
                WM_MBUTTONUP => Some((Button::Middle, false)),
                WM_XBUTTONDOWN => Some((x_button, true)),
                WM_XBUTTONUP => Some((x_button, false)),
                _ => None,
            };
            if let Some((button, down)) = event {
                let input = Input::Mouse(button);
                if (watched(input) || !down) && handle(input, down) {
                    return LRESULT(1);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn start(main: HWND) {
    let Ok(mut thread) = THREAD.lock() else { return };
    if thread.is_some() {
        return;
    }
    MAIN.store(main.0 as isize, Ordering::Relaxed);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0).ok();
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None, 0).ok();
        let (Some(keyboard), Some(mouse)) = (keyboard, mouse) else {
            crate::log_warn!("could not install the macro trigger hooks");
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
}

fn stop() {
    let Ok(mut thread) = THREAD.lock() else { return };
    if let Some(id) = thread.take() {
        unsafe {
            let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    if let Ok(mut pressed) = HELD.lock() {
        pressed.clear();
    }
}

pub fn configure(main: HWND, triggers: Vec<Trigger>) {
    let empty = triggers.is_empty();
    if let Ok(mut slot) = TRIGGERS.lock() {
        *slot = triggers;
    }
    if empty {
        stop();
    } else {
        start(main);
    }
}
