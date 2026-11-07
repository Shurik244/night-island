use std::marker::PhantomData;
use std::rc::Rc;

use windows::Win32::Foundation::{CloseHandle, HWND, RECT, SIZE};
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PWSTR;

use super::desktop_dock::Window;

#[derive(PartialEq, Eq)]
struct Identity {
    pid: u32,
    thread: u32,
    owner: isize,
    class: Vec<u16>,
    path: String,
}

impl Identity {
    fn read(hwnd: HWND) -> Option<Self> {
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() || GetAncestor(hwnd, GA_ROOT) != hwnd {
                return None;
            }
            let mut pid = 0;
            let thread = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 || thread == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 32768];
            let mut len = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len);
            let _ = CloseHandle(process);
            result.ok()?;
            let path = crate::data::desktop_dock::key(&String::from_utf16_lossy(&buffer[..len as usize]));
            let mut class = [0u16; 256];
            let len = GetClassNameW(hwnd, &mut class);
            if len == 0 {
                return None;
            }
            Some(Self {
                pid,
                thread,
                owner: GetWindow(hwnd, GW_OWNER).unwrap_or_default().0 as isize,
                class: class[..len as usize].to_vec(),
                path,
            })
        }
    }
}

pub struct Thumbnail {
    handle: Option<isize>,
    source: HWND,
    destination: HWND,
    identity: Identity,
    destination_thread: u32,
    thread_bound: PhantomData<Rc<()>>,
}

impl Thumbnail {
    pub fn new(destination: HWND, source: &Window) -> Option<Self> {
        let source_hwnd = HWND(source.handle as *mut _);
        let identity = Identity::read(source_hwnd)?;
        if identity.path != crate::data::desktop_dock::key(&source.path) || source_hwnd == destination {
            return None;
        }
        unsafe {
            let mut pid = 0;
            let destination_thread = GetWindowThreadProcessId(destination, Some(&mut pid));
            if pid != std::process::id() || destination_thread == 0 || GetAncestor(destination, GA_ROOT) != destination
            {
                return None;
            }
            let handle = DwmRegisterThumbnail(destination, source_hwnd).ok()?;
            let thumbnail = Self {
                handle: Some(handle),
                source: source_hwnd,
                destination,
                identity,
                destination_thread,
                thread_bound: PhantomData,
            };
            thumbnail.source_size()?;
            Some(thumbnail)
        }
    }

    fn valid(&self) -> bool {
        unsafe {
            let mut pid = 0;
            if GetWindowThreadProcessId(self.destination, Some(&mut pid)) != self.destination_thread
                || pid != std::process::id()
                || !IsWindowVisible(self.source).as_bool()
                || Identity::read(self.source).as_ref() != Some(&self.identity)
            {
                return false;
            }
            let mut cloaked = 0u32;
            if DwmGetWindowAttribute(self.source, DWMWA_CLOAKED, &mut cloaked as *mut _ as _, size_of::<u32>() as u32)
                .is_ok()
                && cloaked != 0
            {
                return false;
            }
            let mut affinity = 0;
            !(GetWindowDisplayAffinity(self.source, &mut affinity).is_ok() && affinity != WDA_NONE.0)
        }
    }

    pub fn source_size(&self) -> Option<SIZE> {
        if !self.valid() {
            return None;
        }
        let size = unsafe { DwmQueryThumbnailSourceSize(self.handle?).ok()? };
        (size.cx > 0 && size.cy > 0).then_some(size)
    }

    pub fn is_minimized(&self) -> bool {
        unsafe { IsIconic(self.source).as_bool() }
    }

