use super::*;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::WinRT::RoUninitialize;

struct NativeWindow(HWND);

impl Drop for NativeWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

fn window(ex: WINDOW_EX_STYLE) -> Result<NativeWindow> {
    Ok(NativeWindow(unsafe {
        CreateWindowExW(
            ex | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("STATIC"),
            w!("Island motion fixture"),
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
    }))
}

fn seek(app: &App, progress: f32) -> Result<()> {
    for property in [h!("Offset"), h!("Size"), h!("CornerRadius")] {
        let controller = app.geometry.TryGetAnimationController(property)?;
        controller.Pause()?;
        controller.SetProgress(progress)?;
    }
    for sprite in [&app.tint, &app.expanded.sprite, &app.collapsed.sprite, &app.pet.char_layer.sprite] {
        let controller = sprite.TryGetAnimationController(h!("Opacity"))?;
        controller.Pause()?;
        controller.SetProgress(progress)?;
    }
    for sprite in std::iter::once(&app.progress_fill.sprite).chain(app.ears.iter().map(|ear| &ear.sprite)) {
        if let Ok(controller) = sprite.TryGetAnimationController(h!("Opacity")) {
            controller.Pause()?;
            controller.SetProgress(progress)?;
        }
    }
    for property in [h!("Scale"), h!("Offset")] {
        let controller = app.expanded.sprite.TryGetAnimationController(property)?;
        controller.Pause()?;
        controller.SetProgress(progress)?;
    }
    if app.ui.cover_in_flight {
        for property in [h!("Offset"), h!("Size")] {
            let controller = app.cover_layer.sprite.TryGetAnimationController(property)?;
            controller.Pause()?;
            controller.SetProgress(progress)?;
        }
    }
    unsafe {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        DwmFlush()?;
        std::thread::sleep(Duration::from_millis(25));
        DwmFlush()?;
    }
    Ok(())
}

fn capture(app: &App, frame: usize) -> Result<()> {
    let Some(folder) = std::env::var_os("NI_OPEN_MOTION_PREVIEW") else { return Ok(()) };
    let folder = std::path::PathBuf::from(folder).join(if app.docked() { "docked" } else { "floating" });
    let size = app.window_size(Win::Big);
    let (x, y) = app.window_origin(Win::Big);
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
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let previous = SelectObject(dc, bitmap.into());
        let copied =
            BitBlt(dc, 0, 0, size.0, size.1, Some(screen), x.round() as i32, y.round() as i32, SRCCOPY | CAPTUREBLT);
        let mut pixels = std::slice::from_raw_parts(bits.cast::<u8>(), (size.0 * size.1 * 4) as usize).to_vec();
        SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        ReleaseDC(None, screen);
        copied?;
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        assert!(
            pixels.chunks_exact(4).any(|pixel| pixel[0] < 20 && pixel[1] < 20 && pixel[2] < 20),
            "The native island must be visible in captured frame {frame}"
        );
        std::fs::create_dir_all(&folder).unwrap();
        let mut png = png::Encoder::new(
            std::fs::File::create(folder.join(format!("island-{frame:03}.png"))).unwrap(),
            size.0 as u32,
            size.1 as u32,
        );
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    Ok(())
}

#[test]
#[ignore = "Uses an isolated Windows compositor and synthetic island windows"]
fn native_island_open_close_retarget_dpi_and_content() -> Result<()> {
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
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for edge in Edge::ALL {
            let owner = window(WS_EX_NOREDIRECTIONBITMAP)?;
            let settings = Settings {
                placement: Placement::Docked,
                dock_edge: edge,
                character: Character::None,
                music_cover: true,
                style: Style::Flat,
                ..Default::default()
            };
            let mut app = App::new_visuals(owner.0, scale, settings)?;
            app.ui.cfg = app.settings.clone();
            app.place(Win::Big);
            app.set_geometry_now(Win::Big, false, false)?;
            app.state = State::Expanded;
            let start = Instant::now();
            app.animate_to(true, false)?;
            assert!(start.elapsed() < Duration::from_millis(250));
            for progress in [0.0, 0.25, 0.6, 1.0] {
                seek(&app, progress)?;
            }
            app.state = State::Collapsing;
            app.animate_to(false, false)?;
            let before = app.open_motion.as_ref().unwrap().progress.value(Instant::now());
            app.state = State::Expanded;
            app.animate_to(true, false)?;
            let after = app.open_motion.as_ref().unwrap().progress.frame(0.0);
            assert!((before - after).abs() < 0.03);
            seek(&app, 1.0)?;
            app.kill_timer(Timer::Expand);
            app.set_timer(Timer::Expand, 100);
            app.set_timer(Timer::Expand, 5000);
            std::thread::sleep(Duration::from_millis(130));
            let mut message = MSG::default();
            assert!(unsafe { PeekMessageW(&mut message, Some(owner.0), WM_TIMER, WM_TIMER, PM_REMOVE).as_bool() });
            assert_eq!(message.wParam.0, Timer::Expand as usize);
            app.kill_timer(Timer::Expand);
            app.settings.island_animations = false;
            app.animate_to(false, false)?;
            assert_eq!(app.transition_ms(false), 1);
            assert_eq!(app.open_motion.as_ref().unwrap().progress.value(Instant::now()), 0.0);
            app.animate_to(true, false)?;
            assert_eq!(app.open_content_point(300, 100), Some((300, 100)));
            app.open_motion = None;
        }
    }
    for placement in [Placement::Docked, Placement::Floating] {
        let background = window(WS_EX_LAYERED | WS_EX_TOPMOST)?;
        let owner = window(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST)?;
        let (screen_w, screen_h) = App::screen_size();
        let settings = Settings {
            placement,
            float_pos: Some([(screen_w * 0.5) as i32, (screen_h * 0.25) as i32]),
            style: Style::Flat,
            character: Character::None,
            music_cover: true,
            music_eq: false,
            music_lyrics: false,
            edge_spikes: false,
            auto_hide: false,
            ..Default::default()
        };
        let mut app = App::new_visuals(owner.0, 1.0, settings)?;
        app.ui.cfg = app.settings.clone();
        app.ui.media = media::Media {
            title: "Midnight City".into(),
            artist: "M83".into(),
            playing: true,
            source: "Motion preview".into(),
            cover: media::debug_cover(16, &[72, 140, 230, 255].repeat(256)),
            ..Default::default()
        };
        media::debug_progress(65.0, 100.0);
        app.cw = view::collapsed_width(&app.ui, COLLAPSED.0);
        app.collapsed.resize(window_size(app.scale, Win::Small, app.docked(), app.cw, &window_room(&app.settings)))?;
        assert_eq!(app.window_origin(Win::Small), app.window_origin(Win::Big));
        assert_eq!(app.window_size(Win::Small), app.window_size(Win::Big));
        app.place(Win::Big);
        app.set_geometry_now(Win::Big, false, false)?;
        app.state = State::Expanded;
        app.repaint();
        let size = app.window_size(Win::Big);
        let (x, y) = app.window_origin(Win::Big);
        let surface = crate::ui::popup::PopupSurface::new(size)?;
        surface.paint(&app.gpu, |p| {
            p.fill_rect((0.0, 0.0, size.0 as f32, size.1 as f32), crate::ui::gfx::rgba(0.14, 0.21, 0.29, 1.0))
        })?;
        surface.present(background.0, Some(POINT { x: x.round() as i32, y: y.round() as i32 }), 255)?;
        unsafe {
            let _ = ShowWindow(background.0, SW_SHOWNOACTIVATE);
            SetWindowPos(
                owner.0,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )?;
        }
        app.animate_to(true, false)?;
        app.start_cover_motion(true);
        for frame in 0..=30 {
            seek(&app, frame as f32 / 30.0)?;
            capture(&app, frame)?;
        }
        app.finish_cover_motion();
        app.collapse();
        for frame in 0..=24 {
            seek(&app, frame as f32 / 24.0)?;
            capture(&app, 31 + frame)?;
        }
        let before = app.pill_screen(false);
        app.shrink_window();
        assert_eq!(before, app.pill_screen(false));
        for property in [h!("Offset"), h!("Size"), h!("CornerRadius")] {
            assert!(app.geometry.TryGetAnimationController(property).is_err());
        }
        for frame in 0..10 {
            unsafe {
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                DwmFlush()?;
                std::thread::sleep(Duration::from_millis(25));
                DwmFlush()?;
            }
            capture(&app, 56 + frame)?;
            let dc = unsafe { GetDC(None) };
            for dx in [4.0, 12.0, 35.0] {
                let color = unsafe {
                    GetPixel(dc, (before.0 + before.2 + dx).round() as i32, (before.1 + before.3 * 0.5).round() as i32)
                };
                assert!((color.0 & 255) > 25, "The island stretched right after collapse, frame {frame}: {color:?}");
            }
            unsafe {
                ReleaseDC(None, dc);
            }
        }
        let outside = POINT { x: (before.0 + before.2 * 0.5) as i32, y: (before.1 + before.3 + 120.0) as i32 };
        assert_ne!(unsafe { WindowFromPoint(outside) }, owner.0, "The empty canvas must not intercept other windows");
        app.settings.character = Character::Spark;
        app.ui.char_inset = CHAR_INSET;
        app.ui.weather = Some(crate::services::weather::Weather {
            place: "Preview".into(),
            temp: 4.0,
            wind: 2.0,
            humidity: 50.0,
            sky: crate::services::weather::Sky::Overcast,
            night: false,
            hourly: Vec::new(),
            daily: Vec::new(),
            city_missing: false,
        });
        app.settings.style = Style::Liquid;
        app.settings.edge_spikes = true;
        app.settings.spike_color = true;
        app.settings.pill_blur = false;
        app.apply_layout();
        let size = app.window_size(Win::Big);
        let origin = app.window_origin(Win::Big);
        let backdrop = crate::ui::popup::PopupSurface::new(size)?;
        backdrop.paint(&app.gpu, |p| {
            p.fill_rect((0.0, 0.0, size.0 as f32, size.1 as f32), crate::ui::gfx::rgba(0.14, 0.21, 0.29, 1.0))
        })?;
        backdrop.present(background.0, Some(POINT { x: origin.0.round() as i32, y: origin.1.round() as i32 }), 255)?;
        app.paint_progress_fill();
        app.pet.char_layer.paint(&app.gpu, |p| {
            let _ = characters::draw_attached(
                p,
                Character::Spark,
                characters::BOX_W * app.scale,
                &characters::Pose {
                    t: 0.0,
                    reaction: characters::Reaction::Idle,
                    look: (0.0, 0.0),
                    blink: false,
                    accent: (0.6, 0.7, 0.9),
                    beat: 0.0,
                    motion: Default::default(),
                },
                None,
            );
        })?;
        app.pet.char_layer.sprite.SetIsVisible(true)?;
        let body = app.pill_rect(false);
        app.liquid.pull_ready = true;
        app.liquid.bump_amp = 24.0;
        app.liquid.pull = crate::ui::gfx::ElasticPull {
            point: (body.0 + body.2 * 0.6, body.1 + body.3),
            normal: (0.0, 1.0),
            spread: 44.0,
            depth: 24.0,
        };
        for (index, style) in [
            crate::data::settings::EqStyle::Spikes,
            crate::data::settings::EqStyle::Wave,
            crate::data::settings::EqStyle::Bars,
        ]
        .into_iter()
        .enumerate()
        {
            app.settings.eq_style = style;
            app.paint_edge_levels(&[0.25, 0.6, 0.4, 0.75, 0.65, 0.5, 0.75, 0.5, 0.2]);
            assert!(app.liquid.body_active.get());
            assert!(!app.liquid.bump.sprite.IsVisible()?);
            let _ = app.compositor.RequestCommitAsync();
            unsafe {
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                DwmFlush()?;
                std::thread::sleep(Duration::from_millis(25));
                DwmFlush()?;
            }
            capture(&app, 70 + index)?;
        }
        app.ui.media.accent = Some((183, 140, 77));
        media::debug_progress(20.0, 100.0);
        app.paint_progress_fill();
        let (at, size, radius) = app.pill(app.win, false, false);
        let rect = (at.X, at.Y, size.X, size.Y);
        let cursors = [
            (at.X - 5.0, at.Y + size.Y * 0.5),
            (at.X - 5.0, at.Y + size.Y + 5.0),
            (at.X + 10.0, at.Y + size.Y + 10.0),
            (at.X + size.X + 5.0, at.Y + size.Y * 0.5),
            (at.X + size.X + 5.0, at.Y + size.Y + 5.0),
            (at.X + size.X - 10.0, at.Y + size.Y + 10.0),
        ];
        for (index, cursor) in cursors.into_iter().enumerate() {
            app.liquid.pull = crate::ui::gfx::cursor_pull(rect, radius.X, cursor, 160.0, 40.0, None);
            app.liquid.bump_amp = 40.0;
            app.settings.eq_style = crate::data::settings::EqStyle::Spikes;
            app.paint_edge_levels(&[0.25, 0.6, 0.4, 0.75, 0.65, 0.5, 0.75, 0.5, 0.2]);
            assert!(app.liquid.highlight.sprite.IsVisible()?, "Body fill must retain the continuous edge gradient");
            let _ = app.compositor.RequestCommitAsync();
            unsafe {
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                DwmFlush()?;
                std::thread::sleep(Duration::from_millis(30));
                DwmFlush()?;
            }
            capture(&app, 80 + index)?;
        }
        app.ui.cfg.music_progress = false;
        app.paint_edge_levels(&[0.3; 9]);
        assert!(app.liquid.highlight.sprite.IsVisible()?, "Equalizer colour remains available without body fill");
        app.ui.cfg.music_progress = true;
        app.settings.bump_tint = crate::data::settings::BumpTint::Off;
        app.paint_edge_levels(&[0.3; 9]);
        assert!(!app.liquid.highlight.sprite.IsVisible()?, "Disabling liquid edge colour must remove its gradient");
        app.settings.bump_tint = crate::data::settings::BumpTint::Equalizer;
        app.liquid.pull_ready = false;
        app.liquid.bump_amp = 0.0;
        app.settings.eq_style = crate::data::settings::EqStyle::Bars;
        for (profile, response) in crate::data::settings::EqResponse::ALL.into_iter().enumerate() {
            app.settings.eq_response = response;
            app.music.edge_values.set([0.0; audio::BANDS]);
            let mut peak = 0.0_f32;
            for frame in 0..24 {
                audio::debug_levels(std::array::from_fn(|band| {
                    if (4..8).contains(&frame) { if band < 10 { 1.0 } else { 0.65 } } else { 0.08 }
                }));
                app.music.edge_time.set(Instant::now() - Duration::from_millis(16));
                app.paint_spikes();
                peak = peak.max(app.music.edge_values.get()[4]);
                if [0, 6, 9, 13, 23].contains(&frame) {
                    let _ = app.compositor.RequestCommitAsync();
                    unsafe {
                        let mut message = MSG::default();
                        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                        DwmFlush()?;
                        std::thread::sleep(Duration::from_millis(20));
                        DwmFlush()?;
                    }
                    capture(&app, 100 + profile * 30 + frame)?;
                }
            }
            if response != crate::data::settings::EqResponse::Calm {
                assert!(peak > 0.9, "Short beats must be visible in the responsive profiles");
                assert!(app.music.edge_values.get()[4] < 0.2, "The edge must settle before the next beat");
            }
        }
        audio::debug_levels([0.0; audio::BANDS]);
        app.settings.eq_response = crate::data::settings::EqResponse::Responsive;
        app.music.spectrum_running = true;
        app.liquid.bump_amp = 0.0;
        app.liquid.bump_vel = 0.0;
        app.paint_spikes();
        assert!(app.liquid.body_active.get());
        app.liquid_tick();
        assert!(app.liquid.body_active.get(), "The liquid timer must preserve the audio silhouette");
        app.set_decoration_through(false);
        app._target.SetRoot(None::<&windows::UI::Composition::Visual>)?;
        app.compositor.Close()?;
    }
    Ok(())
}

