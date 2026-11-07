use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use windows::core::BOOL;

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject, GetWindowRgn, HRGN, SetWindowRgn};
use windows::Win32::UI::Shell::{ABM_GETSTATE, ABM_SETSTATE, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage};
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct TaskbarGuard(Child);

impl TaskbarGuard {
    pub fn running(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }

    pub fn start() -> std::io::Result<Self> {
        Command::new(std::env::current_exe()?)
            .arg("--island-taskbar-guard")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .map(Self)
    }
}

impl Drop for TaskbarGuard {
    fn drop(&mut self) {
        self.0.stdin.take();
    }
}

fn taskbars() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> BOOL {
        let mut class = [0u16; 80];
        let len = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
        let name = String::from_utf16_lossy(&class[..len]);
        if matches!(name.as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            unsafe {
                (*(param.0 as *mut Vec<HWND>)).push(hwnd);
            }
        }
        BOOL(1)
    }
    let mut result = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM((&raw mut result) as isize));
    }
    result
}

struct Region(Option<HRGN>);
impl Region {
    fn capture(hwnd: HWND) -> Self {
        unsafe {
            let region = CreateRectRgn(0, 0, 0, 0);
            if region.0.is_null() {
                return Self(None);
            }
            if GetWindowRgn(hwnd, region).0 != 0 {
                return Self(Some(region));
            }
            let _ = DeleteObject(region.into());
            Self(None)
        }
    }
}
impl Drop for Region {
    fn drop(&mut self) {
        if let Some(region) = self.0.take() {
            unsafe {
                let _ = DeleteObject(region.into());
            }
        }
    }
}

struct HiddenTaskbars {
    state: Option<usize>,
    owned_state: usize,
    hidden: Vec<(HWND, u32, RECT, Region)>,
}

impl HiddenTaskbars {
    fn new() -> Self {
        Self { state: None, owned_state: 0, hidden: Vec::new() }
    }