    pub fn update(&mut self, destination: RECT, visible: bool) -> bool {
        let Some(handle) = self.handle else { return false };
        if self.source_size().is_none()
            || destination.right <= destination.left
            || destination.bottom <= destination.top
        {
            self.unregister();
            return false;
        }
        let properties = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
            rcDestination: destination,
            opacity: 255,
            fVisible: visible.into(),
            fSourceClientAreaOnly: false.into(),
            ..Default::default()
        };
        if unsafe { DwmUpdateThumbnailProperties(handle, &properties) }.is_err() {
            self.unregister();
            return false;
        }
        true
    }

    pub fn set_opacity(&self, opacity: u8) {
        if let Some(handle) = self.handle {
            let properties = DWM_THUMBNAIL_PROPERTIES { dwFlags: DWM_TNP_OPACITY, opacity, ..Default::default() };
            unsafe {
                let _ = DwmUpdateThumbnailProperties(handle, &properties);
            }
        }
    }

    fn unregister(&mut self) {
        if let Some(handle) = self.handle.take() {
            unsafe {
                let _ = DwmUnregisterThumbnail(handle);
            }
        }
    }
}

impl Drop for Thumbnail {
    fn drop(&mut self) {
        self.unregister();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{COLORREF, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
    use windows::core::w;

    struct Fixture {
        source: HWND,
        destination: HWND,
        dpi: windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.destination);
                let _ = DestroyWindow(self.source);
                SetThreadDpiAwarenessContext(self.dpi);
            }
        }
    }

    unsafe extern "system" fn paint(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            if msg == WM_PAINT {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let brush = CreateSolidBrush(COLORREF(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32));
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                FillRect(dc, &rect, brush);
                let _ = DeleteObject(brush.into());
                let _ = EndPaint(hwnd, &ps);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }

    fn pump() {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = DwmFlush();
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    fn fixture(destination_style: WINDOW_EX_STYLE) -> Fixture {
        unsafe {
            let dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let instance = GetModuleHandleW(None).unwrap();
            let class = w!("NightIsland.PreviewTest");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(paint),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let monitor = super::super::desktop_dock::monitors().into_iter().find(|monitor| monitor.primary).unwrap();
            let create = |offset, style| {
                CreateWindowExW(
                    WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | style,
                    class,
                    w!("Night Island preview test"),
                    WS_POPUP,
                    monitor.work.left + offset,
                    monitor.work.top + 40,
                    240,
                    160,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
                .unwrap()
            };
            let source = create(40, WINDOW_EX_STYLE(0));
            let destination = create(320, destination_style);
            let _ = ShowWindow(source, SW_SHOWNOACTIVATE);
            let _ = ShowWindow(destination, SW_SHOWNOACTIVATE);
            Fixture { source, destination, dpi }
        }
    }

    fn record(source: HWND) -> Window {
        Window {
            handle: source.0 as isize,
            path: std::env::current_exe().unwrap().to_string_lossy().into_owned(),
            title: "Preview test".into(),
            active: false,
            minimized: false,
            island_settings: false,
        }
    }

    fn color_reaches_destination(fixture: &Fixture, color: u32) -> bool {
        unsafe {
            SetWindowLongPtrW(fixture.source, GWLP_USERDATA, color as isize);
            let _ = InvalidateRect(Some(fixture.source), None, false);
            let mut rect = RECT::default();
            GetWindowRect(fixture.destination, &mut rect).unwrap();
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(2) {
                pump();
                let dc = GetDC(None);
                let pixel = GetPixel(dc, rect.left + 80, rect.top + 60);
                ReleaseDC(None, dc);
                if pixel.0 == color {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    #[ignore = "Shows temporary native source and preview windows without activation"]
    fn native_thumbnail_updates_live_and_preserves_minimized_source() {
        let foreground = unsafe { GetForegroundWindow() };
        let fixture = fixture(WINDOW_EX_STYLE(0));
        let source = record(fixture.source);
        let mut thumbnail = Thumbnail::new(fixture.destination, &source).expect("DWM thumbnail registration");
        let size = thumbnail.source_size().unwrap();
        assert_eq!((size.cx, size.cy), (240, 160));
        let rect = RECT { left: 10, top: 10, right: 230, bottom: 150 };
        assert!(thumbnail.update(rect, true));
        assert!(color_reaches_destination(&fixture, 0x0011bb22));
        assert!(color_reaches_destination(&fixture, 0x00cc3311));
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        unsafe {
            let _ = ShowWindow(fixture.source, SW_SHOWMINNOACTIVE);
        }
        pump();
        assert!(thumbnail.is_minimized());
        let can_preview = thumbnail.source_size().is_some();
        assert_eq!(thumbnail.update(rect, true), can_preview);
        assert!(unsafe { IsIconic(fixture.source).as_bool() });
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        let handle = thumbnail.handle;
        drop(thumbnail);
        if let Some(handle) = handle {
            assert!(unsafe { DwmQueryThumbnailSourceSize(handle) }.is_err());
        }
    }

    #[test]
    #[ignore = "Creates temporary native windows to validate stale and protected sources"]
    fn native_thumbnail_rejects_changed_hidden_closed_and_protected_sources() {
        let fixture = fixture(WINDOW_EX_STYLE(0));
        let source = record(fixture.source);
        let mut wrong = source.clone();
        wrong.path.push_str(".different");
        assert!(Thumbnail::new(fixture.destination, &wrong).is_none());
        assert!(Thumbnail::new(fixture.source, &source).is_none());
        let rect = RECT { left: 0, top: 0, right: 100, bottom: 100 };
        let mut thumbnail = Thumbnail::new(fixture.destination, &source).unwrap();
        unsafe {
            let _ = ShowWindow(fixture.source, SW_HIDE);
        }
        assert!(thumbnail.source_size().is_none());
        assert!(!thumbnail.update(rect, true));
        assert!(thumbnail.handle.is_none());
        unsafe {
            let _ = ShowWindow(fixture.source, SW_SHOWNOACTIVATE);
            SetWindowDisplayAffinity(fixture.source, WDA_EXCLUDEFROMCAPTURE).unwrap();
        }
        assert!(Thumbnail::new(fixture.destination, &source).is_none());
        unsafe {
            SetWindowDisplayAffinity(fixture.source, WDA_NONE).unwrap();
        }
        let mut thumbnail = Thumbnail::new(fixture.destination, &source).unwrap();
        unsafe {
            SetWindowLongPtrW(fixture.source, GWLP_HWNDPARENT, fixture.destination.0 as isize);
        }
        assert!(!thumbnail.update(rect, true));
        unsafe {
            SetWindowLongPtrW(fixture.source, GWLP_HWNDPARENT, 0);
            let cloaked = windows::core::BOOL::from(true);
            DwmSetWindowAttribute(fixture.source, DWMWA_CLOAK, &cloaked as *const _ as _, size_of_val(&cloaked) as u32)
                .unwrap();
        }
        assert!(Thumbnail::new(fixture.destination, &source).is_none());
        unsafe {
            let cloaked = windows::core::BOOL::from(false);
            DwmSetWindowAttribute(fixture.source, DWMWA_CLOAK, &cloaked as *const _ as _, size_of_val(&cloaked) as u32)
                .unwrap();
        }
        let mut thumbnail = Thumbnail::new(fixture.destination, &source).unwrap();
        unsafe {
            DestroyWindow(fixture.source).unwrap();
        }
        assert!(!thumbnail.update(rect, true));
        assert!(thumbnail.handle.is_none());
    }

    #[test]
    #[ignore = "Checks DWM thumbnail rendering on a no-redirection popup"]
    fn native_thumbnail_no_redirection_destination_compatibility() {
        let fixture = fixture(WS_EX_NOREDIRECTIONBITMAP);
        let mut thumbnail = Thumbnail::new(fixture.destination, &record(fixture.source)).unwrap();
        assert!(thumbnail.update(RECT { left: 10, top: 10, right: 230, bottom: 150 }, true));
        assert!(color_reaches_destination(&fixture, 0x0033cc11));
    }
}