#[test]
#[ignore = "Checks live activity transitions and expanded equalizer using isolated native surfaces"]
fn native_live_activities_preserve_music_and_open_equalizer_is_smoothed() -> Result<()> {
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
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for placement in [Placement::Floating, Placement::Docked] {
            let owner = window(WS_EX_NOREDIRECTIONBITMAP)?;
            let settings = Settings {
                placement,
                character: Character::None,
                music_eq: false,
                eq_smooth: true,
                eq_response: crate::data::settings::EqResponse::Calm,
                eq_when_open: true,
                edge_spikes: true,
                music_track_intro: true,
                ..Settings::default()
            };
            let mut app = App::new_visuals(owner.0, scale, settings)?;
            let now = chrono::Local::now().timestamp();
            app.reminders = vec![
                Reminder { id: 17, due: now + 300, text: "Чай".into() },
                Reminder { id: 19, due: now + 400, text: "Проверить духовку".into() },
            ];
            app.ui.media.playing = true;
            app.ui.media.title = "New track".into();
            app.ui.media.artist = "Preview".into();
            app.ui.media.accent = Some((110, 150, 230));
            app.sync_live_activities();
            assert_eq!(app.ui.live.len(), 2);
            let content = super::pill_motion::PillContent::current(&app.ui);
            app.reminders[0].due -= 1;
            app.sync_live_activities();
            assert!(super::pill_motion::PillContent::current(&app.ui) == content);
            app.maybe_start_music_intro();
            assert!(app.pill_motion.intro.is_none());
            app.settings.live_activities = false;
            app.sync_live_activities();
            assert!(app.ui.live.is_empty());
            app.settings.live_activities = true;
            app.sync_live_activities();
            app.state = State::Expanded;
            app.win = Win::Big;
            app.music.spectrum_running = true;
            app.music.edge_time.set(Instant::now() - Duration::from_millis(16));
            audio::debug_levels([1.0; audio::BANDS]);
            app.paint_open_eq();
            let first = app.music.edge_values.get()[0];
            assert!(first > 0.0 && first < 0.8, "Open equalizer jumped to {first}");
            app.music.edge_time.set(Instant::now() - Duration::from_millis(16));
            audio::debug_levels([0.0; audio::BANDS]);
            app.paint_open_eq();
            let second = app.music.edge_values.get()[0];
            assert!(second > 0.0 && second < first);
            assert!(app.liquid.ring.sprite.IsVisible()?);
            if scale == 1.0
                && placement == Placement::Floating
                && let Some(folder) = std::env::var_os("NI_ACTIVITY_REVIEW")
            {
                let folder = std::path::PathBuf::from(folder);
                std::fs::create_dir_all(&folder).unwrap();
                app.ui.live_changed = None;
                for (name, size, settings) in [("activities", (760, 520), false), ("sounds", (940, 760), true)] {
                    let surface = crate::ui::popup::PopupSurface::new(size)?;
                    if settings {
                        app.ui.sw.group = view::GROUPS.iter().position(|g| g.name == "Sounds").unwrap();
                    }
                    surface.paint(&app.gpu, |p| {
                        p.fill_rect(
                            (0.0, 0.0, size.0 as f32, size.1 as f32),
                            crate::ui::gfx::rgba(0.03, 0.04, 0.07, 1.0),
                        )?;
                        if settings {
                            view::paint_settings_window(p, (940.0, 760.0), 1.0, &app.ui)
                        } else {
                            let width = view::collapsed_width(&app.ui, 200.0);
                            let left = (760.0 - width) / 2.0;
                            p.fill_round_rect(
                                (left, 30.0, left + width, 64.0),
                                17.0,
                                crate::ui::gfx::rgba(0.0, 0.0, 0.0, 1.0),
                            )?;
                            view::paint_collapsed(p, (left, 30.0, width, 34.0), 1.0, &app.ui)?;
                            view::paint_expanded(p, (60.0, 100.0, 640.0, 380.0), 1.0, &app.ui, &[])
                        }
                    })?;
                    let bytes: Vec<u8> =
                        surface.pixels().chunks_exact(4).flat_map(|v| [v[2], v[1], v[0], v[3]]).collect();
                    let mut encoder = png::Encoder::new(
                        std::fs::File::create(folder.join(format!("{name}.png"))).unwrap(),
                        size.0 as u32,
                        size.1 as u32,
                    );
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder.write_header().unwrap().write_image_data(&bytes).unwrap();
                }
            }
            app.music.spectrum_running = false;
            audio::debug_levels([0.0; audio::BANDS]);
        }
    }
    Ok(())
}