    fn claim_state(&mut self) {
        let mut data = APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, ..Default::default() };
        let current = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) };
        let original = *self.state.get_or_insert(current);
        self.owned_state = original | ABS_AUTOHIDE as usize;
        if current != self.owned_state {
            data.lParam = LPARAM(self.owned_state as isize);
            unsafe {
                SHAppBarMessage(ABM_SETSTATE, &mut data);
            }
        }
    }

    fn refresh(&mut self) {
        self.hidden.retain(|(hwnd, pid, _, _)| unsafe {
            let mut current = 0;
            GetWindowThreadProcessId(*hwnd, Some(&mut current));
            IsWindow(Some(*hwnd)).as_bool() && current == *pid
        });
        let windows = taskbars();
        if !windows.is_empty() {
            self.claim_state();
        }
        for hwnd in windows {
            unsafe {
                if !GetPropW(hwnd, windows::core::w!("NightIsland.TrayAccess")).0.is_null() {
                    continue;
                }
                if IsWindowVisible(hwnd).as_bool() {
                    if !self.hidden.iter().any(|(old, _, _, _)| *old == hwnd) {
                        let mut pid = 0;
                        GetWindowThreadProcessId(hwnd, Some(&mut pid));
                        let mut rect = RECT::default();
                        let _ = GetWindowRect(hwnd, &mut rect);
                        self.hidden.push((hwnd, pid, rect, Region::capture(hwnd)));
                    }
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
        }
    }
}

impl Drop for HiddenTaskbars {
    fn drop(&mut self) {
        unsafe {
            for (hwnd, pid, rect, region) in &mut self.hidden {
                let mut current = 0;
                GetWindowThreadProcessId(*hwnd, Some(&mut current));
                if current == *pid && IsWindow(Some(*hwnd)).as_bool() {
                    if !GetPropW(*hwnd, windows::core::w!("NightIsland.TrayAccess")).0.is_null() {
                        let _ = RemovePropW(*hwnd, windows::core::w!("NightIsland.TrayAccess"));
                        if SetWindowRgn(*hwnd, region.0, false) != 0 {
                            region.0 = None;
                        }
                        let _ = SetWindowPos(
                            *hwnd,
                            None,
                            rect.left,
                            rect.top,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                    let _ = ShowWindow(*hwnd, SW_SHOWNOACTIVATE);
                }
            }
            let mut data = APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, ..Default::default() };
            if let Some(state) = self.state
                && SHAppBarMessage(ABM_GETSTATE, &mut data) == self.owned_state
            {
                data.lParam = LPARAM(state as isize);
                SHAppBarMessage(ABM_SETSTATE, &mut data);
            }
        }
    }
}

pub fn handle_cli() -> bool {
    if !std::env::args().any(|arg| arg == "--island-taskbar-guard") {
        return false;
    }
    use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
    let Ok(mutex) = (unsafe { CreateMutexW(None, false, windows::core::w!("Local\\NightIsland.TaskbarGuard")) }) else {
        return true;
    };
    let acquired = unsafe { WaitForSingleObject(mutex, 5000) };
    if acquired != WAIT_OBJECT_0 && acquired != WAIT_ABANDONED {
        unsafe {
            let _ = CloseHandle(mutex);
        }
        return true;
    }
    struct Mutex(HANDLE);
    impl Drop for Mutex {
        fn drop(&mut self) {
            unsafe {
                let _ = ReleaseMutex(self.0);
                let _ = CloseHandle(self.0);
            }
        }
    }
    let _mutex = Mutex(mutex);
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    std::thread::spawn(move || {
        let _ = std::io::stdin().read(&mut [0u8; 1]);
        signal.store(true, Ordering::Release);
    });
    let mut taskbars = HiddenTaskbars::new();
    while !stop.load(Ordering::Acquire) {
        taskbars.refresh();
        std::thread::sleep(Duration::from_millis(150));
    }
    super::system_tray::inline::recover();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Runs the built taskbar guardian and checks recovery after its input pipe closes"]
    fn native_taskbar_guard_recovers_after_parent_disconnect() -> std::io::Result<()> {
        let executable =
            std::env::var_os("NI_TASKBAR_HELPER").expect("NI_TASKBAR_HELPER must name the built Island executable");
        let visible: Vec<_> =
            taskbars().into_iter().filter(|hwnd| unsafe { IsWindowVisible(*hwnd).as_bool() }).collect();
        let mut data = APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, ..Default::default() };
        let state = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) };
        let mut child = Command::new(executable)
            .arg("--island-taskbar-guard")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while visible.iter().any(|hwnd| unsafe { IsWindowVisible(*hwnd).as_bool() }) {
            assert!(std::time::Instant::now() < deadline, "Taskbar guardian did not start");
            std::thread::sleep(Duration::from_millis(20));
        }
        child.stdin.take();
        while child.try_wait()?.is_none() {
            assert!(std::time::Instant::now() < deadline, "Taskbar guardian did not restore the desktop");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(visible.iter().all(|hwnd| unsafe { IsWindowVisible(*hwnd).as_bool() }));
        assert_eq!(unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) }, state);
        Ok(())
    }

    #[test]
    #[ignore = "Temporarily hides Explorer taskbars and verifies restoration"]
    fn native_taskbar_replacement_restores_state_and_visibility() {
        let initial = taskbars();
        assert!(!initial.is_empty());
        let visible: Vec<_> = initial.into_iter().filter(|hwnd| unsafe { IsWindowVisible(*hwnd).as_bool() }).collect();
        let mut data = APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, ..Default::default() };
        let state = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) };
        {
            let mut guard = HiddenTaskbars::new();
            assert!(guard.state.is_none());
            assert_eq!(unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) }, state);
            guard.refresh();
            assert!(visible.iter().all(|hwnd| unsafe { !IsWindowVisible(*hwnd).as_bool() }));
            assert_eq!(unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) }, state | ABS_AUTOHIDE as usize);
            data.lParam = LPARAM(state as isize);
            unsafe {
                SHAppBarMessage(ABM_SETSTATE, &mut data);
            }
            for hwnd in &visible {
                unsafe {
                    let _ = ShowWindow(*hwnd, SW_SHOWNOACTIVATE);
                }
            }
            guard.refresh();
            assert_eq!(guard.state, Some(state));
            assert!(visible.iter().all(|hwnd| unsafe { !IsWindowVisible(*hwnd).as_bool() }));
            assert_eq!(unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) }, state | ABS_AUTOHIDE as usize);
        }
        assert!(visible.iter().all(|hwnd| unsafe { IsWindowVisible(*hwnd).as_bool() }));
        assert_eq!(unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) }, state);
    }

    #[test]
    #[ignore = "Opens the real Explorer tray above a hidden taskbar, then restores the desktop"]
    fn native_hidden_taskbar_keeps_tray_actions_available() -> windows::core::Result<()> {
        use windows::core::w;
        struct Restore(HWND);
        impl Drop for Restore {
            fn drop(&mut self) {
                if crate::platform::system_tray::is_open() {
                    let _ = crate::platform::system_tray::show((0, 0));
                }
                unsafe {
                    for class in [w!("TopLevelWindowForOverflowXamlIsland"), w!("NotifyIconOverflowWindow")] {
                        if let Ok(hwnd) = FindWindowW(class, None) {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        }
                    }
                    let _ = SetForegroundWindow(self.0);
                }
            }
        }
        assert!(!crate::platform::system_tray::is_open());
        let _restore = Restore(unsafe { GetForegroundWindow() });
        let mut guard = HiddenTaskbars::new();
        guard.refresh();
        crate::platform::system_tray::show((1200, 56))?;
        assert!(crate::platform::system_tray::is_open());
        let hwnd = unsafe {
            FindWindowW(w!("TopLevelWindowForOverflowXamlIsland"), None)
                .or_else(|_| FindWindowW(w!("NotifyIconOverflowWindow"), None))?
        };
        let mut rect = windows::Win32::Foundation::RECT::default();
        unsafe {
            GetWindowRect(hwnd, &mut rect)?;
        }
        assert_eq!(rect.top, 56);
        guard.refresh();
        std::thread::sleep(Duration::from_millis(400));
        assert!(crate::platform::system_tray::is_open());
        crate::platform::system_tray::show((1200, 56))?;
        assert!(!crate::platform::system_tray::is_open());
        crate::platform::system_tray::show((1200, 56))?;
        assert!(crate::platform::system_tray::is_open());
        Ok(())
    }
}
