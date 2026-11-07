use crate::island::*;
use crate::platform::{wake, wake_install};

pub(in crate::island) const WM_CLIPBOARDUPDATE: u32 = 0x031D;
pub(in crate::island) const WM_MOUSELEAVE: u32 = 0x02A3;
pub(in crate::island) const WM_SHOW: u32 = WM_APP + 2;

fn customize_header_menu(point: windows::Win32::Foundation::POINT) {
    use crate::data::settings::{DEFAULT_HEADER_TABS, HIDEABLE_TABS};
    use crate::ui::i18n::tr;
    use crate::ui::menu::{self, Entry};
    loop {
        let mut entries = Vec::new();
        with_app(|a| {
            for (i, name) in HIDEABLE_TABS.iter().enumerate() {
                if *name == "Steam" && !steam::client_installed() {
                    continue;
                }
                let pinned = a.settings.header_tabs.iter().any(|tab| tab == name) && !a.settings.tab_hidden(name);
                let location = if a.settings.tab_hidden(name) {
                    "Hidden"
                } else if pinned {
                    "On the header"
                } else {
                    "In More"
                };
                entries.push(Entry::item(i + 1, &format!("{} · {}", tr(name), tr(location))).glyph(if pinned {
                    '\u{E73E}'
                } else {
                    '\u{E739}'
                }));
            }
            entries.push(Entry::divider());
            entries.push(Entry::item(200, &tr("Restore default header")));
        });
        let Some(choice) = menu::show(entries, (point.x, point.y), false, app_scale(), &|f| with_app(|a| f(&a.gpu)))
        else {
            break;
        };
        if choice == 200 {
            with_app(|a| {
                a.settings.header_tabs = DEFAULT_HEADER_TABS.map(String::from).to_vec();
                for name in DEFAULT_HEADER_TABS {
                    a.settings.set_tab_hidden(name, false);
                }
            });
        } else if let Some(name) = choice.checked_sub(1).and_then(|i| HIDEABLE_TABS.get(i)).copied() {
            let mut pinned = false;
            let mut replacements = Vec::new();
            with_app(|a| {
                pinned = a.settings.header_tabs.iter().any(|tab| tab == name) && !a.settings.tab_hidden(name);
                replacements = a.settings.header_tabs.clone();
            });
            let replacement = if !pinned {
                let mut entries = vec![Entry::item(100, &tr("Add to header")), Entry::divider()];
                entries.extend(
                    replacements
                        .iter()
                        .enumerate()
                        .map(|(i, old)| Entry::item(i + 1, &format!("{} {}", tr("Replace:"), tr(old)))),
                );
                let Some(id) =
                    menu::show(entries, (point.x, point.y), false, app_scale(), &|f| with_app(|a| f(&a.gpu)))
                else {
                    continue;
                };
                if id == 100 { None } else { replacements.get(id - 1).cloned() }
            } else {
                None
            };
            with_app(|a| {
                a.settings.set_header_tab(name, !pinned, replacement.as_deref());
            });
        }
        with_app(|a| {
            a.ui.cfg = a.settings.clone();
            a.settings.save();
            a.render_settings_win();
            a.repaint_expanded();
        });
    }
}

