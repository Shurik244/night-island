use std::collections::HashMap;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, HSTRING, Interface, PWSTR};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub handle: isize,
    pub path: String,
    pub title: String,
    pub active: bool,
    pub minimized: bool,
    pub island_settings: bool,
}

#[derive(Default)]
struct WindowCache {
    windows: Vec<Window>,
    scanned: Option<std::time::Instant>,
    foreground: isize,
    pending: bool,
}

static WINDOW_CACHE: std::sync::Mutex<WindowCache> =
    std::sync::Mutex::new(WindowCache { windows: Vec::new(), scanned: None, foreground: 0, pending: false });

pub fn cached_windows() -> Vec<Window> {
    let foreground = unsafe { GetForegroundWindow() }.0 as isize;
    let Ok(mut cache) = WINDOW_CACHE.lock() else { return Vec::new() };
    let request = !cache.pending
        && (cache.scanned.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(2))
            || cache.foreground != foreground);
    if request {
        cache.pending = true;
        cache.foreground = foreground;
        cache.scanned = Some(std::time::Instant::now());
    }
    let mut snapshot = cache.windows.clone();
    drop(cache);
    for window in &mut snapshot {
        window.active = window.handle == foreground;
    }
    if request {
        let worker = std::thread::Builder::new().name("dock-windows".into()).spawn(|| {
            let snapshot = windows();
            if let Ok(mut cache) = WINDOW_CACHE.lock() {
                cache.windows = snapshot;
                cache.pending = false;
            }
        });
        if worker.is_err()
            && let Ok(mut cache) = WINDOW_CACHE.lock()
        {
            cache.pending = false;
        }
    }
    snapshot
}

#[derive(Clone, Debug)]
pub struct Monitor {
    pub device: String,
    pub bounds: RECT,
    pub work: RECT,
    pub primary: bool,
}

pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn each(handle: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let out = unsafe { &mut *(data.0 as *mut Vec<Monitor>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(handle, &mut info.monitorInfo) }.as_bool() {
            let end = info.szDevice.iter().position(|c| *c == 0).unwrap_or(info.szDevice.len());
            out.push(Monitor {
                device: String::from_utf16_lossy(&info.szDevice[..end]),
                bounds: info.monitorInfo.rcMonitor,
                work: info.monitorInfo.rcWork,
                primary: info.monitorInfo.dwFlags & 1 != 0,
            });
        }
        true.into()
    }
    let mut out = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM((&raw mut out) as isize));
    }
    out.sort_by_key(|m: &Monitor| (!m.primary, m.bounds.left, m.bounds.top));
    out
}

pub fn resolved_target(target: &str) -> String {
    if !target.to_lowercase().ends_with(".lnk") {
        return target.to_string();
    }
    let resolve = || -> windows::core::Result<String> {
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.cast::<IPersistFile>()?.Load(&HSTRING::from(target), STGM_READ)?;
            let mut path = [0u16; 2048];
            link.GetPath(&mut path, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)?;
            let end = path.iter().position(|c| *c == 0).unwrap_or(path.len());
            Ok(String::from_utf16_lossy(&path[..end]))
        }
    };
    resolve().ok().filter(|p| !p.is_empty()).unwrap_or_else(|| target.to_string())
}

fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 2048];
        let mut len = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        result.ok().map(|_| String::from_utf16_lossy(&buffer[..len as usize]))
    }
}

