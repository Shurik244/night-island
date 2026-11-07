use super::*;

use crate::data::desktop_dock::Pin;
use crate::platform::dock_actions::{AppAudio, DiscordVoice, MediaSession};
use windows::UI::Composition::Compositor;
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
    RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};

struct Runtime(DPI_AWARENESS_CONTEXT);

impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
            RoUninitialize();
        }
    }
}

struct Sources(Vec<HWND>);

impl Drop for Sources {
    fn drop(&mut self) {
        for hwnd in &self.0 {
            unsafe {
                if IsWindow(Some(*hwnd)).as_bool() {
                    let _ = DestroyWindow(*hwnd);
                }
            }
        }
    }
}

unsafe extern "system" fn source_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if message == WM_PAINT {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let brush = CreateSolidBrush(COLORREF(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32));
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            FillRect(dc, &rect, brush);
            let _ = DeleteObject(brush.into());
            if GetWindowLongPtrW(hwnd, GWLP_USERDATA) == 0 {
                let white = CreateSolidBrush(COLORREF(0x00ffffff));
                for x in (0..rect.right).step_by(8) {
                    FillRect(dc, &RECT { left: x, top: 0, right: x + 4, bottom: rect.bottom }, white);
                }
                let _ = DeleteObject(white.into());
            }
            let _ = EndPaint(hwnd, &ps);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, message, w, l)
    }
}

fn pump() {
    unsafe {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let _ = DwmFlush();
    }
    std::thread::sleep(Duration::from_millis(20));
}

fn click(preview: &Preview, action: Action) {
    let foreground = unsafe { GetForegroundWindow() };
    let button = preview.state.buttons.iter().find(|button| button.action == action && button.enabled).unwrap();
    let x = ((button.rect.0 + button.rect.2) * 0.5 * preview.state.scale) as u16;
    let y = ((button.rect.1 + button.rect.3) * 0.5 * preview.state.scale) as u16;
    let point = LPARAM((u32::from(x) | (u32::from(y) << 16)) as isize);
    unsafe {
        SendMessageW(preview.state.hwnd, WM_MOUSEMOVE, Some(WPARAM(0)), Some(point));
        SendMessageW(preview.state.hwnd, WM_LBUTTONDOWN, Some(WPARAM(0)), Some(point));
        SendMessageW(preview.state.hwnd, WM_LBUTTONUP, Some(WPARAM(0)), Some(point));
        assert_eq!(GetForegroundWindow(), foreground);
    }
    pump();
}

fn expect_live_color(preview: &Preview, color: u32) {
    let image = scaled(preview.state.layout.image, preview.state.scale);
    let x = preview.state.origin.0 + ((image.0 + image.2) * 0.5) as i32;
    let y = preview.state.origin.1 + ((image.1 + image.3) * 0.5) as i32;
    let start = Instant::now();
    let mut pixel = COLORREF(0);
    while start.elapsed() < Duration::from_secs(2) {
        pump();
        unsafe {
            let dc = GetDC(None);
            pixel = GetPixel(dc, x, y);
            ReleaseDC(None, dc);
        }
        if pixel.0 == color {
            return;
        }
    }
    panic!("DWM preview pixel mismatch: expected {color:#x}, got {:#x}", pixel.0);
}

fn volume_drag(preview: &mut Preview) {
    let foreground = unsafe { GetForegroundWindow() };
    let hwnd = preview.state.hwnd;
    let button = preview.state.buttons.iter().find(|button| button.action == Action::Volume).unwrap();
    let y = (button.rect.1 + button.rect.3) * 0.5;
    let point = |x: f32| {
        let x = (x * preview.state.scale) as i16 as u16;
        let y = (y * preview.state.scale) as i16 as u16;
        LPARAM((u32::from(x) | (u32::from(y) << 16)) as isize)
    };
    let start = point((button.rect.0 + button.rect.2) * 0.5);
    let minimum = point(-10.0);
    let maximum = point(preview.state.layout.width + 20.0);
    unsafe {
        SendMessageW(hwnd, WM_LBUTTONDOWN, Some(WPARAM(1)), Some(start));
        assert_eq!(GetCapture(), hwnd);
        SendMessageW(hwnd, WM_MOUSEMOVE, Some(WPARAM(1)), Some(minimum));
        assert_eq!(preview.state.snapshot.audio.as_ref().unwrap().volume, 0.0);
        SendMessageW(hwnd, WM_MOUSEMOVE, Some(WPARAM(1)), Some(maximum));
        assert_eq!(preview.state.snapshot.audio.as_ref().unwrap().volume, 1.0);
        SendMessageW(hwnd, WM_LBUTTONUP, Some(WPARAM(0)), Some(maximum));
        assert_ne!(GetCapture(), hwnd);
        assert!(preview.state.pressed.is_none());
        assert_eq!(GetForegroundWindow(), foreground);
    }
}