fn show_header_menu(hwnd: HWND) {
    use crate::ui::menu::Entry;
    let mut entries = Vec::new();
    let mut point = windows::Win32::Foundation::POINT::default();
    with_app(|a| {
        a.popup_open = true;
        a.kill_timer(Timer::Collapse);
        a.set_tip(None);
        let pill = a.pill_full();
        for i in view::overflow_tabs(pill, a.scale, &a.ui) {
            let (_, glyph, name) = view::TABS[i];
            entries.push(Entry::item(i + 1, &crate::ui::i18n::tr(name)).glyph(glyph));
        }
        if !entries.is_empty() {
            entries.push(Entry::divider());
        }
        entries.push(Entry::item(103, &crate::ui::i18n::tr("Customize header…")).glyph('\u{E713}'));
        if view::tab_settings_group(a.ui.tab).is_some() {
            entries.push(Entry::item(100, &crate::ui::i18n::tr("Settings of this tab")));
        }
        entries.push(Entry::item(
            101,
            &crate::ui::i18n::tr(if a.settings.lock_drag { "Unlock dragging" } else { "Lock dragging" }),
        ));
        entries.push(Entry::item(
            102,
            &crate::ui::i18n::tr(if a.settings.tab_drag { "Stop moving tabs" } else { "Move tabs by dragging" }),
        ));
        let r = view::more_rect(pill, a.scale);
        point.x = r.0 as i32;
        point.y = (r.3 + 6.0 * a.scale) as i32;
    });
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut point);
    }
    let choice = crate::ui::menu::show(entries, (point.x, point.y), false, app_scale(), &|f| {
        with_app(|a| f(&a.gpu));
    });
    if choice == Some(103) {
        customize_header_menu(point);
    }
    with_app(|a| {
        a.popup_open = false;
        a.pointer.tracking = false;
        match choice {
            Some(id) if id > 0 && id <= view::TABS.len() => {
                let tab = a.landing_tab(view::TABS[id - 1].0);
                a.select_tab(tab);
            }
            Some(103) => {}
            Some(100) => {
                if matches!(a.ui.tab, view::Tab::Chat | view::Tab::Ai) {
                    a.open_ai_settings();
                } else if let Some(group) = view::tab_settings_group(a.ui.tab) {
                    a.open_settings_at(group);
                }
            }
            Some(id @ (101 | 102)) => {
                if id == 101 {
                    a.settings.lock_drag = !a.settings.lock_drag;
                } else {
                    a.settings.tab_drag = !a.settings.tab_drag;
                }
                a.ui.cfg = a.settings.clone();
                a.settings.save();
                a.render_settings_win();
            }
            _ => {}
        }
        a.repaint_expanded();
        if !a.cursor_inside() && !a.ui.pinned {
            a.set_timer(Timer::Collapse, a.settings.close_delay_ms.max(1));
        }
    });
}

pub(in crate::island) fn point_from(lparam: LPARAM) -> (i32, i32) {
    (lparam.0 as i16 as i32, (lparam.0 >> 16) as i16 as i32)
}