pub fn windows() -> Vec<Window> {
    struct Scan {
        out: Vec<Window>,
        paths: HashMap<u32, Option<String>>,
        foreground: HWND,
    }
    unsafe extern "system" fn each(hwnd: HWND, param: LPARAM) -> BOOL {
        let scan = unsafe { &mut *(param.0 as *mut Scan) };
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() || GetWindowTextLengthW(hwnd) == 0 {
                return true.into();
            }
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            if style & WS_EX_TOOLWINDOW.0 != 0
                || (GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null()) && style & WS_EX_APPWINDOW.0 == 0)
            {
                return true.into();
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast(), size_of::<u32>() as u32);
            if cloaked != 0 {
                return true.into();
            }
            let mut class = [0u16; 128];
            let n = GetClassNameW(hwnd, &mut class) as usize;
            if matches!(
                String::from_utf16_lossy(&class[..n]).as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            ) {
                return true.into();
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let island_settings =
                pid == std::process::id() && String::from_utf16_lossy(&class[..n]) == "NightIsland.Settings";
            if pid == std::process::id() && !island_settings {
                return true.into();
            }
            let path = scan.paths.entry(pid).or_insert_with(|| process_path(pid));
            let Some(path) = path else { return true.into() };
            let mut title = [0u16; 512];
            let n = GetWindowTextW(hwnd, &mut title) as usize;
            scan.out.push(Window {
                handle: hwnd.0 as isize,
                path: path.clone(),
                title: String::from_utf16_lossy(&title[..n]),
                active: hwnd == scan.foreground,
                minimized: IsIconic(hwnd).as_bool(),
                island_settings,
            });
        }
        (scan.out.len() < 256).into()
    }
    let mut scan = Scan { out: Vec::new(), paths: HashMap::new(), foreground: unsafe { GetForegroundWindow() } };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM((&raw mut scan) as isize));
    }
    scan.out
}

pub fn activate(window: &Window, minimize_active: bool) {
    let hwnd = HWND(window.handle as *mut _);
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if process_path(pid)
            .is_none_or(|path| crate::data::desktop_dock::key(&path) != crate::data::desktop_dock::key(&window.path))
        {
            return;
        }
        if minimize_active && GetForegroundWindow() == hwnd {
            let _ = ShowWindowAsync(hwnd, SW_MINIMIZE);
        } else {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindowAsync(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn foreground_app() -> Option<HWND> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return None;
        }
        let mut class = [0u16; 128];
        let n = GetClassNameW(hwnd, &mut class) as usize;
        if matches!(
            String::from_utf16_lossy(&class[..n]).as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return None;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast(), size_of::<u32>() as u32);
        (cloaked == 0).then_some(hwnd)
    }
}

fn on_monitor(hwnd: HWND, monitor: &Monitor) -> bool {
    unsafe {
        let handle = windows::Win32::Graphics::Gdi::MonitorFromWindow(
            hwnd,
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        );
        let mut info = windows::Win32::Graphics::Gdi::MONITORINFO {
            cbSize: size_of::<windows::Win32::Graphics::Gdi::MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(handle, &mut info).as_bool() && info.rcMonitor == monitor.bounds
    }
}

fn fullscreen_bounds(rect: RECT, monitor: RECT, captioned: bool) -> bool {
    !captioned
        && monitor.right > monitor.left
        && monitor.bottom > monitor.top
        && rect.left <= monitor.left + 2
        && rect.top <= monitor.top + 2
        && rect.right >= monitor.right - 2
        && rect.bottom >= monitor.bottom - 2
}

#[derive(Clone, Copy)]
pub enum WindowAction {
    Restore,
    Maximize,
    Minimize,
    Close,
}

pub fn change_window(window: &Window, action: WindowAction) -> bool {
    let hwnd = HWND(window.handle as *mut _);
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if process_path(pid)
            .is_none_or(|path| crate::data::desktop_dock::key(&path) != crate::data::desktop_dock::key(&window.path))
        {
            return false;
        }
        match action {
            WindowAction::Restore => ShowWindowAsync(hwnd, SW_RESTORE).as_bool(),
            WindowAction::Maximize => ShowWindowAsync(hwnd, SW_MAXIMIZE).as_bool(),
            WindowAction::Minimize => ShowWindowAsync(hwnd, SW_MINIMIZE).as_bool(),
            WindowAction::Close => {
                PostMessageW(Some(hwnd), WM_CLOSE, windows::Win32::Foundation::WPARAM(0), LPARAM(0)).is_ok()
            }
        }
    }
}

pub fn file_properties(owner: HWND, target: &str) {
    unsafe {
        let _ = windows::Win32::UI::Shell::SHObjectProperties(
            Some(owner),
            windows::Win32::UI::Shell::SHOP_FILEPATH,
            &HSTRING::from(target),
            windows::core::PCWSTR::null(),
        );
    }
}

