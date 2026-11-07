use crate::island::*;

fn mutex_name() -> windows::core::HSTRING {
    let base = "Local\\NightIsland.SingleInstance";
    match std::env::var("NI_INSTANCE") {
        Ok(tag) if cfg!(feature = "demo") => format!("{base}.{tag}").into(),
        _ => base.into(),
    }
}

pub fn run() -> Result<()> {
    unsafe {
        let mut _mutex = CreateMutexW(None, false, &mutex_name())?;
        let updated = std::env::args().any(|a| a == "--updated");
        let mut waited = 0;
        while updated
            && waited < 50
            && windows::Win32::Foundation::GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS
        {
            let _ = windows::Win32::Foundation::CloseHandle(_mutex);
            std::thread::sleep(std::time::Duration::from_millis(200));
            _mutex = CreateMutexW(None, false, &mutex_name())?;
            waited += 1;
        }
        if windows::Win32::Foundation::GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS {
            if !std::env::args().any(|arg| arg == "--startup")
                && let Ok(existing) = FindWindowW(w!("NightIsland.Window"), None)
            {
                let _ = PostMessageW(Some(existing), WM_SHOW, WPARAM(0), LPARAM(0));
            }
            return Ok(());
        }

        let started = Instant::now();
        crate::data::log::init();
        crate::log_info!("starting Night Island {}", env!("CARGO_PKG_VERSION"));
        update::clean_up_after_update();
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        RoInitialize(RO_INIT_SINGLETHREADED)?;
        let _dispatcher = CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        })?;

        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: w!("NightIsland.Window"),
            ..Default::default()
        };
        RegisterClassExW(&class);

        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.lpszClassName,
            w!("Night Island"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        tray::add(hwnd);
        clips::sys::listen(hwnd);
        dnd::register(hwnd);

        let settings = Settings::load();
        {
            let (panel, accent) = crate::ui::theme::colors(&settings);
            crate::ui::theme::set(panel, accent);
        }
        let weather_on = settings.weather;
        let weather_city = settings.weather_city.clone();
        let (weather_lat, weather_lon) = (settings.weather_lat, settings.weather_lon);
        crate::ui::i18n::set_ru(settings.language == crate::data::settings::Language::Ru);
        crate::ui::icons::set_legacy(settings.legacy_icons);
        crate::ui::fonts::load_custom(&settings.interface_font_file);
        crate::ui::fonts::set(settings.interface_font);
        let dpi_scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        ai::configure(&settings.ai);
        update::configure(settings.update_check, settings.update_beta);
        let mut app = App::new(hwnd, dpi_scale, settings)?;
        app.place(Win::Small);
        app.apply_capture();
        capture::set_hide_self(app.settings.capture_self_hidden);
        app.set_blur(false);
        app.schedule_autohide();
        app.start_liquid();
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        let _ = windows::Win32::System::RemoteDesktop::WTSRegisterSessionNotification(
            hwnd,
            windows::Win32::System::RemoteDesktop::NOTIFY_FOR_THIS_SESSION,
        );
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        crate::log_info!("Primary window ready in {} ms", started.elapsed().as_millis());
        with_app(|app| app.configure_system_notifications());
        with_app(|app| app.configure_desktop_dock());
        with_app(|app| app.configure_top_panel());

        let failed = with_app_value(|a| register_hotkeys(hwnd, &a.settings)).unwrap_or_default();
        if !failed.is_empty() {
            crate::log_warn!("hotkeys taken by another program: {failed:?}");
        }
        #[cfg(feature = "demo")]
        demo::apply(hwnd);
        apps::start_indexing();
        with_app(|app| media::configure_priority(app.settings.prefer_playing_media));
        media::start(hwnd);
        crate::services::yandex::start(hwnd);
        downloads::start(hwnd);
        privacy::start(hwnd);
        update::start_background(hwnd);
        if with_app_value(|a| a.settings.notify.lock_keys).unwrap_or(false) {
            lockkeys::start(hwnd);
        }
        match (weather_lat, weather_lon) {
            (Some(lat), Some(lon)) => weather::set_place(&weather_city, lat, lon),
            _ => weather::set_city(&weather_city),
        }
        weather::start(hwnd, weather_on);
        spotify::init(hwnd);
        discord::start(hwnd);
        with_app(|a| {
            a.start_game_watch();
            if a.settings.tour_pending() && !cfg!(feature = "demo") {
                a.open_guide();
            }
        });

        if !cfg!(feature = "demo") {
            crate::platform::autostart::refresh();
        }
        crate::log_info!("Startup services scheduled in {} ms", started.elapsed().as_millis());
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

impl App {
    pub(in crate::island) fn new(hwnd: HWND, dpi_scale: f32, settings: Settings) -> Result<Self> {
        let mut app = Self::new_visuals(hwnd, dpi_scale, settings)?;
        app.store = Store::open();
        app.apply_character();
        app.ui.cfg = app.settings.clone();
        app.ui.yandex.ratings_hidden = !app.settings.wave_ratings;
        app.ui.yandex.search_hidden = !app.settings.wave_search;
        app.ui.char_inset = if app.attached_character_on() { CHAR_INSET } else { 0.0 };
        app.cw = view::collapsed_width(&app.ui, COLLAPSED.0);
        if app.settings.hang() != Hang::Down {
            app.apply_layout();
        }
        app.ui.mic_muted = volume::mic_muted();
        vpn::set_enabled(true);
        vpn::start_watch(app.hwnd);
        app.reload_clips();
        if let Some(store) = &app.store {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            store.close_open_uses(now as i64);
        }
        app.start_activity();
        app.sync_metrics();
        app.settings.assistant.apply();
        app.sync_wake();
        app.ui.agents = agents::sessions();
        agents::watch(hwnd);
        app.init_steam();
        steam::watch(hwnd);
        app.reload_dock();
        app.restore_tab();
        app.reload_reminders();
        app.init_notes();
        app.update_offsets();
        app.repaint();
        app.schedule_clock();
        app.apply_pill_opacity();
        Ok(app)
    }

    pub(in crate::island) fn new_visuals(hwnd: HWND, dpi_scale: f32, settings: Settings) -> Result<Self> {
        crate::island::timers::raise_timer_resolution();
        let compositor = Compositor::new()?;
        let target = unsafe { compositor.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(hwnd, true)? };

        let full = Vector2 { X: 1.0, Y: 1.0 };

        let backdrop = compositor.CreateSpriteVisual()?;
        backdrop.SetRelativeSizeAdjustment(full)?;
        backdrop.SetBrush(&compositor.CreateHostBackdropBrush()?)?;

        let tint = compositor.CreateSpriteVisual()?;
        tint.SetRelativeSizeAdjustment(full)?;
        let (pr, pg, pb) = crate::ui::theme::panel_u8();
        tint.SetBrush(&compositor.CreateColorBrushWithColor(Color { A: 255, R: pr, G: pg, B: pb })?)?;

        let scale = dpi_scale * settings.scale_percent as f32 / 100.0;
        let gpu = Gpu::new(&compositor)?;
        let docked = settings.placement == Placement::Docked;
        let collapsed =
            gpu.layer(&compositor, window_size(scale, Win::Small, docked, COLLAPSED.0, &window_room(&settings)))?;
        let progress_fill = gpu.layer(&compositor, collapsed.size())?;
        let expanded =
            gpu.layer(&compositor, window_size(scale, Win::Big, docked, COLLAPSED.0, &window_room(&settings)))?;
        let ear_px = ((EAR * scale).round() as i32, (EAR * scale).round() as i32);
        let ears = [gpu.layer(&compositor, ear_px)?, gpu.layer(&compositor, ear_px)?];
        for ear in &ears {
            ear.sprite.SetOpacity(0.0)?;
        }
        expanded.sprite.SetOpacity(0.0)?;

        let char_px = ((characters::BOX_W * scale).round() as i32, (characters::BOX_H * scale).round() as i32);
        let char_layer = gpu.layer(&compositor, char_px)?;
        char_layer.sprite.SetIsVisible(false)?;

        let intro_cover = gpu.layer(&compositor, ((28.0 * scale).ceil() as i32, (28.0 * scale).ceil() as i32))?;
        let cover_layer = gpu.layer(&compositor, ((72.0 * scale).ceil() as i32, (72.0 * scale).ceil() as i32))?;
        cover_layer.sprite.SetIsVisible(false)?;
        let previous_pill = gpu.layer(&compositor, collapsed.size())?;
        previous_pill.sprite.SetIsVisible(false)?;
        let intro_title = gpu.layer(&compositor, ((240.0 * scale).ceil() as i32, (34.0 * scale).ceil() as i32))?;
        let intro_title_clip = compositor.CreateInsetClip()?;
        intro_title.sprite.SetClip(&intro_title_clip)?;
        intro_cover.sprite.SetIsVisible(false)?;
        intro_title.sprite.SetIsVisible(false)?;

        let ring =
            gpu.layer(&compositor, window_size(scale, Win::Small, docked, COLLAPSED.0, &window_room(&settings)))?;
        ring.sprite.SetIsVisible(false)?;
        let highlight = gpu.layer(&compositor, collapsed.size())?;
        highlight.sprite.SetIsVisible(false)?;
        let bump = gpu.layer(&compositor, bump_size(scale, (COLLAPSED.0, COLLAPSED.1)))?;
        bump.sprite.SetIsVisible(false)?;
        bump.sprite.SetCenterPoint(Vector3 { X: bump.size().0 as f32 / 2.0, Y: 0.0, Z: 0.0 })?;
        let bump_fill = compositor.CreateSpriteVisual()?;
        bump_fill.SetRelativeSizeAdjustment(full)?;
        bump_fill.SetBrush(&bump.surface_brush(&compositor)?)?;
        bump.sprite.Children()?.InsertAtTop(&bump_fill)?;
        let bump_blur = compositor.CreateSpriteVisual()?;
        bump_blur.SetRelativeSizeAdjustment(full)?;
        bump_blur.SetBrush(&compositor.CreateHostBackdropBrush()?)?;
        let bump_shape = compositor.CreatePathGeometry()?;
        bump_blur.SetClip(&compositor.CreateGeometricClipWithGeometry(&bump_shape)?)?;
        bump_blur.SetIsVisible(false)?;
        bump.sprite.Children()?.InsertAtBottom(&bump_blur)?;
        bump.sprite.SetBrush(&compositor.CreateColorBrushWithColor(Color { A: 0, R: 0, G: 0, B: 0 })?)?;

        let visual = compositor.CreateContainerVisual()?;
        visual.SetRelativeSizeAdjustment(full)?;
        let children = visual.Children()?;
        children.InsertAtTop(&backdrop)?;
        children.InsertAtTop(&tint)?;
        children.InsertAtTop(&progress_fill.sprite)?;
        children.InsertAtTop(&highlight.sprite)?;
        children.InsertAtTop(&expanded.sprite)?;
        children.InsertAtTop(&collapsed.sprite)?;
        children.InsertAtTop(&previous_pill.sprite)?;
        children.InsertAtTop(&char_layer.sprite)?;

        let mut spectrum_brushes = Vec::new();

        let mut spectrum = Vec::new();
        for i in 0..audio::BANDS {
            let t = i as f32 / (audio::BANDS - 1) as f32;
            let (r, g, b) = (166.0 + 50.0 * t, 179.0 - 40.0 * t, 255.0);
            let bar = compositor.CreateSpriteVisual()?;
            bar.SetSize(Vector2 { X: SPEC_W * scale, Y: SPEC_H * scale })?;
            bar.SetCenterPoint(Vector3 { X: 0.0, Y: SPEC_H * scale / 2.0, Z: 0.0 })?;
            let brush = compositor.CreateColorBrushWithColor(Color { A: 255, R: r as u8, G: g as u8, B: b as u8 })?;
            bar.SetBrush(&brush)?;
            spectrum_brushes.push(brush);
            bar.SetIsVisible(false)?;
            children.InsertAtTop(&bar)?;
            spectrum.push(bar);
        }

        let geometry = compositor.CreateRoundedRectangleGeometry()?;
        children.InsertAtTop(&intro_cover.sprite)?;
        children.InsertAtTop(&cover_layer.sprite)?;
        children.InsertAtTop(&intro_title.sprite)?;
        let round_clip = compositor.CreateGeometricClipWithGeometry(&geometry)?;
        visual.SetClip(&round_clip)?;
        let body_shape = compositor.CreatePathGeometry()?;
        let body_clip = compositor.CreateGeometricClipWithGeometry(&body_shape)?;
        let root = compositor.CreateContainerVisual()?;
        root.SetRelativeSizeAdjustment(full)?;
        let root_children = root.Children()?;
        root_children.InsertAtTop(&ring.sprite)?;
        root_children.InsertAtTop(&visual)?;
        root_children.InsertAtTop(&bump.sprite)?;
        for ear in &ears {
            root_children.InsertAtTop(&ear.sprite)?;
        }
        target.SetRoot(&root)?;

        let app = App {
            hwnd,
            scale,
            dpi_scale,
            settings_win: None,
            desktop_dock: None,
            windows_taskbar: None,
            top_panel: None,
            top_panel_poll: Instant::now(),
            top_panel_radios: Instant::now(),
            top_panel_data: Default::default(),
            power_last: view::battery(),
            lock_pending: Vec::new(),
            settings,
            win: Win::Small,
            compositor,
            geometry,
            open_motion: None,
            state: State::Collapsed,
            _target: target,
            _visual: visual,
            backdrop,
            tint,
            ui: {
                let mut ui = view::UiState::new();
                ui.composited_progress = true;
                ui
            },
            store: None,
            popup_open: false,
            overlay: None,
            want_dock_dialog: false,
            want_chat_dialog: false,
            want_header_menu: false,
            pending_power: None,
            pending_ask: None,
            wake_session: false,
            agent: None,
            dialog: Vec::new(),
            run_serial: 0,
            last_spoken: String::new(),
            reminders: Vec::new(),
            recording: None,
            notice_link: None,
            prev_foreground: HWND::default(),
            cw: COLLAPSED.0,
            system_notices: Default::default(),
            pill_motion: super::pill_motion::PillMotion {
                content: None,
                width: None,
                intro: None,
                intro_started: None,
                intro_width: COLLAPSED.0,
                gate: Default::default(),
                track: (String::new(), String::new(), String::new()),
                cover: intro_cover,
                previous: previous_pill,
                title: intro_title,
                title_clip: intro_title_clip,
            },
            pending_high_surrogate: None,
            gaming: false,
            pill_hidden: false,
            ghost_stage: 0,
            metrics_on: None,
            steam_game: None,
            ghosted: false,
            root,
            gpu,
            ears,
            collapsed,
            progress_fill,
            expanded,
            cover_layer,
            cover_flight_until: None,
            liquid: Liquid {
                ring,
                highlight,
                pull: Default::default(),
                pull_ready: false,
                bump,
                bump_fill,
                bump_blur,
                round_clip,
                body_clip,
                body_shape,
                body_active: std::cell::Cell::new(false),
                bump_amp: 0.0,
                bump_vel: 0.0,
                last_tick: Instant::now(),
                idle_running: false,
            },
            music: MusicBars {
                spectrum,
                spectrum_brushes,
                spectrum_running: false,
                spectrum_scale: [0.0; audio::BANDS],
                edge_values: std::cell::Cell::new([0.0; audio::BANDS]),
                edge_time: std::cell::Cell::new(Instant::now()),
            },
            pet: Pet {
                returning: None,
                behavior: crate::ui::pet_behavior::PetBehavior::new(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .subsec_nanos(),
                ),
                speech: Default::default(),
                speech_cue: Default::default(),
                voice_phase: Default::default(),
                motion: Default::default(),
                controls_at: Instant::now(),
                cursor: None,
                char_layer,
                desktop: None,
                chat: None,
                beat: 0.0,
                beat_at: Instant::now(),
                born: Instant::now(),
                next_blink: Instant::now() + Duration::from_secs(3),
                blink_until: Instant::now(),
                rng: 0x9E37_79B9,
                reaction_until: None,
                char_key: None,
                frame: 0,
            },
            pointer: Pointer {
                expand_pending: Default::default(),
                decoration_through: false,
                tracking: false,
                press: None,
                character_press: None,
                moving: None,
                drag_from: None,
                selecting: false,
                reading: false,
                slider: None,
                vol_drag: false,
                seek_drag: false,
            },
            clipboard: ClipboardState { clips: Vec::new(), clips_all: Vec::new(), own_clip_seq: 0, paste_tries: 0 },
        };
        app.set_geometry_now(Win::Small, false, false)?;
        app.paint_ears();
        Ok(app)
    }
}