pub(in crate::island) extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        crate::ui::fonts::WM_FONT_PICK => {
            let mut owner = hwnd;
            with_app(|a| {
                owner = a.settings_win.unwrap_or(hwnd);
                a.popup_open = true;
            });
            let file = pick_font(owner);
            with_app(|a| {
                a.popup_open = false;
                if let Some(file) = file {
                    a.ui.sw.font_busy = true;
                    a.ui.sw.font_status.clear();
                    crate::ui::fonts::import_async(file, hwnd);
                }
                a.render_settings_win();
            });
            LRESULT(0)
        }
        crate::ui::fonts::WM_FONT_READY => {
            with_app(|a| a.finish_font_import());
            LRESULT(0)
        }
        crate::platform::notifications::WM_NOTIFICATION_OPENED => {
            with_app(|a| {
                if wparam.0 != 0 {
                    a.collapse();
                } else {
                    a.notify(Notice::Error, '\u{E783}', "Could not open the sender app");
                }
            });
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let (x, y) = point_from(lparam);
            let mut through = false;
            with_app(|a| {
                if a.open_motion.is_some() {
                    let mut rect = windows::Win32::Foundation::RECT::default();
                    if unsafe { GetWindowRect(hwnd, &mut rect).is_ok() }
                        && let Some(hit) = a.transition_hit(x - rect.left, y - rect.top)
                    {
                        through = !hit;
                        return;
                    }
                }
                if a.state == State::Collapsed
                    && (a.liquid_on() || a.pill_motion.width.is_some())
                    && !a.character_interacting()
                    && (!a.pill_motion_busy() || a.pill_motion.width.is_some())
                {
                    let mut rect = windows::Win32::Foundation::RECT::default();
                    if unsafe { GetWindowRect(hwnd, &mut rect).is_ok() } {
                        through = !a.on_interactive_body(x - rect.left, y - rect.top);
                        a.set_decoration_through(through);
                    }
                }
            });
            LRESULT(if through { HTTRANSPARENT as isize } else { HTCLIENT as isize })
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = point_from(lparam);
            let left_down = wparam.0 & 0x0001 != 0;
            let mut drag = None;
            with_app(|a| {
                a.on_mouse_move(x, y);
                a.hover_at(x, y);
                if left_down {
                    a.drag_character();
                    a.drag_select(x, y);
                    a.drag_read(x, y);
                    a.drag_slider(x);
                    a.drag_volume(x);
                    a.drag_mixer(x);
                    a.drag_seek(x);
                    a.drag_island();
                    a.drag_tabs(x, y);
                    a.drag_tiles(x, y);
                    a.drag_notes(x, y);
                }
                drag = a.take_drag(x, y, left_down);
            });
            if let Some(path) = drag {
                dnd::drag_out(&path);
                with_app(|a| a.on_mouse_leave());
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wparam.0 >> 16) as i16 as i32;
            with_app(|a| a.wheel(delta));
            LRESULT(0)
        }
        WM_CLIPBOARDUPDATE => {
            with_app(|a| a.set_timer(Timer::Clip, 40));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            with_app(|a| {
                a.pointer.drag_from = None;
                a.finish_dock_press();
                a.finish_tab_press();
                a.finish_tile_press();
                a.pointer.selecting = false;
                a.pointer.reading = false;
                if a.pointer.vol_drag && a.ui.tab == view::Tab::Yandex {
                    a.drag_volume(point_from(lparam).0);
                    unsafe {
                        let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                    }
                }
                a.pointer.vol_drag = false;
                a.finish_note_drag();
                a.mouse_up();
                a.finish_seek();
                a.finish_mixer();
                a.finish_slider();
            });
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            with_app(|a| {
                a.pointer.vol_drag = false;
                if a.ui.pressed_action.take().is_some() {
                    a.repaint_expanded();
                }
                if a.pointer.seek_drag {
                    a.pointer.seek_drag = false;
                    a.ui.home.seek = None;
                    a.repaint_expanded();
                }
                a.pointer.character_press = None;
                if a.ui.mixer.dragging.take().is_some() {
                    a.sync_mixer();
                }
            });
            LRESULT(0)
        }
        dnd::WM_DND_ENTER => {
            with_app(|a| a.on_dnd_enter());
            LRESULT(0)
        }
        dnd::WM_DND_LEAVE => {
            with_app(|a| a.on_dnd_leave());
            LRESULT(0)
        }
        dnd::WM_DND_DROP => {
            with_app(|a| a.on_dnd_drop());
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = point_from(lparam);
            with_app(|a| a.mouse_down(x, y));
            let mut dialog = false;
            with_app(|a| dialog = std::mem::take(&mut a.want_dock_dialog));
            if dialog {
                with_app(|a| a.popup_open = true);
                let picked = pick_file(hwnd);
                with_app(|a| a.finish_dock_dialog(picked));
            }
            let mut chat_dialog = false;
            with_app(|a| chat_dialog = std::mem::take(&mut a.want_chat_dialog));
            if chat_dialog {
                with_app(|a| a.popup_open = true);
                let picked = pick_file(hwnd);
                with_app(|a| a.finish_chat_dialog(picked));
            }
            let mut header_menu = false;
            with_app(|a| header_menu = std::mem::take(&mut a.want_header_menu));
            if header_menu {
                show_header_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            let (x, y) = point_from(lparam);
            with_app(|a| {
                if let Some((index, _)) = a.dock_target_at(x, y) {
                    a.apply_dock_choice(Some(3), index, "");
                }
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let (x, y) = point_from(lparam);
            let mut handled = false;
            with_app(|a| handled = a.right_click_home_volume(x, y));
            if handled {
                return LRESULT(0);
            }
            if with_app_value(|a| a.can_release_character(x, y)) == Some(true) {
                with_app(|a| a.popup_open = true);
                let mut point = windows::Win32::Foundation::POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut point);
                }
                let choice = crate::ui::menu::show(
                    vec![crate::ui::menu::Entry::item(1, &crate::ui::i18n::tr("Release character"))],
                    (point.x, point.y),
                    false,
                    app_scale(),
                    &|f| with_app(|a| f(&a.gpu)),
                );
                with_app(|a| {
                    a.popup_open = false;
                    if choice == Some(1) {
                        a.set_character_detached(true);
                    }
                });
                return LRESULT(0);
            }
            let mut target = None;
            with_app(|a| target = a.dock_target_at(x, y));
            if let Some((index, path)) = target {
                with_app(|a| a.popup_open = true);
                let choice = dock_popup(hwnd, std::path::Path::new(&path).exists());
                with_app(|a| {
                    a.popup_open = false;
                    a.apply_dock_choice(choice, index, &path);
                });
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_app(|a| a.on_mouse_leave());
            LRESULT(0)
        }
        WM_TIMER => {
            with_app(|a| a.on_timer(wparam.0));
            LRESULT(0)
        }
        WM_HOTKEY => {
            match Hotkey::from_id(wparam.0 as i32) {
                Some(Hotkey::Toggle) => with_app(|a| {
                    a.restore_pill();
                    a.toggle();
                }),
                Some(Hotkey::Clipboard) => with_app(|a| {
                    a.restore_pill();
                    a.open_tab(view::Tab::Clipboard);
                }),
                Some(Hotkey::PlainPaste) => with_app(|a| a.plain_paste()),
                Some(Hotkey::Command) => with_app(|a| {
                    a.restore_pill();
                    a.toggle_command();
                }),
                Some(Hotkey::HidePill) => with_app(|a| a.hide_pill_for_a_while()),
                Some(Hotkey::Mic) => with_app(|a| a.toggle_mic()),
                Some(Hotkey::ShotArea) => with_app(|a| a.capture(capture::Mode::Area)),
                Some(Hotkey::ShotFull) => with_app(|a| capture::fullscreen(a.hwnd)),
                Some(Hotkey::Picker) => with_app(|a| a.capture(capture::Mode::Picker)),
                Some(Hotkey::Ocr) => with_app(|a| a.capture(capture::Mode::Ocr)),
                Some(Hotkey::Qr) => with_app(|a| a.capture(capture::Mode::Qr)),
                Some(Hotkey::Rec) => with_app(|a| a.toggle_recording()),
                Some(Hotkey::Dictate) => with_app(|a| a.toggle_dictation()),
                _ => {}
            }
            LRESULT(0)
        }
        privacy::WM_PRIVACY => {
            with_app(|a| a.on_privacy());
            LRESULT(0)
        }
        downloads::WM_DOWNLOAD => {
            with_app(|a| a.on_downloads());
            LRESULT(0)
        }
        recorder::WM_RECORDED => {
            with_app(|a| a.on_recorded());
            LRESULT(0)
        }
        recognize::WM_RECOGNIZED => {
            with_app(|a| a.on_recognized());
            LRESULT(0)
        }
        capture::WM_CAPTURE_DONE => {
            with_app(|a| a.on_capture_done());
            LRESULT(0)
        }
        weather::WM_GEO => {
            with_app(|a| a.on_geo());
            LRESULT(0)
        }
        agy::WM_AGY => {
            with_app(|a| a.on_agy());
            LRESULT(0)
        }
        overlay::WM_OVERLAY => {
            with_app(|a| a.on_overlay(wparam.0));
            LRESULT(0)
        }
        discord::WM_DISCORD => {
            with_app(|a| a.on_discord());
            LRESULT(0)
        }
        spotify::WM_SPOTIFY => {
            with_app(|a| a.on_spotify());
            LRESULT(0)
        }
        weather::WM_WEATHER => {
            with_app(|a| a.on_weather());
            LRESULT(0)
        }
        lockkeys::WM_LOCKKEY => {
            let vk = wparam.0;
            with_app(|a| a.on_lockkey(vk));
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
            if wparam.0 as u32 == PBT_APMPOWERSTATUSCHANGE {
                with_app(|a| a.on_power());
            }
            LRESULT(1)
        }
        update::WM_UPDATE => {
            with_app(|a| a.on_update());
            LRESULT(0)
        }
        everything::WM_FILES => {
            with_app(|a| a.on_files());
            LRESULT(0)
        }
        steam::WM_STEAM => {
            with_app(|a| a.on_steam());
            LRESULT(0)
        }
        agents::WM_AGENT => {
            with_app(|a| a.on_agent());
            LRESULT(0)
        }
        winevent::WM_FOREGROUND => {
            winevent::acknowledge();
            with_app(|a| {
                a.on_foreground_changed();
                a.update_top_panel(true);
                if let Some(dock) = &mut a.desktop_dock {
                    dock.refresh_windows();
                }
            });
            LRESULT(0)
        }
        crate::platform::memory_tools::WM_MEMORY_TOOLS => {
            with_app(|a| a.on_memory_tools());
            LRESULT(0)
        }
        metrics::WM_METRICS => {
            with_app(|a| a.on_metrics());
            LRESULT(0)
        }
        dictation::WM_VOICE => {
            with_app(|a| a.on_voice());
            LRESULT(0)
        }
        crate::services::assistant::shell::WM_SHELL => {
            with_app(|a| a.on_shell());
            LRESULT(0)
        }
        crate::platform::localvoice::WM_LOCAL_VOICE => {
            with_app(|a| a.on_local_voice_setup());
            LRESULT(0)
        }
        crate::platform::speech::WM_SPOKEN => {
            if crate::platform::speech::current(wparam.0 as u32) {
                with_app(|a| a.on_spoken());
            }
            LRESULT(0)
        }
        wake::WM_WAKE => {
            with_app(|a| a.on_wake());
            LRESULT(0)
        }
        wake_install::WM_WAKE_SETUP => {
            with_app(|a| a.on_wake_setup());
            LRESULT(0)
        }
        ai::WM_TRANSCRIBED => {
            with_app(|a| a.on_transcribed());
            LRESULT(0)
        }
        crate::ui::mini_chat::WM_MINI_CHAT => {
            with_app(|a| a.on_mini_chat(wparam.0, lparam.0));
            let mut dialog = false;
            with_app(|a| dialog = std::mem::take(&mut a.want_chat_dialog));
            if dialog {
                with_app(|a| a.popup_open = true);
                let picked = pick_file(hwnd);
                with_app(|a| a.finish_chat_dialog(picked));
            }
            LRESULT(0)
        }
        ai::WM_CHAT => {
            with_app(|a| a.on_chat());
            LRESULT(0)
        }
        ai::WM_COMMAND => {
            with_app(|a| a.on_voice_command());
            LRESULT(0)
        }
        vpn::WM_VPN => {
            with_app(|a| a.on_vpn());
            LRESULT(0)
        }
        steam_art::WM_STEAM_ART => {
            with_app(|a| a.on_steam_art());
            LRESULT(0)
        }
        steam::WM_STEAM_SWITCH => {
            with_app(|a| a.on_steam_switch());
            LRESULT(0)
        }
        ai::WM_SORT => {
            with_app(|a| a.on_sort());
            LRESULT(0)
        }
        ai::WM_AI => {
            with_app(|a| a.on_ai());
            LRESULT(0)
        }
        lyrics::WM_LYRICS => {
            with_app(|a| a.on_lyrics());
            LRESULT(0)
        }
        crate::platform::notifications::WM_NOTIFICATIONS | crate::platform::notifications::WM_NOTIFICATION_ACCESS => {
            with_app(|app| app.on_system_notifications());
            LRESULT(0)
        }
        crate::ui::notification_card::WM_NOTIFICATION_CARD => {
            with_app(|app| app.on_notification_card(wparam.0, lparam.0));
            LRESULT(0)
        }
        WM_WTSSESSION_CHANGE => {
            if wparam.0 == 7 || wparam.0 == 8 {
                with_app(|app| app.notification_session(wparam.0 == 7));
            }
            LRESULT(0)
        }
        crate::services::yandex::WM_YANDEX => {
            with_app(|a| a.on_yandex());
            LRESULT(0)
        }
        crate::platform::audio_mixer::WM_MIXER => {
            with_app(|a| a.on_mixer());
            LRESULT(0)
        }
        media::WM_MEDIA => {
            with_app(|a| a.on_media());
            LRESULT(0)
        }
        system::WM_RADIOS => {
            with_app(|a| a.on_radios(wparam.0));
            LRESULT(0)
        }
        WM_COPYDATA => {
            let cds = unsafe { (lparam.0 as *const windows::Win32::System::DataExchange::COPYDATASTRUCT).as_ref() };
            if let Some(cds) = cds
                && !cds.lpData.is_null()
                && cds.cbData <= 4 * 1024 * 1024
            {
                let bytes = unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) };
                if everything::on_copydata(wparam.0, cds.dwData, bytes) {
                    return LRESULT(1);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            with_app(|a| a.on_keydown(wparam.0 as u16));
            LRESULT(0)
        }
        WM_CHAR => {
            with_app(|a| a.on_char(wparam.0 as u16));
            LRESULT(0)
        }
        WM_ACTIVATE if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE => {
            with_app(|a| {
                if demo::active("NI_DEBUG_CMD") {
                    return;
                }
                if a.ui.cmd.open {
                    a.close_command(false);
                } else {
                    a.end_edit(false);
                }
            });
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let (mut ibeam, mut hand) = (false, false);
            with_app(|a| {
                ibeam = a.cursor_over_note();
                hand = a.ui.hand;
            });
            if (ibeam || hand) && (lparam.0 & 0xFFFF) as u32 == HTCLIENT {
                let shape = if ibeam { IDC_IBEAM } else { IDC_HAND };
                unsafe { SetCursor(LoadCursorW(None, shape).ok()) };
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SHOW => {
            with_app(|a| a.peek());
            LRESULT(0)
        }
        crate::ui::companion::WM_COMPANION => {
            with_app(|a| a.on_companion(wparam.0, lparam.0));
            LRESULT(0)
        }
        tray::WM_TRAY => {
            match tray::click(lparam) {
                tray::Click::Primary => with_app(|a| a.toggle()),
                tray::Click::Secondary => match tray::menu(app_scale(), &|f| with_app(|a| f(&a.gpu))) {
                    Some(tray::ID_EXIT) => unsafe {
                        let _ = DestroyWindow(hwnd);
                    },
                    Some(tray::ID_REBUILD) => {
                        tray::rebuild_and_restart();
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                    Some(id) => with_app(|a| a.apply_menu_choice(id)),
                    None => {}
                },
                tray::Click::None => {}
            }
            LRESULT(0)
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            tray::add(hwnd);
            with_app(|a| {
                a.configure_taskbar_replacement();
                a.top_panel_request(4);
            });
            LRESULT(0)
        }
        WM_DPICHANGED => {
            with_app(|a| {
                a.dpi_scale = (wparam.0 & 0xffff) as f32 / 96.0;
                a.apply_layout();
                if a.top_panel.is_some() {
                    a.configure_top_panel();
                }
            });
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
            with_app(|a| a.top_panel_request(3));
            LRESULT(0)
        }
        crate::platform::app_capture::WM_APP_CAPTURE => {
            crate::island::desktop_dock::capture_result();
            LRESULT(0)
        }
        crate::platform::system_tray::inline::WM_RESULT => {
            if let Some(result) = crate::platform::system_tray::inline::take() {
                with_app(|a| {
                    if let Some(panel) = &mut a.top_panel {
                        match result {
                            Ok(entries) if wparam.0 == 0 => panel.set_tray_icons(entries),
                            Err(()) => panel.tray_failed(),
                            _ => {}
                        }
                    }
                });
            }
            LRESULT(0)
        }
        crate::ui::top_panel::WM_TOP_PANEL => {
            with_app(|a| a.top_panel_request(wparam.0));
            LRESULT(0)
        }
        crate::ui::desktop_dock::WM_DESKTOP_DOCK => {
            crate::island::desktop_dock::handle_request(hwnd, wparam.0);
            LRESULT(0)
        }
        settings_win::WM_SETTINGS_WINDOW_CHANGED => {
            settings_win::on_settings_window_changed();
            LRESULT(0)
        }
        WM_DESTROY => {
            with_app(|a| {
                a.save_note();
                a.top_panel.take();
                a.desktop_dock.take();
                a.windows_taskbar.take();
            });
            dnd::unregister(hwnd);
            clips::sys::unlisten(hwnd);
            tray::remove(hwnd);
            std::process::exit(0);
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn app_scale() -> f32 {
    let mut scale = 1.0;
    with_app(|a| scale = a.scale);
    scale
}

pub(in crate::island) fn dock_popup(_hwnd: HWND, exists: bool) -> Option<usize> {
    use crate::ui::menu::{Entry, show};
    let mut entries = Vec::new();
    if exists {
        entries.push(Entry::item(1, "Show in folder").glyph('\u{E838}'));
        entries.push(Entry::item(2, "Run as administrator").glyph('\u{E7EF}'));
        entries.push(Entry::divider());
    }
    entries.push(Entry::item(3, "Remove from the dock").glyph('\u{E711}').danger());
    let mut pt = windows::Win32::Foundation::POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    show(entries, (pt.x, pt.y), false, app_scale(), &|f| with_app(|a| f(&a.gpu)))
}

pub(in crate::island) fn pick_file(owner: HWND) -> Option<String> {
    pick_file_filtered(owner, false)
}

fn pick_font(owner: HWND) -> Option<String> {
    pick_file_filtered(owner, true)
}

fn pick_file_filtered(owner: HWND, font: bool) -> Option<String> {
    use windows::Win32::Foundation::{ERROR_CANCELLED, RPC_E_CHANGED_MODE};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST, FileOpenDialog, IFileOpenDialog,
        SIGDN_FILESYSPATH,
    };
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let uninit = init.is_ok();
        if init.is_err() && init != RPC_E_CHANGED_MODE {
            return pick_file_legacy(owner);
        }
        let helper = focus_owner();
        let result = (|| {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            if font {
                dialog
                    .SetFileTypes(&[windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC {
                        pszName: w!("Fonts (*.ttf, *.otf, *.ttc)"),
                        pszSpec: w!("*.ttf;*.otf;*.ttc"),
                    }])
                    .ok()?;
            }
            let options = dialog.GetOptions().ok()?;
            dialog
                .SetOptions(options | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR)
                .ok()?;
            match dialog.Show(helper.or(Some(owner))) {
                Ok(()) => {}
                Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => return Some(None),
                Err(_) => return None,
            }
            let item = dialog.GetResult().ok()?;
            let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let path = name.to_string().ok();
            CoTaskMemFree(Some(name.0.cast()));
            Some(path)
        })();
        if let Some(h) = helper {
            let _ = DestroyWindow(h);
        }
        if uninit {
            CoUninitialize();
        }
        match result {
            Some(path) => path,
            None => pick_file_legacy(owner),
        }
    }
}

fn focus_owner() -> Option<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()?;
        let _ = SetForegroundWindow(hwnd);
        Some(hwnd)
    }
}

fn pick_file_legacy(owner: HWND) -> Option<String> {
    use windows::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    let mut buffer = [0u16; 1024];
    let filter: Vec<u16> = "All files\0*.*\0\0".encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: windows::core::PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(buffer.as_mut_ptr()),
        nMaxFile: buffer.len() as u32,
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };
    unsafe { GetOpenFileNameW(&mut ofn).as_bool() }.then(|| {
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..len])
    })
}
