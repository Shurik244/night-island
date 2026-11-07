use super::*;
use crate::ui::gfx::WHITE;
use windows::UI::Composition::Compositor;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_CENTER;
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
struct Window(HWND);
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

fn save(wing: &Wing, name: &str) -> Result<()> {
    if std::env::var_os("NI_TOP_PANEL_PREVIEW").is_none() {
        return Ok(());
    }
    let surface = PopupSurface::new(wing.state.size)?;
    surface.paint(&wing.state.gpu, |p| wing.state.draw(p))?;
    save_surface(&surface, wing.state.size, name)
}

fn save_surface(surface: &PopupSurface, size: (i32, i32), name: &str) -> Result<()> {
    let Some(folder) = std::env::var_os("NI_TOP_PANEL_PREVIEW") else { return Ok(()) };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    let mut pixels = surface.pixels();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        let alpha = pixel[3] as u32;
        if alpha != 0 {
            for value in &mut pixel[..3] {
                *value = ((*value as u32 * 255) / alpha).min(255) as u8;
            }
        }
    }
    let mut encoder = png::Encoder::new(
        std::fs::File::create(folder.join(format!("{name}.png"))).unwrap(),
        size.0 as u32,
        size.1 as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    Ok(())
}

fn capture_icon_catalog(gpu: &Gpu) -> Result<()> {
    let glyphs: Vec<_> = crate::ui::icons::modern_glyphs().collect();
    let columns = 11;
    let rows = glyphs.len().div_ceil(columns);
    let size = (932, 96 + rows as i32 * 62);
    let surface = PopupSurface::new(size)?;
    let cell = |i: usize| (25 + (i % columns) * 80, 86 + (i / columns) * 62);
    let draw = |p: &Painter, catalog: bool| -> Result<()> {
        if catalog {
            p.fill_rect((0.0, 0.0, size.0 as f32, size.1 as f32), rgba(0.047, 0.051, 0.071, 1.0))?;
            p.text(
                "Night Island · Иконки интерфейса",
                (25.0, 15.0, 890.0, 52.0),
                25.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                WHITE,
            )?;
            p.text(
                "Проверка в двух размерах · 24 и 12 px",
                (25.0, 52.0, 890.0, 75.0),
                13.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                rgba(0.72, 0.76, 0.85, 1.0),
            )?;
        }
        for (i, &ch) in glyphs.iter().enumerate() {
            let (left, top) = cell(i);
            let (x, y) = (left as f32, top as f32);
            if catalog {
                p.fill_round_rect((x, y, x + 72.0, y + 55.0), 10.0, rgba(0.08, 0.09, 0.126, 1.0))?;
                p.text(
                    &format!("{:04X}", u32::from(ch)),
                    (x + 1.0, y + 35.0, x + 71.0, y + 52.0),
                    9.0,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                    rgba(0.60, 0.64, 0.75, 1.0),
                )?;
            }
            p.glyph(ch, (x + 5.0, y + 3.0, x + 36.0, y + 36.0), 24.0, WHITE)?;
            p.glyph(ch, (x + 43.0, y + 3.0, x + 67.0, y + 36.0), 12.0, rgba(0.74, 0.8, 1.0, 1.0))?;
        }
        Ok(())
    };
    surface.paint(gpu, |p| draw(p, false))?;
    let pixels = surface.pixels();
    for (i, ch) in glyphs.iter().enumerate() {
        let (x, y) = cell(i);
        for (left, right) in [(x, x + 41), (x + 41, x + 72)] {
            let drawn = (y..y + 38)
                .flat_map(|row| (left..right).map(move |col| (row * size.0 as usize + col) * 4 + 3))
                .filter(|&offset| pixels[offset] > 40)
                .count();
            assert!(drawn > 8, "icon U+{:04X} is missing", u32::from(*ch));
        }
    }
    surface.paint(gpu, |p| draw(p, true))?;
    save_surface(&surface, size, "icon-catalog")
}