fn screenshot(preview: &Preview, name: &str) -> Result<()> {
    let Some(folder) = std::env::var_os("NI_DOCK_HOVER_SCREENSHOTS") else { return Ok(()) };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    let size = (
        (preview.state.layout.width * preview.state.scale).ceil() as i32,
        (preview.state.layout.height * preview.state.scale).ceil() as i32,
    );
    let surface = crate::ui::popup::PopupSurface::new(size)?;
    surface.paint(&preview.state.gpu, |p| preview.state.paint(p))?;
    pump();
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.0,
                biHeight: -size.1,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let previous = SelectObject(dc, bitmap.into());
        let copied =
            BitBlt(dc, 0, 0, size.0, size.1, Some(screen), preview.state.origin.0, preview.state.origin.1, SRCCOPY);
        ReleaseDC(None, screen);
        let mut pixels = std::slice::from_raw_parts(bits.cast::<u8>(), (size.0 * size.1 * 4) as usize).to_vec();
        SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        copied?;
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let mut encoder = png::Encoder::new(
            std::fs::File::create(folder.join(format!("{name}.png"))).unwrap(),
            size.0 as u32,
            size.1 as u32,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    Ok(())
}

fn verify_preview_material(preview: &mut Preview, work: RECT) -> Result<()> {
    assert!(preview.state.backdrop.is_some());
    let saved = preview.state.settings.clone();
    let size = (
        (preview.state.layout.width * preview.state.scale) as i32,
        (preview.state.layout.height * preview.state.scale) as i32,
    );
    let origin = preview.state.origin;
    let background = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("NightIsland.DockPreviewFixture"),
            w!("Preview material fixture"),
            WS_POPUP,
            work.left,
            work.top,
            work.right - work.left,
            work.bottom - work.top,
            None,
            None,
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?
    };
    let _background = Sources(vec![background]);
    unsafe {
        let _ = ShowWindow(background, SW_SHOWNOACTIVATE);
    }
    let sample = || -> Vec<[u8; 3]> {
        unsafe {
            let dc = GetDC(None);
            let result = (60..size.0 - 60)
                .map(|x| {
                    let pixel = GetPixel(dc, origin.0 + x, origin.1 + size.1 - 19).0;
                    [pixel as u8, (pixel >> 8) as u8, (pixel >> 16) as u8]
                })
                .collect();
            ReleaseDC(None, dc);
            result
        }
    };
    let settle = || {
        for _ in 0..12 {
            pump();
        }
    };
    let mut energy = Vec::new();
    for blur in [false, true] {
        preview.state.settings.blur = blur;
        preview.state.settings.opacity = 0.2;
        preview.state.settings.blur_opacity = 0.2;
        preview.state.arrange();
        preview.refresh_backdrop();
        settle();
        let pixels = sample();
        energy.push(pixels.windows(2).map(|p| p[0][0].abs_diff(p[1][0]) as f32).sum::<f32>());
        screenshot(preview, if blur { "material-blur-on" } else { "material-blur-off" })?;
        assert!(preview.state.thumbnail_visible);
    }
    assert!(energy[0] > 1000.0 && energy[1] < energy[0] * 0.6, "Preview must blur the live background: {energy:?}");
    let mut means = Vec::new();
    for color in [0x000000ff, 0x00ff0000] {
        unsafe {
            SetWindowLongPtrW(background, GWLP_USERDATA, color);
            let _ = InvalidateRect(Some(background), None, false);
        }
        preview.refresh_backdrop();
        settle();
        let pixels = sample();
        means
            .push([0, 1, 2].map(|channel| pixels.iter().map(|p| p[channel] as u32).sum::<u32>() / pixels.len() as u32));
    }
    assert!(means[0][0] > means[0][2] + 80 && means[1][2] > means[1][0] + 80, "Live scene must update: {means:?}");
    preview.state.settings.blur_opacity = 1.0;
    preview.state.settings.tint_hue = 120.0;
    preview.state.settings.tint_saturation = 1.0;
    preview.state.settings.tint_brightness = 0.3;
    preview.state.arrange();
    settle();
    let pixels = sample();
    assert!(pixels.iter().all(|p| p[1] > p[0] + 30 && p[1] > p[2] + 30));
    preview.state.settings = saved;
    preview.state.arrange();
    Ok(())
}