#[test]
#[ignore = "Uses native Composition surfaces and an isolated settings profile"]
fn native_widget_resize_commits_on_release_and_cancels_without_saving() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_SINGLETHREADED)?;
    }
    let queue = unsafe {
        CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        })?
    };
    {
        let owner = window(WS_EX_NOREDIRECTIONBITMAP)?;
        let settings = Settings {
            home_actions: vec![0, 1],
            home_weather_widget: false,
            island_animations: false,
            character: Character::None,
            ..Settings::default()
        };
        let mut app = App::new_visuals(owner.0, 1.0, settings)?;
        app.state = State::Expanded;
        app.win = Win::Big;
        app.toggle_widget_editor();
        app.begin_widget_resize(0, 0, 0);
        assert!(app.drag_widget_resize(132, 54));
        assert_eq!(app.ui.home.widgets.preview, Some((0, crate::data::home_widgets::WidgetSize::Wide4)));
        assert!(app.settings.home_widget_sizes.is_empty());
        assert!(app.cancel_widget_resize());
        assert!(app.settings.home_widget_sizes.is_empty());
        assert!(app.ui.home.widgets.preview.is_none());
        app.begin_widget_resize(0, 0, 0);
        app.drag_widget_resize(0, 54);
        assert!(app.finish_widget_resize());
        assert_eq!(app.settings.home_widget_sizes.get(&0), Some(&crate::data::home_widgets::WidgetSize::Tall2));
        let mut restored: Settings = serde_json::from_str(&serde_json::to_string(&app.settings).unwrap()).unwrap();
        restored.sanitize();
        assert_eq!(restored.home_widget_sizes, app.settings.home_widget_sizes);
        app.toggle_widget_editor();
        assert!(!app.ui.home.widgets.editing);
        let surface = crate::ui::popup::PopupSurface::new((32, 32))?;
        surface.paint(&app.gpu, |p| {
            p.fill_rect((0.0, 0.0, 32.0, 32.0), crate::ui::gfx::rgba(0.0, 0.0, 0.0, 1.0))?;
            p.with_opacity((0.0, 0.0, 32.0, 32.0), 0.5, || {
                p.fill_rect((0.0, 0.0, 32.0, 32.0), crate::ui::gfx::rgba(1.0, 0.0, 0.0, 1.0))
            })
        })?;
        let pixels = surface.pixels();
        let red = pixels[(16 * 32 + 16) * 4 + 2];
        assert!((120..=136).contains(&red), "Widget opacity layer rendered {red}");
    }
    drop(queue);
    unsafe {
        RoUninitialize();
    }
    Ok(())
}