fn pump() {
    unsafe {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[test]
#[ignore = "Uses synthetic windows to verify the shared island and panel motion"]
fn native_linked_panels_follow_the_same_boundary_through_reversals() -> Result<()> {
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
    let mut panel = TopPanel::new(HWND::default(), &gpu, &TopPanelSettings::default())?;
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for width in [1920.0, 2560.0, 3840.0] {
            let settings = TopPanelSettings {
                enabled: true,
                auto_hide: false,
                reserve_space: false,
                hide_fullscreen: false,
                clear_window_controls: false,
                ..Default::default()
            };
            panel.configure(&settings, scale);
            panel.monitor.work.right = width as i32;
            panel.monitor.bounds.right = width as i32;
            let center = width * 0.5;
            let closed = (center - 120.0 * scale, 8.0 * scale, center + 120.0 * scale, 42.0 * scale);
            let opened = (center - 320.0 * scale, 8.0 * scale, center + 320.0 * scale, 392.0 * scale);
            let now = Instant::now();
            let mut progress = crate::ui::reveal::Reveal::new(0.0);
            progress.retarget(true, 420, crate::data::settings::AnimCurve::Spring, now);
            for (step, showing) in [true, false, true].into_iter().enumerate() {
                if step > 0 {
                    progress.retarget(
                        showing,
                        320,
                        crate::data::settings::AnimCurve::Spring,
                        now + std::time::Duration::from_millis(180 * step as u64),
                    );
                }
                panel.sync_island_motion(Some(IslandMotion { progress: progress.clone(), closed, opened }));
                panel.update(Snapshot::default(), if showing { opened } else { closed }, false);
                if !crate::ui::feedback::motion_allowed() {
                    assert!(panel.left.state.island_motion.is_none());
                    continue;
                }
                for wing in [&mut panel.left, &mut panel.right] {
                    wing.state.travel.show(true, 0, now);
                }
                for ms in (0..600).step_by(7) {
                    let time = now + std::time::Duration::from_millis(ms);
                    let bounds = IslandMotion { progress: progress.clone(), closed, opened }.bounds(time);
                    let left = panel.left.state.position_at(time);
                    let right = panel.right.state.position_at(time);
                    let left_gap = bounds.0 - left.0 - panel.left.state.size.0 as f32;
                    let right_gap = right.0 - bounds.2;
                    if panel.left.state.visible {
                        assert!(
                            (left_gap - 20.0 * scale).abs() < 0.02,
                            "left: {width}, {scale}, {step}, {ms}, {left_gap}"
                        );
                        assert_eq!(panel.left.state.boundary_at(time).unwrap().0, bounds.0);
                    }
                    if panel.right.state.visible {
                        assert!(
                            (right_gap - 20.0 * scale).abs() < 0.02,
                            "right: {width}, {scale}, {step}, {ms}, {right_gap}"
                        );
                        assert_eq!(panel.right.state.boundary_at(time).unwrap().0, bounds.2);
                    }
                    if panel.left.state.visible && panel.right.state.visible {
                        assert!((left.1 - right.1).abs() < 0.01);
                    }
                }
            }
            panel.left.state.hide();
            panel.right.state.hide();
        }
    }
    Ok(())
}

fn expect_right_panel_fades_before_hide(panel: &mut TopPanel) -> Result<()> {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Dwm::DwmFlush;
    use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};

    let work = panel.monitor.work;
    let origin = POINT { x: (work.left + work.right) / 2, y: (work.top + work.bottom) / 2 };
    let right = &mut panel.right;
    let width = right.state.dimensions().0.ceil() as i32;
    let size = (width + 80, 64);
    let backdrop = Window(unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            w!("STATIC"),
            w!("Panel fade test"),
            WS_POPUP,
            origin.x,
            origin.y,
            size.0,
            size.1,
            None,
            None,
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?
    });
    let surface = PopupSurface::new(size)?;
    surface
        .paint(&right.state.gpu, |p| p.fill_rect((0.0, 0.0, size.0 as f32, size.1 as f32), rgba(0.3, 0.7, 0.4, 1.0)))?;
    surface.present(backdrop.0, Some(origin), 255)?;
    unsafe {
        let _ = ShowWindow(backdrop.0, SW_SHOWNOACTIVATE);
    }
    let sample = |x| unsafe {
        pump();
        DwmFlush()?;
        let dc = GetDC(None);
        let pixel = GetPixel(dc, x, origin.y + 27);
        ReleaseDC(None, dc);
        assert_ne!(pixel, COLORREF(u32::MAX));
        Ok::<_, windows::core::Error>(pixel)
    };
    let x = origin.x + 30;
    let background = sample(x)?;
    let expected = COLORREF(0x66b24c);
    let close_to = |a: COLORREF, b: COLORREF, tolerance| {
        [0, 8, 16].into_iter().all(|shift| ((a.0 >> shift) & 255).abs_diff((b.0 >> shift) & 255) <= tolerance)
    };
    assert!(close_to(background, expected, 2), "synthetic background must cover the test pixel");

    right.state.settings.motion_ms = 800;
    right.state.boundary = Some((origin.x as f32, false));
    right.state.departure = (origin.x - width + 50, origin.y);
    let destination = (origin.x + 20, origin.y);
    right.place(destination, 1.0, true);
    right.state.travel.seek(810);
    right.state.motion_frame();
    right.state.render();
    right.place(destination, 1.0, false);
    right.state.stop_frames();
    right.state.reveal.borrow().as_ref().expect("composition reveal").seek(0.4)?;
    assert!(!close_to(sample(x)?, background, 10), "the closing panel must be visible");
    right.state.reveal.borrow().as_ref().expect("composition reveal").seek(0.95)?;
    assert!(close_to(sample(x)?, background, 3), "the panel must be transparent at the end of its travel");
    assert!(right.state.reveal.borrow().is_some(), "the compositor is still drawing the animation window");
    right.state.hide();
    Ok(())
}

#[test]
fn island_panel_corridor_covers_diagonal_crossings_without_covering_the_desktop() {
    let island = (500.0, 8.0, 1100.0, 408.0);
    let right = (1120.0, -2.0, 1400.0, 52.0);
    let left = (200.0, -2.0, 480.0, 52.0);
    for point in [(1110.0, 30.0), (1110.0, 200.0), (1110.0, 400.0)] {
        assert!(across_gap(island, right, point, 12.0));
    }
    assert!(across_gap(island, left, (490.0, 250.0), 12.0));
    assert!(!across_gap(island, right, (1250.0, 250.0), 12.0));
    assert!(!across_gap(island, right, (1110.0, 450.0), 12.0));
}