#[test]
#[ignore = "Shows synthetic source windows and the live dock hover popup without activating user apps"]
fn native_dock_preview_live_paint_switch_close_sync_and_dpi() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_SINGLETHREADED)?;
    }
    let _runtime = Runtime(unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) });
    let _queue = unsafe {
        CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        })?
    };
    let compositor = Compositor::new()?;
    let gpu = Gpu::new(&compositor)?;
    let monitor = desktop::monitors().into_iter().find(|monitor| monitor.primary).unwrap();
    let work = monitor.work;
    let instance = unsafe { GetModuleHandleW(None)? };
    let class = w!("NightIsland.DockPreviewFixture");
    unsafe {
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(source_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        });
    }
    let mut sources = Sources(Vec::new());
    for (index, color) in [0x0011bb33, 0x00bb3311].into_iter().enumerate() {
        let source = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class,
                w!("Synthetic dock preview source"),
                WS_POPUP,
                work.left + 20 + index as i32 * 20,
                work.top + 20,
                240,
                160,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        sources.0.push(source);
        unsafe {
            SetWindowLongPtrW(source, GWLP_USERDATA, color);
            let _ = ShowWindow(source, SW_SHOWNOACTIVATE);
        }
    }
    let path = std::env::current_exe().unwrap().to_string_lossy().into_owned();
    let mut entry = Entry {
        pin: Pin { name: "Preview fixture".into(), target: path.clone(), icon: String::new() },
        pinned: true,
        windows: sources
            .0
            .iter()
            .enumerate()
            .map(|(index, hwnd)| desktop::Window {
                handle: hwnd.0 as isize,
                path: path.clone(),
                title: format!("Synthetic source {}", index + 1),
                active: false,
                minimized: false,
                island_settings: false,
            })
            .collect(),
        icon: None,
    };
    let center = (work.left + work.right) as f32 * 0.5;
    let anchor = (center - 24.0, work.bottom as f32 - 30.0, center + 24.0, work.bottom as f32);
    let mut preview = Preview::new(
        sources.0[0],
        &gpu,
        &entry,
        anchor,
        work,
        1.0,
        Options { capture_hidden: false, settings: Default::default(), actions: Arc::new(Client::new()) },
    )?;
    for scale in [1.0, 1.5, 2.0] {
        preview.show(&entry, anchor, work, scale);
        assert!(preview.is_open());
        assert!(preview.state.thumbnail_visible);
        expect_live_color(&preview, 0x0011bb33);
        click(&preview, Action::NextWindow);
        assert_eq!(preview.state.window_index, 1);
        expect_live_color(&preview, 0x00bb3311);
        click(&preview, Action::PreviousWindow);
        expect_live_color(&preview, 0x0011bb33);
        preview.state.snapshot = Snapshot {
            target: path.clone(),
            ready: true,
            media: vec![MediaSession {
                id: 1,
                title: "Synthetic track title".into(),
                artist: "Fixture artist".into(),
                source: "Fixture".into(),
                playing: true,
                can_previous: true,
                can_toggle: true,
                can_next: true,
            }],
            audio: Some(AppAudio { volume: 0.64, muted: false }),
            discord: Some(DiscordVoice {
                connected: true,
                available: true,
                error: None,
                mute: false,
                deaf: false,
                channel: "General".into(),
                members: vec![
                    crate::services::discord::Member { name: "Alex".into(), muted: false, deaf: false, speaking: true },
                    crate::services::discord::Member { name: "Mira".into(), muted: true, deaf: false, speaking: false },
                ],
            }),
        };
        preview.state.arrange();
        expect_live_color(&preview, 0x0011bb33);
        for button in &preview.state.buttons {
            assert!(button.rect.0 >= 0.0 && button.rect.2 <= preview.state.layout.width);
            assert!(button.rect.1 >= 0.0 && button.rect.3 <= preview.state.layout.height);
        }
        volume_drag(&mut preview);
        preview.state.snapshot.audio.as_mut().unwrap().volume = 0.64;
        preview.state.arrange();
        screenshot(&preview, &format!("hover-controls-{scale:.1}"))?;
        let complete_snapshot = preview.state.snapshot.clone();
        let complete_entry = preview.state.entry.clone();
        preview.state.entry.pin.name = "Browser".into();
        preview.state.entry.windows.truncate(1);
        preview.state.snapshot.discord = None;
        preview.state.arrange();
        assert!(preview.state.layout.width <= 320.0);
        assert!(preview.state.layout.height <= 292.0);
        assert!(!preview.state.buttons.iter().any(|button| button.action == Action::CloseWindow));
        expect_live_color(&preview, 0x0011bb33);
        screenshot(&preview, &format!("hover-browser-{scale:.1}"))?;
        preview.state.entry = complete_entry;
        preview.state.snapshot = complete_snapshot;
        preview.state.entry.pin.name = "Discord".into();
        preview.state.snapshot.media.clear();
        preview.state.arrange();
        expect_live_color(&preview, 0x0011bb33);
        screenshot(&preview, &format!("hover-discord-{scale:.1}"))?;
        if scale == 1.0 {
            let voice = preview.state.snapshot.discord.as_mut().unwrap();
            voice.mute = true;
            voice.deaf = true;
            preview.state.arrange();
            screenshot(&preview, "hover-discord-muted")?;
            let voice = preview.state.snapshot.discord.as_mut().unwrap();
            voice.available = false;
            voice.error = Some("Fixture connection unavailable".into());
            preview.state.arrange();
            assert!(
                preview
                    .state
                    .buttons
                    .iter()
                    .filter(|b| matches!(b.action, Action::Microphone | Action::Deafen))
                    .all(|b| !b.enabled)
            );
            screenshot(&preview, "hover-discord-unavailable")?;
        }
        click(&preview, Action::Dismiss);
        let deadline = Instant::now() + Duration::from_millis(350);
        while preview.is_open() && Instant::now() < deadline {
            pump();
        }
        assert!(!preview.is_open());
        assert!(preview.dismissed());
        assert!(preview.state.thumbnail.is_none());
        assert!(!unsafe { IsWindowVisible(preview.state.hwnd).as_bool() });
        if let Some(backdrop) = &preview.state.backdrop {
            assert!(!unsafe { IsWindowVisible(backdrop.window()).as_bool() });
        }
        let foreground = unsafe { GetForegroundWindow() };
        assert_ne!(foreground, preview.state.hwnd);
        assert!(!sources.0.contains(&foreground));
    }
    preview.show(&entry, anchor, work, 1.0);
    verify_preview_material(&mut preview, work)?;
    click(&preview, Action::NextWindow);
    click(&preview, Action::CloseWindow);
    assert!(!unsafe { IsWindow(Some(sources.0[1])).as_bool() });
    entry.windows.retain(|window| unsafe { IsWindow(Some(HWND(window.handle as *mut _))).as_bool() });
    preview.sync_entry(&entry);
    assert_eq!(preview.state.entry.windows.len(), 1);
    assert_eq!(preview.state.window_index, 0);
    expect_live_color(&preview, 0x0011bb33);
    unsafe {
        SetWindowLongPtrW(sources.0[0], GWLP_USERDATA, 0x00cc4422);
        let _ = InvalidateRect(Some(sources.0[0]), None, false);
    }
    expect_live_color(&preview, 0x00cc4422);
    entry.windows.clear();
    preview.sync_entry(&entry);
    assert!(!preview.is_open());
    assert!(preview.state.thumbnail.is_none());
    let foreground = unsafe { GetForegroundWindow() };
    assert_ne!(foreground, preview.state.hwnd);
    assert!(!sources.0.contains(&foreground));
    Ok(())
}