#[test]
#[ignore = "Exercises native widget dragging, live layout and capture cancellation"]
fn native_widget_drag_previews_then_commits_or_cancels_at_every_scale() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_SINGLETHREADED)?;
    }
    let queue = unsafe {
        CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        })?
    };
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for placement in [Placement::Floating, Placement::Docked] {
            let owner = window(WS_EX_NOREDIRECTIONBITMAP)?;
            let mut app = App::new_visuals(
                owner.0,
                scale,
                Settings {
                    home_actions: vec![0, 1, 2, 3],
                    home_weather_widget: false,
                    island_animations: true,
                    character: Character::None,
                    placement,
                    ..Settings::default()
                },
            )?;
            app.ui.cfg = app.settings.clone();
            app.state = State::Expanded;
            app.win = Win::Big;
            app.toggle_widget_editor();
            let pill = app.pill_rect(true);
            let initial = view::home_widget_targets(pill, scale, &app.ui);
            let center = |r: view::Rect| (((r.0 + r.2) / 2.0).round() as i32, ((r.1 + r.3) / 2.0).round() as i32);
            let (x, y) = center(initial[0].1);
            app.click_home(x, y);
            assert!(app.ui.home.widgets.press.is_some());
            let (to_x, to_y) = center(initial[3].1);
            app.drag_tiles(to_x, to_y);
            let preview = view::home_widget_targets(pill, scale, &app.ui);
            assert_eq!(app.ui.home.widgets.drag.as_ref().unwrap().order, vec![1, 2, 3, 0]);
            assert_eq!(app.settings.home_actions, vec![0, 1, 2, 3]);
            app.on_mouse_leave();
            app.on_timer(Timer::Collapse as usize);
            assert!(app.state == State::Expanded);
            app.finish_tile_press();
            assert_eq!(app.settings.home_actions, vec![1, 2, 3, 0]);
            assert_eq!(preview, view::home_widget_targets(pill, scale, &app.ui));
            let now = view::home_widget_rects(pill, scale, &app.ui);
            let (x, y) = center(now.into_iter().find(|v| v.0 == 0).unwrap().1);
            app.begin_widget_drag(0, x, y);
            app.drag_widget(to_x - ((132.0 * scale) as i32), to_y);
            app.on_keydown(VK_ESCAPE.0);
            assert!(!app.ui.home.widgets.interacting());
            assert_eq!(app.settings.home_actions, vec![1, 2, 3, 0]);
            let area = view::home_widget_area(pill, scale);
            let target = view::home_widget_targets(pill, scale, &app.ui).into_iter().find(|v| v.0 == 0).unwrap().1;
            let (x, y) = center(target);
            app.begin_widget_drag(0, x, y);
            app.drag_widget((area.0 - 50.0 * scale).round() as i32, y);
            assert!(!app.ui.home.widgets.drag.as_ref().unwrap().valid);
            app.finish_widget_drag();
            assert_eq!(app.settings.home_actions, vec![1, 2, 3, 0]);
            app.settings.home_actions = vec![0, 1, 2, 3, view::weather_widget_id()];
            app.settings
                .home_widget_sizes
                .insert(view::weather_widget_id(), crate::data::home_widgets::WidgetSize::Wide4);
            app.ui.cfg = app.settings.clone();
            app.ui.home.widgets.motion = Default::default();
            let start = view::home_widget_targets(pill, scale, &app.ui);
            let (x, y) = center(start.iter().find(|v| v.0 == view::weather_widget_id()).unwrap().1);
            app.begin_widget_drag(view::weather_widget_id(), x, y);
            let area = view::home_widget_area(pill, scale);
            app.drag_widget(x, (area.1 + 50.0 * scale).round() as i32);
            let preview = view::home_widget_targets(pill, scale, &app.ui);
            app.finish_widget_drag();
            assert_eq!(app.settings.home_actions[0], view::weather_widget_id());
            assert_eq!(preview, view::home_widget_targets(pill, scale, &app.ui));
        }
    }
    drop(queue);
    unsafe {
        RoUninitialize();
    }
    Ok(())
}
