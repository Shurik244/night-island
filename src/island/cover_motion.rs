use crate::island::*;

impl App {
    pub(in crate::island) fn finish_cover_motion(&mut self) {
        if self.cover_flight_until.take().is_none() {
            return;
        }
        self.ui.cover_in_flight = false;
        let _ = self.cover_layer.sprite.SetIsVisible(false);
        let _ = self.cover_layer.sprite.StopAnimation(h!("Offset"));
        let _ = self.cover_layer.sprite.StopAnimation(h!("Size"));
        self.repaint_collapsed();
        self.repaint_expanded();
    }

    pub(in crate::island) fn start_cover_motion(&mut self, expanded: bool) {
        if self.ui.tab != view::Tab::Home || !self.settings.island_animations || !crate::ui::feedback::motion_allowed()
        {
            self.finish_cover_motion();
            return;
        }
        let Some(small) = view::pill_cover_rect(self.pill_rect(false), self.scale, &self.ui) else {
            self.finish_cover_motion();
            return;
        };
        let Some(cover) = self.ui.media.cover.clone() else {
            self.finish_cover_motion();
            return;
        };
        let large = view::home_cover_rect(self.pill_full(), self.scale, &self.ui);
        let (from, to) = if expanded { (small, large) } else { (large, small) };
        let width = large.2 - large.0;
        let height = large.3 - large.1;
        let result = (|| -> Result<()> {
            self.cover_layer.resize((width.ceil() as i32, height.ceil() as i32))?;
            self.cover_layer.paint(&self.gpu, |p| {
                let _ = p.cover((0.0, 0.0, width, height), 12.0 * self.scale, &cover, self.ui.cfg.cover_mode);
            })?;
            if self.cover_flight_until.is_none() {
                self.cover_layer.sprite.SetOffset(Vector3 { X: from.0, Y: from.1, Z: 0.0 })?;
                self.cover_layer.sprite.SetSize(Vector2 { X: from.2 - from.0, Y: from.3 - from.1 })?;
            }
            let ms = self.transition_ms(expanded);
            if self.open_motion.is_some() {
                self.cover_layer.sprite.StartAnimation(
                    h!("Offset"),
                    &self.open_vector3(|p| Vector3 {
                        X: small.0 + (large.0 - small.0) * p,
                        Y: small.1 + (large.1 - small.1) * p,
                        Z: 0.0,
                    })?,
                )?;
                self.cover_layer.sprite.StartAnimation(
                    h!("Size"),
                    &self.open_vector(
                        Vector2 { X: small.2 - small.0, Y: small.3 - small.1 },
                        Vector2 { X: large.2 - large.0, Y: large.3 - large.1 },
                    )?,
                )?;
            } else {
                let offset = self.compositor.CreateVector3KeyFrameAnimation()?;
                offset.InsertKeyFrameWithEasingFunction(1.0, Vector3 { X: to.0, Y: to.1, Z: 0.0 }, &self.ease()?)?;
                offset.SetDuration(Duration::from_millis(ms).into())?;
                self.cover_layer.sprite.StartAnimation(h!("Offset"), &offset)?;
                self.cover_layer
                    .sprite
                    .StartAnimation(h!("Size"), &self.spring(Vector2 { X: to.2 - to.0, Y: to.3 - to.1 }, ms)?)?;
            }
            self.cover_layer.sprite.SetIsVisible(true)?;
            self.cover_flight_until = Some(Instant::now() + Duration::from_millis(ms));
            self.ui.cover_in_flight = true;
            Ok(())
        })();
        if let Err(error) = result {
            crate::log_warn!("could not connect the player cover: {error}");
            self.cover_flight_until = Some(Instant::now());
            self.finish_cover_motion();
        }
        self.repaint_collapsed();
        self.repaint_expanded();
        if expanded {
            self.set_timer(Timer::Settle, self.transition_ms(true) as u32 + 30);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::WinRT::{
        CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
        RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
    };

    #[test]
    #[ignore = "Requires an isolated Windows Composition runtime"]
    fn connected_cover_retargets_and_cleans_up_at_every_edge_and_scale() -> Result<()> {
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
        for scale in [1.0, 1.5, 2.0] {
            for edge in Edge::ALL {
                let hwnd = unsafe {
                    CreateWindowExW(
                        WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW,
                        w!("STATIC"),
                        w!("Connected cover test"),
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
                };
                let settings =
                    Settings { placement: Placement::Docked, dock_edge: edge, music_cover: true, ..Default::default() };
                let mut app = App::new_visuals(hwnd, scale, settings)?;
                app.ui.cfg = app.settings.clone();
                app.ui.media = media::Media {
                    title: "Midnight City".into(),
                    artist: "M83".into(),
                    playing: true,
                    source: "Fixture".into(),
                    accent: Some((72, 140, 230)),
                    cover: media::debug_cover(16, &[72, 140, 230, 255].repeat(256)),
                    ..Default::default()
                };
                app.ui.char_inset = CHAR_INSET;
                app.place(Win::Big);
                app.state = State::Expanded;
                let expected = view::pill_cover_rect(app.pill_rect(false), scale, &app.ui).unwrap();
                app.start_cover_motion(true);
                if crate::ui::feedback::motion_allowed() {
                    let source = app.cover_layer.sprite.Offset()?;
                    let size = app.cover_layer.sprite.Size()?;
                    assert!((source.X - expected.0).abs() < 0.01, "{edge:?} at {scale}");
                    assert!((source.Y - expected.1).abs() < 0.01, "{edge:?} at {scale}");
                    assert!((size.X - (expected.2 - expected.0)).abs() < 0.01);
                    assert!((size.Y - (expected.3 - expected.1)).abs() < 0.01);
                    assert!(app.ui.cover_in_flight, "{edge:?} at {scale}");
                    assert!(app.cover_layer.sprite.IsVisible()?);
                    let before = app.cover_layer.sprite.Offset()?;
                    app.start_cover_motion(false);
                    assert_eq!(
                        app.cover_layer.sprite.Offset()?,
                        before,
                        "Reversing must not reset to the other endpoint"
                    );
                    app.cover_flight_until = Some(Instant::now() - Duration::from_millis(1));
                    app.ui_anim_tick();
                    assert!(!app.ui.cover_in_flight);
                    assert!(!app.cover_layer.sprite.IsVisible()?);
                }
                app.start_cover_motion(true);
                app.finish_cover_motion();
                assert!(!app.ui.cover_in_flight && app.cover_flight_until.is_none());
                if scale == 1.0
                    && edge == Edge::Top
                    && let Some(folder) = std::env::var_os("NI_UI_MOTION_PREVIEW")
                {
                    let folder = std::path::PathBuf::from(folder);
                    std::fs::create_dir_all(&folder).unwrap();
                    app.ui.cfg.header_icon_colors = true;
                    app.ui.cfg.header_tabs =
                        ["Clipboard", "Note", "Weather", "Record", "My Wave"].map(String::from).to_vec();
                    let old = app.ui.media.clone();
                    app.ui.media.title = "Night Drive".into();
                    app.ui.media.artist = "Chromatic".into();
                    app.ui.media.source = "Browser".into();
                    app.ui.media.accent = Some((208, 112, 80));
                    app.ui.media.cover = media::debug_cover(16, &[208, 112, 80, 255].repeat(256));
                    app.ui.home.track_transition.retarget(&old, true);
                    for frame in 0..24 {
                        app.ui.home.track_transition.preview_progress(frame as f32 / 23.0);
                        let surface = crate::ui::popup::PopupSurface::new((640, 410))?;
                        surface.paint(&app.gpu, |p| {
                            p.fill_round_rect(
                                (0.0, 0.0, 640.0, 410.0),
                                28.0,
                                crate::ui::gfx::rgba(0.025, 0.025, 0.03, 1.0),
                            )?;
                            view::paint_expanded(p, (0.0, 0.0, 640.0, 410.0), 1.0, &app.ui, &[])
                        })?;
                        let mut pixels = surface.pixels();
                        for pixel in pixels.chunks_exact_mut(4) {
                            pixel.swap(0, 2);
                        }
                        let mut encoder = png::Encoder::new(
                            std::fs::File::create(folder.join(format!("track-{frame:03}.png"))).unwrap(),
                            640,
                            410,
                        );
                        encoder.set_color(png::ColorType::Rgba);
                        encoder.set_depth(png::BitDepth::Eight);
                        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
                    }
                }
                if scale == 1.0
                    && edge == Edge::Top
                    && let Some(path) = std::env::var_os("NI_HEADER_SELECTION_PREVIEW")
                {
                    app.ui.cfg.header_icon_colors = true;
                    app.ui.cfg.header_tabs =
                        ["Clipboard", "Note", "Weather", "Record", "My Wave"].map(String::from).to_vec();
                    app.ui.feedback.set_enabled(false);
                    let surface = crate::ui::popup::PopupSurface::new((640, 198))?;
                    surface.paint(&app.gpu, |p| {
                        p.fill_rect((0.0, 0.0, 640.0, 198.0), crate::ui::gfx::rgba(0.025, 0.025, 0.03, 1.0))?;
                        for (row, (tab, modern)) in
                            [(view::Tab::Clipboard, true), (view::Tab::Yandex, true), (view::Tab::Clipboard, false)]
                                .into_iter()
                                .enumerate()
                        {
                            let y = row as f32 * 66.0;
                            app.ui.tab = tab;
                            app.ui.cfg.header_active_accent = modern;
                            app.ui.hover_tab = Some(0);
                            p.push_clip((0.0, y, 640.0, y + 62.0));
                            view::paint_expanded(p, (0.0, y, 640.0, 384.0), 1.0, &app.ui, &[])?;
                            p.pop_clip();
                        }
                        Ok(())
                    })?;
                    let mut pixels = surface.pixels();
                    for pixel in pixels.chunks_exact_mut(4) {
                        pixel.swap(0, 2);
                    }
                    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 640, 198);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
                }
                drop(app);
                unsafe {
                    DestroyWindow(hwnd)?;
                }
            }
        }
        Ok(())
    }
}