pub fn fullscreen(monitor: &Monitor) -> bool {
    use windows::Win32::UI::Shell::{QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState};
    let Some(hwnd) = foreground_app() else { return false };
    let exclusive = unsafe { SHQueryUserNotificationState() }.is_ok_and(|state| state == QUNS_RUNNING_D3D_FULL_SCREEN);
    fullscreen_window(hwnd, monitor, exclusive)
}

fn fullscreen_window(hwnd: HWND, monitor: &Monitor, exclusive: bool) -> bool {
    use windows::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS;
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        if exclusive && on_monitor(hwnd, monitor) {
            return true;
        }
        let captioned = GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION.0 == WS_CAPTION.0;
        let mut rect = RECT::default();
        if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut rect).cast(), size_of::<RECT>() as u32)
            .is_err()
            && GetWindowRect(hwnd, &mut rect).is_err()
        {
            return false;
        }
        fullscreen_bounds(rect, monitor.bounds, captioned)
    }
}

pub fn foreground_app_hidden(monitor: &Monitor, hidden_apps: &[String]) -> bool {
    if hidden_apps.is_empty() {
        return false;
    }
    let Some(hwnd) = foreground_app() else { return false };
    if !on_monitor(hwnd, monitor) {
        return false;
    }
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    process_path(pid).is_some_and(|path| {
        let path = crate::data::desktop_dock::key(&path);
        hidden_apps.iter().any(|hidden| crate::data::desktop_dock::key(hidden) == path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};
    use windows::core::w;

    #[test]
    fn fullscreen_geometry_distinguishes_games_from_maximized_windows_and_other_monitors() {
        let primary = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
        let secondary = RECT { left: -2560, top: -360, right: 0, bottom: 1080 };
        assert!(fullscreen_bounds(primary, primary, false));
        assert!(fullscreen_bounds(secondary, secondary, false));
        assert!(!fullscreen_bounds(primary, primary, true));
        assert!(!fullscreen_bounds(primary, secondary, false));
        assert!(!fullscreen_bounds(RECT { bottom: 1040, ..primary }, primary, false));
        assert!(fullscreen_bounds(RECT { left: 1, top: 1, right: 1919, bottom: 1079 }, primary, false));
        assert!(!fullscreen_bounds(RECT::default(), RECT::default(), false));
    }

    #[test]
    #[ignore = "Creates a temporary native window behind the desktop"]
    fn native_dock_fullscreen_monitor_and_windowed_transitions() -> windows::core::Result<()> {
        use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
        unsafe {
            let previous_dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            struct Dpi(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);
            impl Drop for Dpi {
                fn drop(&mut self) {
                    unsafe {
                        SetThreadDpiAwarenessContext(self.0);
                    }
                }
            }
            let _dpi = Dpi(previous_dpi);
            let monitor = monitors().into_iter().next().unwrap();
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Dock fullscreen geometry test"),
                WS_POPUP,
                -12000,
                -12000,
                320,
                180,
                None,
                None,
                Some(windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?.into()),
                None,
            )?;
            struct WindowGuard(HWND);
            impl Drop for WindowGuard {
                fn drop(&mut self) {
                    unsafe {
                        let _ = DestroyWindow(self.0);
                    }
                }
            }
            let _guard = WindowGuard(hwnd);
            let place = |rect: RECT| -> windows::core::Result<()> {
                SetWindowPos(
                    hwnd,
                    Some(HWND_BOTTOM),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
                )?;
                windows::Win32::Graphics::Dwm::DwmFlush()?;
                Ok(())
            };
            place(monitor.bounds)?;
            assert!(fullscreen_window(hwnd, &monitor, false));
            let caption = WS_VISIBLE | WS_OVERLAPPEDWINDOW;
            SetWindowLongW(hwnd, GWL_STYLE, caption.0 as i32);
            place(monitor.bounds)?;
            assert!(!fullscreen_window(hwnd, &monitor, false));
            SetWindowLongW(hwnd, GWL_STYLE, (WS_VISIBLE | WS_POPUP).0 as i32);
            let inset = RECT { right: monitor.bounds.right - 80, bottom: monitor.bounds.bottom - 80, ..monitor.bounds };
            place(inset)?;
            assert!(!fullscreen_window(hwnd, &monitor, false));
            assert!(fullscreen_window(hwnd, &monitor, true));
            let elsewhere = Monitor {
                device: "Disconnected".into(),
                bounds: RECT { left: -30000, top: -30000, right: -28080, bottom: -28920 },
                work: RECT::default(),
                primary: false,
            };
            assert!(!fullscreen_window(hwnd, &elsewhere, false));
            assert!(!fullscreen_window(hwnd, &elsewhere, true));
            place(monitor.bounds)?;
            assert!(fullscreen_window(hwnd, &monitor, false));
            let _ = ShowWindow(hwnd, SW_HIDE);
            assert!(!fullscreen_window(hwnd, &monitor, false));
        }
        Ok(())
    }

    #[test]
    #[ignore = "Helper for the isolated dock window test"]
    fn native_dock_window_fixture() {
        let Some(file) = std::env::var_os("NI_DOCK_WINDOW_FIXTURE") else { return };
        unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap();
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    CreateWindowExW(
                        WS_EX_NOACTIVATE,
                        w!("STATIC"),
                        w!("Night Island dock test fixture"),
                        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                        -12000,
                        -12000,
                        320,
                        180,
                        None,
                        None,
                        Some(instance.into()),
                        None,
                    )
                    .unwrap()
                })
                .collect();
            std::fs::write(
                file,
                serde_json::to_vec(&handles.iter().map(|h| h.0 as isize).collect::<Vec<_>>()).unwrap(),
            )
            .unwrap();
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(15) && handles.iter().any(|h| IsWindow(Some(*h)).as_bool()) {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            for hwnd in handles {
                let _ = DestroyWindow(hwnd);
            }
        }
    }

    #[test]
    #[ignore = "Creates temporary offscreen native windows in a child process"]
    fn native_dock_discovers_and_restores_other_process_windows() {
        struct Fixture {
            child: Child,
            file: std::path::PathBuf,
            foreground: HWND,
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = self.child.kill();
                let _ = self.child.wait();
                let _ = std::fs::remove_file(&self.file);
                unsafe {
                    let _ = SetForegroundWindow(self.foreground);
                }
            }
        }
        let file = std::env::temp_dir().join(format!("night-island-dock-fixture-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "platform::desktop_dock::tests::native_dock_window_fixture",
                "--test-threads=1",
            ])
            .env("NI_DOCK_WINDOW_FIXTURE", &file)
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let fixture = Fixture { child, file, foreground: unsafe { GetForegroundWindow() } };
        let wait = |predicate: &dyn Fn() -> bool| {
            let started = Instant::now();
            while !predicate() && started.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(predicate());
        };
        wait(&|| {
            std::fs::read(&fixture.file).ok().and_then(|b| serde_json::from_slice::<Vec<isize>>(&b).ok()).is_some()
        });
        let handles: Vec<isize> = serde_json::from_slice(&std::fs::read(&fixture.file).unwrap()).unwrap();
        let snapshot = windows();
        let found: Vec<_> = snapshot.into_iter().filter(|w| handles.contains(&w.handle)).collect();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].path, found[1].path);
        assert!(!found[0].path.is_empty());
        let hwnd = HWND(found[0].handle as *mut _);
        assert!(change_window(&found[0], WindowAction::Minimize));
        wait(&|| unsafe { IsIconic(hwnd) }.as_bool());
        assert!(windows().iter().any(|w| w.handle == found[0].handle));
        activate(&found[0], false);
        wait(&|| !unsafe { IsIconic(hwnd) }.as_bool());
        let mut stale = found[0].clone();
        stale.path = r"C:\not-the-original-application.exe".into();
        assert!(!change_window(&stale, WindowAction::Close));
        assert!(change_window(&found[0], WindowAction::Restore));
        assert!(change_window(&found[0], WindowAction::Close));
        wait(&|| !windows().iter().any(|w| w.handle == found[0].handle));
    }
}