#[test]
#[ignore = "Checks smooth alpha popup composition with live DWM thumbnails using synthetic windows"]
fn native_alpha_popup_thumbnail_compatibility() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_SINGLETHREADED)?;
    }
    let _runtime = Runtime(unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) });
    let _queue = unsafe {
        CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        })?
    };
    let compositor = Compositor::new()?;
    let gpu = Gpu::new(&compositor)?;
    let work = desktop::monitors().into_iter().find(|m| m.primary).unwrap().work;
    let instance = unsafe { GetModuleHandleW(None)? };
    let class = w!("NightIsland.AlphaPreviewFixture");
    unsafe {
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(source_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        });
    }
    let mut windows = Sources(Vec::new());
    let mut create = |style, x, y, width, height, color| -> Result<HWND> {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | style,
                class,
                w!("Alpha preview fixture"),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        windows.0.push(hwnd);
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, color);
        }
        Ok(hwnd)
    };
    let x = work.left + 380;
    let y = work.top + 100;
    let source = create(WINDOW_EX_STYLE(0), work.left + 20, work.top + 20, 240, 160, 0x0011bb33)?;
    let backing = create(WINDOW_EX_STYLE(0), x - 10, y - 10, 320, 260, 0x00bb1166)?;
    let destination = create(WS_EX_LAYERED, x, y, 300, 240, 0)?;
    unsafe {
        let _ = ShowWindow(source, SW_SHOWNOACTIVATE);
        let _ = ShowWindow(backing, SW_SHOWNOACTIVATE);
    }
    let surface = crate::ui::popup::PopupSurface::new((300, 240))?;
    surface.paint(&gpu, |p| p.fill_round_rect((8.0, 8.0, 292.0, 232.0), 28.0, rgba(0.04, 0.04, 0.04, 1.0)))?;
    surface.present(destination, None, 255)?;
    unsafe {
        let _ = ShowWindow(destination, SW_SHOWNOACTIVATE);
    }
    let source_record = desktop::Window {
        handle: source.0 as isize,
        path: std::env::current_exe().unwrap().to_string_lossy().into_owned(),
        title: "Alpha source".into(),
        active: false,
        minimized: false,
        island_settings: false,
    };
    let mut thumbnail = Thumbnail::new(destination, &source_record).unwrap();
    assert!(thumbnail.update(RECT { left: 20, top: 40, right: 280, bottom: 180 }, true));
    let mut center = 0;
    let mut corner = 0;
    for _ in 0..20 {
        pump();
        unsafe {
            let dc = GetDC(None);
            center = GetPixel(dc, x + 150, y + 100).0;
            corner = GetPixel(dc, x + 10, y + 10).0;
            ReleaseDC(None, dc);
        }
        if center == 0x0011bb33 {
            break;
        }
    }
    println!("layered center={center:#x} corner={corner:#x}");
    assert_eq!(corner, 0x00bb1166, "Rounded alpha corner should reveal synthetic backing");
    assert_eq!(center, 0x0011bb33, "Live DWM thumbnail should render over alpha popup");
    unsafe {
        SetWindowLongPtrW(source, GWLP_USERDATA, 0x00cc4422);
        let _ = InvalidateRect(Some(source), None, false);
    }
    surface.paint(&gpu, |p| p.fill_round_rect((8.0, 8.0, 292.0, 232.0), 28.0, rgba(0.08, 0.08, 0.08, 1.0)))?;
    surface.present(destination, None, 255)?;
    for _ in 0..20 {
        pump();
        unsafe {
            let dc = GetDC(None);
            center = GetPixel(dc, x + 150, y + 100).0;
            ReleaseDC(None, dc);
        }
        if center == 0x00cc4422 {
            break;
        }
    }
    assert_eq!(center, 0x00cc4422, "Re-presenting alpha content must preserve live thumbnail updates");
    Ok(())
}