fn click(wing: &Wing, target: Hit) {
    let button = wing.state.buttons.iter().find(|b| b.hit == target).unwrap();
    let x = (((button.rect.0 + button.rect.2) * 0.5) * wing.state.scale) as u16;
    let y = (((button.rect.1 + button.rect.3) * 0.5) * wing.state.scale) as u16;
    let point = LPARAM((u32::from(x) | (u32::from(y) << 16)) as isize);
    unsafe {
        SendMessageW(wing.state.hwnd, WM_LBUTTONDOWN, Some(WPARAM(0)), Some(point));
        SendMessageW(wing.state.hwnd, WM_LBUTTONUP, Some(WPARAM(0)), Some(point));
    }
}

#[test]
fn top_panel_blocks_can_all_be_removed_and_persist_independently() {
    let mut settings = crate::data::settings::Settings::default();
    for id in [
        crate::view::Sid::TopEnabled,
        crate::view::Sid::TopTray,
        crate::view::Sid::TopAutoHide,
        crate::view::Sid::TopApplication,
        crate::view::Sid::TopWifi,
        crate::view::Sid::TopBluetooth,
        crate::view::Sid::TopVolume,
        crate::view::Sid::TopBattery,
        crate::view::Sid::TopFullscreen,
        crate::view::Sid::TopReserve,
        crate::view::Sid::TopMicrophone,
        crate::view::Sid::TopKeyboard,
        crate::view::Sid::TopWorkspaces,
        crate::view::Sid::TopAnimations,
        crate::view::Sid::TopLiquid,
    ] {
        crate::view::toggle_set(id, &mut settings, false);
        assert!(!crate::view::toggle_get(id, &settings));
        crate::view::toggle_set(id, &mut settings, true);
        assert!(crate::view::toggle_get(id, &settings));
    }
    settings.top_panel.bluetooth = false;
    settings.top_panel.battery = false;
    let encoded = serde_json::to_string(&settings).unwrap();
    let restored: crate::data::settings::Settings = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored.top_panel, settings.top_panel);
    assert_eq!(blocks(&restored.top_panel), [Block::Tray, Block::Wifi, Block::Volume]);
    let old: crate::data::settings::Settings = serde_json::from_str("{}").unwrap();
    assert!(!old.top_panel.enabled);
    assert!(old.top_panel.auto_hide && old.top_panel.tray && old.desktop_dock.replace_taskbar);
    for (index, entrance) in PanelEntrance::ALL.into_iter().enumerate() {
        crate::view::cycle_pick(crate::view::Sid::TopEntrance, &mut settings, index);
        assert_eq!(settings.top_panel.entrance, entrance);
        let restored: crate::data::settings::Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored.top_panel.entrance, entrance);
    }
    let empty = TopPanelSettings {
        tray: false,
        application: false,
        wifi: false,
        bluetooth: false,
        volume: false,
        battery: false,
        ..Default::default()
    };
    assert!(!empty.has_blocks());
}

