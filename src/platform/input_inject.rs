use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, MapVirtualKeyW, SendInput, keybd_event, mouse_event,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetSystemMetrics, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, PostMessageW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, WM_CHAR, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};
use windows::core::BOOL;

use crate::tools::macro_script::{Button, Method};

pub struct Injector {
    method: Method,
    target: Option<HWND>,
    target_title: String,
}

fn extended(vk: u16) -> bool {
    matches!(vk, 0x21..=0x28 | 0x2C | 0x2D | 0x2E | 0x5B..=0x5D | 0x6F | 0x90 | 0xA3 | 0xA5)
}

fn scan(vk: u16) -> u16 {
    unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) as u16 }
}

fn send(inputs: &[INPUT]) {
    if !inputs.is_empty() {
        unsafe {
            SendInput(inputs, size_of::<INPUT>() as i32);
        }
    }
}

fn mouse_input(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn key_input(vk: u16, scan_code: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                wScan: scan_code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

struct Search<'a> {
    needle: String,
    found: Option<HWND>,
    own_pid: u32,
    _marker: std::marker::PhantomData<&'a ()>,
}

unsafe extern "system" fn each_window(hwnd: HWND, data: LPARAM) -> BOOL {
    let search = unsafe { &mut *(data.0 as *mut Search) };
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return BOOL(1);
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == search.own_pid {
        return BOOL(1);
    }
    let mut title = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, &mut title) } as usize;
    if n > 0 && String::from_utf16_lossy(&title[..n]).to_lowercase().contains(&search.needle) {
        search.found = Some(hwnd);
        return BOOL(0);
    }
    BOOL(1)
}

pub fn find_window(title: &str) -> Option<HWND> {
    let needle = title.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let mut search = Search { needle, found: None, own_pid: std::process::id(), _marker: std::marker::PhantomData };
    unsafe {
        let _ = EnumWindows(Some(each_window), LPARAM((&raw mut search) as isize));
    }
    search.found
}

pub fn foreground_title_contains(title: &str) -> bool {
    let needle = title.trim().to_lowercase();
    let hwnd = unsafe { GetForegroundWindow() };
    if needle.is_empty() || hwnd.0.is_null() {
        return false;
    }
    let mut buffer = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..n]).to_lowercase().contains(&needle)
}

impl Injector {
    pub fn new(method: Method, target_title: &str) -> Self {
        Self { method, target: None, target_title: target_title.trim().to_string() }
    }

    fn post_window(&mut self) -> Option<HWND> {
        if !self.target_title.is_empty() {
            if self.target.is_none_or(|h| !unsafe { IsWindowVisible(h) }.as_bool()) {
                self.target = find_window(&self.target_title);
            }
            return self.target;
        }
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return None;
        }
        let mut info = GUITHREADINFO { cbSize: size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        let thread = unsafe { GetWindowThreadProcessId(foreground, None) };
        if unsafe { GetGUIThreadInfo(thread, &mut info) }.is_ok() && !info.hwndFocus.0.is_null() {
            return Some(info.hwndFocus);
        }
        Some(foreground)
    }

    pub fn key(&mut self, vk: u16, down: bool) {
        let scan_code = scan(vk);
        let ext = extended(vk);
        match self.method {
            Method::Scan if scan_code != 0 => {
                let mut flags = KEYEVENTF_SCANCODE;
                if ext {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                send(&[key_input(0, scan_code, flags)]);
            }
            Method::Scan | Method::Vk => {
                let mut flags = KEYBD_EVENT_FLAGS(0);
                if ext {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                send(&[key_input(vk, scan_code, flags)]);
            }
            Method::Legacy => {
                let mut flags = KEYBD_EVENT_FLAGS(0);
                if ext {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                unsafe { keybd_event(vk as u8, scan_code as u8, flags, 0) };
            }
            Method::Post => {
                let Some(window) = self.post_window() else { return };
                let alt = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x12) } < 0;
                let mut lparam = 1isize | ((scan_code as isize) << 16);
                if ext {
                    lparam |= 1 << 24;
                }
                let message = if down {
                    if alt { WM_SYSKEYDOWN } else { WM_KEYDOWN }
                } else {
                    lparam |= (1 << 30) | (1 << 31);
                    if alt { WM_SYSKEYUP } else { WM_KEYUP }
                };
                unsafe {
                    let _ = PostMessageW(Some(window), message, WPARAM(vk as usize), LPARAM(lparam));
                }
            }
        }
    }

    pub fn text(&mut self, text: &str) {
        if self.method == Method::Post {
            let Some(window) = self.post_window() else { return };
            for unit in text.encode_utf16() {
                unsafe {
                    let _ = PostMessageW(Some(window), WM_CHAR, WPARAM(unit as usize), LPARAM(1));
                }
            }
            return;
        }
        let mut inputs = Vec::new();
        for unit in text.encode_utf16() {
            if unit == 0x0A {
                inputs.push(key_input(0x0D, scan(0x0D), KEYBD_EVENT_FLAGS(0)));
                inputs.push(key_input(0x0D, scan(0x0D), KEYEVENTF_KEYUP));
                continue;
            }
            inputs.push(key_input(0, unit, KEYEVENTF_UNICODE));
            inputs.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
        for chunk in inputs.chunks(200) {
            send(chunk);
        }
    }

    pub fn move_mouse(&mut self, x: i32, y: i32, relative: bool) {
        if relative {
            self.mouse(MOUSEEVENTF_MOVE, x, y, 0);
            return;
        }
        let (left, top, width, height) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2),
                GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2),
            )
        };
        let norm = |v: i32, origin: i32, size: i32| {
            (((v - origin).clamp(0, size - 1) as i64 * 65535) / (size - 1) as i64) as i32
        };
        self.mouse(
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            norm(x, left, width),
            norm(y, top, height),
            0,
        );
    }

    pub fn button(&mut self, button: Button, down: bool) {
        let (flags, data) = match (button, down) {
            (Button::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
            (Button::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
            (Button::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
            (Button::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
            (Button::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
            (Button::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
            (Button::X1, true) => (MOUSEEVENTF_XDOWN, 1),
            (Button::X1, false) => (MOUSEEVENTF_XUP, 1),
            (Button::X2, true) => (MOUSEEVENTF_XDOWN, 2),
            (Button::X2, false) => (MOUSEEVENTF_XUP, 2),
        };
        self.mouse(flags, 0, 0, data);
    }

    pub fn wheel(&mut self, delta: i32) {
        self.mouse(MOUSEEVENTF_WHEEL, 0, 0, delta as u32);
    }

    fn mouse(&mut self, flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: u32) {
        if self.method == Method::Legacy {
            unsafe { mouse_event(flags, dx, dy, data as i32, 0) };
        } else {
            send(&[mouse_input(flags, dx, dy, data)]);
        }
    }
}