#[test]
#[ignore = "Shows isolated top-panel widgets; uses synthetic state without changing audio or radios"]
fn native_top_panel_layout_actions_privacy_and_cleanup() -> Result<()> {
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
    if std::env::var_os("NI_TOP_PANEL_PREVIEW").is_some() {
        capture_icon_catalog(&gpu)?;
        let mut ui = crate::view::UiState::new();
        ui.sw.group = crate::view::GROUPS.iter().position(|group| group.name == "Assistant").unwrap();
        let size = (940, 680);
        let surface = PopupSurface::new(size)?;
        use crate::view::AssistantPage;
        crate::ui::i18n::set_ru(true);
        let island_size = (672, 416);
        let island = PopupSurface::new(island_size)?;
        ui.media = crate::platform::media::Media {
            title: "Ночная прогулка".into(),
            artist: "Демонстрация".into(),
            source: "Night Island".into(),
            accent: Some((139, 161, 236)),
            playing: true,
            ..Default::default()
        };
        ui.home.vol = 0.56;
        ui.home.device = "Динамики".into();
        island.paint(&gpu, |p| {
            p.fill_round_rect((16.0, 16.0, 656.0, 400.0), 28.0, rgba(0.028, 0.032, 0.043, 1.0))?;
            crate::view::paint_expanded(p, (16.0, 16.0, 640.0, 384.0), 1.0, &ui, &[])
        })?;
        save_surface(&island, island_size, "icons-island")?;
        ui.media = Default::default();
        let appearance = crate::view::GROUPS.iter().position(|group| group.name == "Appearance").unwrap();
        let assistant = ui.sw.group;
        ui.sw.group = appearance;
        let paint_settings = |p: &Painter, ui: &crate::view::UiState| {
            crate::view::paint_settings_window(p, (size.0 as f32, size.1 as f32), 1.0, ui)
        };
        surface.paint(&gpu, |p| paint_settings(p, &ui))?;
        let modern = surface.pixels();
        save_surface(&surface, size, "icons-modern")?;
        for font in crate::data::settings::InterfaceFont::ALL {
            crate::ui::fonts::set(font);
            ui.cfg.interface_font = font;
            surface.paint(&gpu, |p| paint_settings(p, &ui))?;
            save_surface(&surface, size, &format!("font-settings-{font:?}"))?;
            island.paint(&gpu, |p| {
                p.fill_round_rect((16.0, 16.0, 656.0, 400.0), 28.0, rgba(0.028, 0.032, 0.043, 1.0))?;
                crate::view::paint_expanded(p, (16.0, 16.0, 640.0, 384.0), 1.0, &ui, &[])
            })?;
            save_surface(&island, island_size, &format!("font-island-{font:?}"))?;
        }
        crate::ui::fonts::set(crate::data::settings::InterfaceFont::Segoe);
        ui.cfg.interface_font = crate::data::settings::InterfaceFont::Segoe;
        crate::ui::icons::set_legacy(true);
        crate::view::toggle_set(crate::view::Sid::LegacyIcons, &mut ui.cfg, true);
        let painted = surface.paint(&gpu, |p| paint_settings(p, &ui));
        crate::ui::icons::set_legacy(false);
        painted?;
        let classic = surface.pixels();
        assert!((70..565).any(|row| {
            let offset = (row * size.0 as usize + 24) * 4;
            modern[offset..offset + 20 * 4] != classic[offset..offset + 20 * 4]
        }));
        save_surface(&surface, size, "icons-classic")?;
        ui.cfg.legacy_icons = false;
        ui.sw.group = assistant;
        ui.cfg.ai.voice_separate = true;
        ui.cfg.assistant.natural_voice = true;
        ui.cfg.assistant.tts_provider = crate::services::assistant::TtsProvider::Fish;
        for (page, name) in [
            (AssistantPage::Overview, "assistant-overview"),
            (AssistantPage::Model, "assistant-model"),
            (AssistantPage::Recognition, "assistant-recognition"),
            (AssistantPage::Voice, "assistant-voice"),
            (AssistantPage::Permissions, "assistant-permissions"),
            (AssistantPage::Advanced, "assistant-advanced"),
        ] {
            ui.sw.assistant_page = page;
            surface.paint(&gpu, |p| crate::view::paint_settings_window(p, (size.0 as f32, size.1 as f32), 1.0, &ui))?;
            save_surface(&surface, size, name)?;
        }
        let mut guide = crate::view::GuideUi::new(&ui.cfg, false);
        guide.select(crate::view::Preset::Desktop, false);
        ui.sw.guide = Some(guide);
        surface.paint(&gpu, |p| crate::view::paint_settings_window(p, (size.0 as f32, size.1 as f32), 1.0, &ui))?;
        save_surface(&surface, size, "guide-welcome")?;
        for (mode, count, name) in
            [(crate::view::GuideMode::Quick, 3, "quick"), (crate::view::GuideMode::Detailed, 5, "guided")]
        {
            ui.sw.guide.as_mut().unwrap().choose_mode(mode, false);
            for step in 0..count {
                if step == 1 {
                    ui.sw.guide.as_mut().unwrap().select_access(crate::view::Access::Full);
                }
                surface
                    .paint(&gpu, |p| crate::view::paint_settings_window(p, (size.0 as f32, size.1 as f32), 1.0, &ui))?;
                save_surface(&surface, size, &format!("guide-{name}-{}", step + 1))?;
                if name == "guided" && step == 2 {
                    for lesson in [crate::view::PreviewLesson::Notification, crate::view::PreviewLesson::Pet] {
                        ui.sw.guide.as_mut().unwrap().lesson(lesson, false);
                        surface.paint(&gpu, |p| {
                            crate::view::paint_settings_window(p, (size.0 as f32, size.1 as f32), 1.0, &ui)
                        })?;
                        save_surface(&surface, size, &format!("guide-{lesson:?}"))?;
                    }
                }
                ui.sw.guide.as_mut().unwrap().move_by(1, false);
            }
        }
        ui.sw.guide = None;
        ui.sw.group = crate::view::GROUPS.iter().position(|g| g.name == "System").unwrap();
        ui.sw.memory.open = true;
        ui.sw.memory.snapshot = Some(crate::platform::memory_tools::Snapshot {
            total: 16 * 1024 * 1024 * 1024,
            available: 7 * 1024 * 1024 * 1024,
            available_before: Some(5 * 1024 * 1024 * 1024),
            available_after: Some(8 * 1024 * 1024 * 1024),
            trimmed: 14,
            skipped: 2,
            watching: true,
            apps: (0..8)
                .map(|i| crate::platform::memory_tools::Application {
                    name: ["Browser", "Discord", "Steam", "Editor"][i % 4].into(),
                    path: format!("application-{i}"),
                    minimized: i % 2 == 0,
                    working: (420 - i as u64 * 30) * 1024 * 1024,
                    after: Some(90 * 1024 * 1024),
                    growth: (330 - i as u64 * 30) * 1024 * 1024,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
        ui.sw.memory.selected.insert("application-0".into());
        surface.paint(&gpu, |p| crate::view::paint_settings_window(p, (size.0 as f32, size.1 as f32), 1.0, &ui))?;
        save_surface(&surface, size, "memory-manager")?;
        crate::ui::i18n::set_ru(false);
    }
    let parent = Window(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("STATIC"),
            w!("Top panel fixture"),
            WS_POPUP,
            20,
            200,
            200,
            150,
            None,
            None,
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?
    });
    let settings = TopPanelSettings {
        enabled: true,
        auto_hide: false,
        reserve_space: false,
        hide_fullscreen: false,
        entrance: PanelEntrance::Sides,
        ..Default::default()
    };
    let mut panel = TopPanel::new(parent.0, &gpu, &settings)?;
    let foreground = unsafe { GetForegroundWindow() };
    let window = desktop::Window {
        handle: parent.0.0 as isize,
        path: std::env::current_exe().unwrap().to_string_lossy().into_owned(),
        title: "Документ — рабочая сессия".into(),
        active: true,
        minimized: false,
        island_settings: true,
    };
    let snapshot = Snapshot {
        active: Some(window.clone()),
        windows: vec![window],
        radios: Radios { wifi: Some(true), bluetooth: Some(false) },
        volume: Some(42),
        muted: false,
        device: "Динамики · USB Audio".into(),
        battery: Some((76, true)),
        microphone_muted: Some(false),
        keyboard_layout: "RU".into(),
    };
    for scale in [1.0, 1.5, 2.0] {
        panel.configure(&settings, scale);
        let center = (panel.monitor.work.left + panel.monitor.work.right) as f32 * 0.5;
        let island =
            (center - 100.0 * scale, panel.top() + 8.0 * scale, center + 100.0 * scale, panel.top() + 42.0 * scale);
        panel.update(snapshot.clone(), island, false);
        for wing in [&mut panel.left, &mut panel.right] {
            wing.state.travel.show(true, 0, Instant::now());
            wing.state.motion_frame();
            wing.state.render();
        }
        assert!(panel.left.state.visible && panel.right.state.visible);
        assert!(
            panel.right.state.origin.0 + panel.right.state.size.0 <= panel.monitor.work.right - (144.0 * scale) as i32
        );
        save(&panel.left, &format!("left-{scale:.1}"))?;
        save(&panel.right, &format!("right-{scale:.1}"))?;
        let mut frame_times = Vec::new();
        for _ in 0..40 {
            let started = Instant::now();
            panel.left.state.render();
            panel.right.state.render();
            frame_times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        frame_times.sort_by(f64::total_cmp);
        println!("Two panel frames at scale {scale}: median {:.2}ms, p95 {:.2}ms", frame_times[20], frame_times[38]);
        let right_edge = panel.right.state.origin.0 + panel.right.state.size.0;
        let buffer = panel.right.state.surface.borrow().as_ref().unwrap().storage_id();
        let wifi_x = {
            let button = panel.right.state.buttons.iter().find(|b| b.hit == Hit::Block(Block::Wifi)).unwrap();
            panel.right.state.origin.0 as f32 + button.rect.0 * scale
        };
        panel.right.state.tray_requested = true;
        panel.set_tray_icons(crate::platform::system_tray::inline::preview_entries());
        for elapsed in (0..=240).step_by(20) {
            panel.right.state.tray_started = Instant::now() - std::time::Duration::from_millis(elapsed);
            panel.right.state.tray_frame();
            panel.update(snapshot.clone(), island, false);
            assert!(panel.right.state.visible, "The expanding wing must remain visible at {elapsed} ms");
            assert_eq!(panel.right.state.origin.0 + panel.right.state.size.0, right_edge);
            assert!(panel.right.state.origin.0 as f32 >= island.2 + 8.0 * scale);
            assert_eq!(panel.right.state.surface.borrow().as_ref().unwrap().storage_id(), buffer);
            let button = panel.right.state.buttons.iter().find(|b| b.hit == Hit::Block(Block::Wifi)).unwrap();
            let x = panel.right.state.origin.0 as f32 + button.rect.0 * scale;
            assert!((x - wifi_x).abs() <= 1.0);
            let mut actual = RECT::default();
            unsafe {
                GetWindowRect(panel.right.state.hwnd, &mut actual)?;
            }
            assert_eq!((actual.left, actual.right), (panel.right.state.origin.0, right_edge));
            save(&panel.right, &format!("tray-frame-{scale:.1}-{elapsed}"))?;
        }
        panel.right.state.tray_started = Instant::now() - std::time::Duration::from_secs(1);
        panel.right.state.tray_frame();
        assert_eq!(panel.right.state.origin.0 + panel.right.state.size.0, right_edge);
        assert_eq!(
            panel.right.state.buttons.iter().filter(|b| matches!(b.hit, Hit::TrayIcon(_))).count(),
            panel.right.state.tray_capacity
        );
        save(&panel.right, &format!("inline-tray-{scale:.1}"))?;
        click(&panel.right, Hit::TrayIcon(0));
        assert!(matches!(panel.take_request(1), Some(Request::TrayAction(_, false, _))));
        click(&panel.right, Hit::TrayPage);
        panel.right.state.tray_started = Instant::now() - std::time::Duration::from_secs(1);
        panel.right.state.tray_frame();
        assert_eq!(panel.right.state.tray_page, panel.right.state.tray_capacity);
        panel.open(Block::Tray);
        panel.right.state.tray_started = Instant::now() - std::time::Duration::from_secs(1);
        panel.right.state.tray_frame();
        assert_eq!(panel.right.state.tray_width, 0.0);
        assert_eq!(panel.right.state.origin.0 + panel.right.state.size.0, right_edge);
        panel.open(Block::Tray);
        assert!(panel.right.state.tray_open);
        assert!(matches!(panel.take_request(1), Some(Request::Tray(_))));
        panel.open(Block::Tray);
        panel.set_tray_icons(crate::platform::system_tray::inline::preview_entries());
        assert!(!panel.right.state.tray_open && !panel.right.state.tray_requested);
        panel.right.state.tray_started = Instant::now() - std::time::Duration::from_secs(1);
        panel.right.state.tray_frame();
        for block in [Block::Application, Block::Wifi, Block::Bluetooth, Block::Volume, Block::Battery] {
            panel.open(block);
            assert!(panel.card.state.visible);
            assert_eq!(panel.card.state.size.0, (306.0 * scale) as i32);
            for button in &panel.card.state.buttons {
                let (width, height) = panel.card.state.dimensions();
                assert!(button.rect.0 >= 10.0 && button.rect.2 <= width - 10.0);
                assert!(button.rect.1 >= 10.0 && button.rect.3 <= height - 10.0);
            }
            save(&panel.card, &format!("card-{block:?}-{scale:.1}"))?;
            click(&panel.card, Hit::Close);
            assert!(!panel.card.state.visible);
        }
        panel.tray_help();
        assert!(panel.card.state.visible);
        assert_eq!(panel.card.state.size.0, (338.0 * scale) as i32);
        save(&panel.card, &format!("tray-help-{scale:.1}"))?;
        click(&panel.card, Hit::WindowsSettings);
        assert!(matches!(panel.take_request(2), Some(Request::WindowsSettings(Block::Tray))));
        panel.tray_help();
        click(&panel.card, Hit::TrayRetry);
        assert!(matches!(panel.take_request(2), Some(Request::Open(Block::Tray))));
        panel.open(Block::Volume);
        click(&panel.card, Hit::Mute);
        assert!(matches!(panel.take_request(2), Some(Request::Mute)));
        click(&panel.card, Hit::Volume);
        assert!(matches!(panel.take_request(2), Some(Request::Volume(_))));
        assert_ne!(unsafe { GetCapture() }, panel.card.state.hwnd);
        panel.set_ghost(0.2, true);
        for wing in [&panel.left, &panel.right, &panel.card] {
            assert!(wing.state.click_through);
            assert_eq!(wing.state.ghost_alpha, 51);
            assert_ne!(unsafe { GetWindowLongPtrW(wing.state.hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize, 0);
            assert_eq!(
                unsafe { SendMessageW(wing.state.hwnd, WM_NCHITTEST, None, Some(LPARAM(0))) }.0,
                HTTRANSPARENT as isize
            );
        }
        panel.set_ghost(1.0, false);
        for wing in [&panel.left, &panel.right, &panel.card] {
            assert_eq!(wing.state.ghost_alpha, 255);
            assert_eq!(unsafe { GetWindowLongPtrW(wing.state.hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize, 0);
        }
        panel.set_capture_hidden(true);
        for wing in [&panel.left, &panel.right, &panel.card] {
            let mut affinity = 0;
            unsafe {
                GetWindowDisplayAffinity(wing.state.hwnd, &mut affinity)?;
            }
            assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
        }
        panel.set_capture_hidden(false);
        panel.update(snapshot.clone(), island, true);
        assert!(!panel.left.state.visible && !panel.right.state.visible && !panel.card.state.visible);
        panel.update(
            snapshot.clone(),
            (panel.monitor.work.left as f32, panel.top(), panel.monitor.work.right as f32, panel.top() + 400.0),
            false,
        );
        assert!(!panel.left.state.visible && !panel.right.state.visible);
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    }
    for entrance in PanelEntrance::ALL {
        for scale in [1.0, 1.5, 2.0] {
            let mut config = settings.clone();
            config.entrance = entrance;
            panel.left.state.hide();
            panel.right.state.hide();
            panel.configure(&config, scale);
            let center = (panel.monitor.work.left + panel.monitor.work.right) as f32 / 2.0;
            let island =
                (center - 100.0 * scale, panel.top() + 8.0 * scale, center + 100.0 * scale, panel.top() + 42.0 * scale);
            panel.update(snapshot.clone(), island, false);
            let mut previous = [0u8; 2];
            if entrance == PanelEntrance::Island {
                if crate::ui::feedback::motion_allowed() {
                    assert!(panel.left.state.revealing.get() && panel.right.state.revealing.get());
                }
                assert!(panel.interacting_at(
                    island,
                    (
                        (panel.right.state.destination.0 + panel.right.state.size.0 - 20) as f32,
                        panel.right.state.destination.1 as f32 + 25.0 * scale,
                    )
                ));
            }
            for elapsed in [0, 90, 180, 270, 400] {
                for (index, wing) in [&mut panel.left, &mut panel.right].into_iter().enumerate() {
                    wing.state.travel.seek(elapsed);
                    wing.state.motion_frame();
                    wing.state.render();
                    assert!(wing.state.alpha >= previous[index]);
                    previous[index] = wing.state.alpha;
                    if elapsed == 400 {
                        assert_eq!(wing.state.origin, wing.state.destination);
                        assert!(!wing.state.opening);
                        assert!(!wing.state.revealing.get());
                        assert!(wing.state.reveal.borrow().is_none());
                    }
                }
            }
            if entrance == PanelEntrance::Island {
                let original = panel.right.state.destination;
                panel.right.place((original.0 + 40, original.1), scale, true);
                assert!((panel.right.state.origin.0 - original.0).abs() <= 1);
                assert!(panel.animating());
                panel.right.state.relocation.seek(400);
                panel.right.state.motion_frame();
                assert_eq!(panel.right.state.origin.0, original.0 + 40);
                panel.right.place(original, scale, true);
                panel.right.state.relocation.seek(400);
                panel.right.state.motion_frame();
                assert_eq!(panel.right.state.origin, original);
                let tray = &panel.right.state;
                let button_point = (tray.origin.0 as f32 + 25.0 * scale, tray.origin.1 as f32 + 25.0 * scale);
                assert!(panel.interacting_at(island, button_point));
                assert!(panel.interacting_at(island, (island.2 + 5.0 * scale, island.1 + 20.0 * scale)));
                panel.set_ghost(0.2, true);
                assert!(!panel.interacting_at(island, button_point));
                panel.set_ghost(1.0, false);
                assert!(
                    (panel.left.state.origin.0 as f32 + panel.left.state.size.0 as f32
                        - (island.0 - config.island_gap as f32 * scale))
                        .abs()
                        <= 1.0
                );
                assert!(
                    (panel.right.state.origin.0 as f32 - (island.2 + config.island_gap as f32 * scale)).abs() <= 1.0
                );
                let right_start = panel.right.state.origin.0;
                let wifi =
                    panel.right.state.buttons.iter().find(|button| button.hit == Hit::Block(Block::Wifi)).unwrap().rect;
                panel.right.state.tray_requested = true;
                panel.set_tray_icons(crate::platform::system_tray::inline::preview_entries());
                for elapsed in [0, 80, 160, 240] {
                    panel.right.state.tray_started = Instant::now() - std::time::Duration::from_millis(elapsed);
                    panel.right.state.tray_frame();
                    panel.update(snapshot.clone(), island, false);
                    assert_eq!(panel.right.state.origin.0, right_start);
                    assert_eq!(
                        panel
                            .right
                            .state
                            .buttons
                            .iter()
                            .find(|button| button.hit == Hit::Block(Block::Wifi))
                            .unwrap()
                            .rect,
                        wifi
                    );
                    assert!(
                        panel.right.state.origin.0 + panel.right.state.size.0
                            <= panel.monitor.work.right - (144.0 * scale) as i32
                    );
                }
                save(&panel.right, &format!("island-tray-{scale:.1}"))?;
            }
            save(&panel.left, &format!("{entrance:?}-left-{scale:.1}"))?;
            save(&panel.right, &format!("{entrance:?}-right-{scale:.1}"))?;
        }
    }
    panel.left.state.hide();
    panel.right.state.hide();
    let mut close_settings = settings.clone();
    close_settings.entrance = PanelEntrance::Island;
    panel.configure(&close_settings, 1.0);
    let center = (panel.monitor.work.left + panel.monitor.work.right) as f32 / 2.0;
    let island = (center - 100.0, panel.top() + 8.0, center + 100.0, panel.top() + 42.0);
    panel.update(snapshot.clone(), island, false);
    let right = &mut panel.right;
    assert!(right.state.visible);
    right.state.travel.seek(u64::from(close_settings.motion_ms / 2));
    right.state.motion_frame();
    right.place(right.state.destination, 1.0, false);
    right.state.travel.seek(u64::from(close_settings.motion_ms) + 1);
    unsafe { SendMessageW(right.state.hwnd, WM_TIMER, Some(WPARAM(FRAME)), None) };
    assert!(!right.state.visible);
    assert!(!right.state.revealing.get());
    assert!(right.state.reveal.borrow().is_none());
    assert!(!unsafe { IsWindowVisible(right.state.hwnd) }.as_bool());
    panel.left.state.hide();
    if crate::ui::feedback::motion_allowed() {
        expect_right_panel_fades_before_hide(&mut panel)?;
    }
    let mut only_volume = settings.clone();
    only_volume.animations = false;
    struct Cursor(POINT);
    impl Drop for Cursor {
        fn drop(&mut self) {
            unsafe {
                let _ = SetCursorPos(self.0.x, self.0.y);
            }
        }
    }
    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point)?;
    }
    let _cursor = Cursor(point);
    let mut auto = settings.clone();
    auto.animations = false;
    auto.auto_hide = true;
    panel.configure(&auto, 1.0);
    let outside = (panel.monitor.bounds.left + 80, panel.monitor.bounds.top + 400);
    unsafe {
        SetCursorPos(outside.0, outside.1)?;
    }
    panel.update(snapshot.clone(), (-1000.0, -1000.0, -900.0, -900.0), false);
    assert!(!panel.left.state.visible && !panel.right.state.visible);
    unsafe {
        SetCursorPos(panel.monitor.bounds.left + 400, panel.monitor.bounds.top + 1)?;
    }
    panel.update(snapshot.clone(), (-1000.0, -1000.0, -900.0, -900.0), false);
    assert!(panel.left.state.visible && panel.right.state.visible);
    unsafe {
        SetCursorPos(panel.right.state.origin.0 + 40, panel.right.state.origin.1 + 25)?;
    }
    panel.last_inside = Instant::now() - std::time::Duration::from_secs(2);
    panel.update(snapshot.clone(), (-1000.0, -1000.0, -900.0, -900.0), false);
    assert!(panel.right.state.visible);
    panel.open(Block::Volume);
    unsafe {
        SetCursorPos(panel.card.state.origin.0 + 40, panel.card.state.origin.1 + 60)?;
    }
    panel.last_inside = Instant::now() - std::time::Duration::from_secs(2);
    panel.update(snapshot.clone(), (-1000.0, -1000.0, -900.0, -900.0), false);
    assert!(panel.card.state.visible && panel.right.state.visible);
    unsafe {
        SetCursorPos(outside.0, outside.1)?;
    }
    panel.last_inside = Instant::now() - std::time::Duration::from_secs(2);
    panel.update(snapshot.clone(), (-1000.0, -1000.0, -900.0, -900.0), false);
    assert!(!panel.card.state.visible && !panel.right.state.visible);
    only_volume.application = false;
    only_volume.tray = false;
    only_volume.wifi = false;
    only_volume.bluetooth = false;
    only_volume.battery = false;
    panel.configure(&only_volume, 1.0);
    panel.update(snapshot.clone(), (-1000.0, -1000.0, -900.0, -900.0), false);
    assert!(!panel.left.state.visible && panel.right.state.visible);
    assert_eq!(panel.right.state.buttons.len(), 1);
    let mut extra = only_volume.clone();
    extra.auto_hide = false;
    extra.volume = false;
    extra.microphone = true;
    extra.keyboard = true;
    extra.workspaces = true;
    panel.configure(&extra, 1.0);
    panel.update(snapshot.clone(), (-1000.0, -1000.0, -900.0, -900.0), false);
    assert!(panel.left.state.visible && !panel.right.state.visible);
    save(&panel.left, "extra-controls")?;
    click(&panel.left, Hit::Block(Block::Microphone));
    assert!(matches!(panel.take_request(0), Some(Request::Microphone)));
    click(&panel.left, Hit::Block(Block::Keyboard));
    assert!(matches!(panel.take_request(0), Some(Request::Keyboard)));
    click(&panel.left, Hit::Block(Block::Workspaces));
    assert!(matches!(panel.take_request(0), Some(Request::Workspaces)));
    let mut unavailable = snapshot;
    unavailable.microphone_muted = None;
    panel.update(unavailable, (-1000.0, -1000.0, -900.0, -900.0), false);
    click(&panel.left, Hit::Block(Block::Microphone));
    assert!(panel.take_request(0).is_none());
    let windows = [panel.left.state.hwnd, panel.right.state.hwnd, panel.card.state.hwnd];
    drop(panel);
    pump();
    for hwnd in windows {
        assert!(!unsafe { IsWindow(Some(hwnd)).as_bool() });
    }
    Ok(())
}

#[test]
#[ignore = "Briefly reserves and restores desktop work area using a native appbar"]
fn native_top_panel_space_is_reserved_once_and_restored() -> Result<()> {
    let before = desktop::monitors().into_iter().find(|m| m.primary).unwrap();
    let parent = Window(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("STATIC"),
            w!("Top panel space fixture"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?
    });
    let mut reservation = super::super::top_panel_space::Reservation::new(parent.0, &before, 50)?;
    let after = desktop::monitors().into_iter().find(|m| m.primary).unwrap();
    assert!(
        after.work.top >= before.work.top + 50,
        "Work area must leave room for the panel: {:?} -> {:?}",
        before.work,
        after.work
    );
    let first = reservation.rect();
    for _ in 0..3 {
        reservation.refresh(&after, 50);
    }
    assert_eq!(reservation.rect(), first);
    drop(reservation);
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    loop {
        pump();
        let restored = desktop::monitors().into_iter().find(|m| m.primary).unwrap();
        if restored.work == before.work {
            break;
        }
        assert!(Instant::now() < deadline, "Work area must be restored: {:?} -> {:?}", before.work, restored.work);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(first.bottom - first.top == 50 && first != RECT::default());
    Ok(())
}
