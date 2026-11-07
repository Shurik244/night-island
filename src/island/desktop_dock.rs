use crate::data::desktop_dock::{Pin, key};
use crate::island::*;
use crate::platform::desktop_dock as desktop;
use crate::ui::desktop_dock::{Dock, Request, WM_DESKTOP_DOCK};
use crate::ui::i18n::tr;

const IMPORT_TASKBAR: usize = 28;
const MEMORY_MANAGER: usize = 29;

impl App {
    pub(in crate::island) fn configure_desktop_dock(&mut self) {
        if !self.settings.desktop_dock.enabled {
            self.desktop_dock.take();
            self.configure_taskbar_replacement();
            return;
        }
        if !self.settings.desktop_dock.taskbar_imported {
            let untouched = self.settings.desktop_dock.pins.is_empty()
                || self.settings.desktop_dock.pins == crate::data::desktop_dock::DockSettings::default().pins;
            if untouched {
                crate::platform::taskbar_pins::start(self.hwnd, false);
            } else {
                self.settings.desktop_dock.taskbar_imported = true;
                self.ui.cfg.desktop_dock.taskbar_imported = true;
                self.settings.save();
            }
        }
        if let Some(dock) = &mut self.desktop_dock {
            dock.update(&self.settings.desktop_dock);
        } else {
            match Dock::new(self.hwnd, &self.compositor, &self.gpu, &self.settings.desktop_dock) {
                Ok(dock) => {
                    dock.set_capture_hidden(self.settings.capture_hidden);
                    self.desktop_dock = Some(dock);
                }
                Err(error) => {
                    self.ui.sw.dock_status = tr("Could not create the desktop dock").into_owned();
                    self.settings.desktop_dock.enabled = false;
                    self.ui.cfg = self.settings.clone();
                    crate::log_error!("desktop dock: {error}");
                }
            }
        }
        self.configure_taskbar_replacement();
    }

    pub(in crate::island) fn configure_taskbar_replacement(&mut self) {
        if self.desktop_dock.is_some() && self.settings.desktop_dock.replace_taskbar {
            if self.windows_taskbar.as_mut().is_none_or(|guard| !guard.running()) {
                self.windows_taskbar.take();
                match crate::platform::taskbar::TaskbarGuard::start() {
                    Ok(guard) => self.windows_taskbar = Some(guard),
                    Err(error) => crate::log_warn!("Could not start taskbar replacement: {error}"),
                }
            }
            self.set_timer(Timer::Taskbar, 2000);
        } else {
            self.windows_taskbar.take();
            self.kill_timer(Timer::Taskbar);
        }
    }

    pub(in crate::island) fn save_desktop_dock(&mut self) {
        self.settings.desktop_dock.sanitize();
        self.ui.cfg = self.settings.clone();
        self.settings.save();
        self.configure_desktop_dock();
        if self.settings_win.is_some() {
            self.render_settings_win();
        }
    }

    pub(in crate::island) fn desktop_dock_setting(&mut self, id: view::Sid) {
        match id {
            view::Sid::DockAdd | view::Sid::DockImport | view::Sid::DockImportTaskbar => unsafe {
                let _ = PostMessageW(
                    Some(self.hwnd),
                    WM_DESKTOP_DOCK,
                    WPARAM(match id {
                        view::Sid::DockAdd => 1,
                        view::Sid::DockImportTaskbar => crate::platform::taskbar_pins::REQUEST_IMPORT,
                        _ => 2,
                    }),
                    LPARAM(0),
                );
            },
            view::Sid::DockTaskbar => cmd::shell_open("ms-settings:taskbar", None),
            view::Sid::DockMonitor => {
                let monitors = desktop::monitors();
                let index = monitors.iter().position(|m| m.device == self.settings.desktop_dock.monitor);
                self.settings.desktop_dock.monitor = match index {
                    Some(i) if i + 1 < monitors.len() => monitors[i + 1].device.clone(),
                    None if !monitors.is_empty() => monitors[0].device.clone(),
                    _ => String::new(),
                };
                self.save_desktop_dock();
            }
            _ => {}
        }
    }
}

fn add_file(hwnd: HWND) {
    let owner = with_app_value(|a| a.settings_win.unwrap_or(hwnd)).unwrap_or(hwnd);
    if let Some(path) = pick_file(owner) {
        with_app(|a| {
            let added = a.settings.desktop_dock.add(Pin::from_path(&path));
            a.ui.sw.dock_status =
                tr(if added { "Application added to the dock" } else { "Already pinned or dock is full (32)" })
                    .into_owned();
            a.save_desktop_dock();
        });
    }
}

fn import_dock(hwnd: HWND) {
    let default = std::env::var_os("ProgramFiles(x86)")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("Steam/steamapps/common/MyDockFinder/ico.ini"));
    let file = default.filter(|p| p.is_file()).or_else(|| {
        let owner = with_app_value(|a| a.settings_win.unwrap_or(hwnd)).unwrap_or(hwnd);
        pick_file(owner).map(std::path::PathBuf::from)
    });
    let Some(file) = file else { return };
    let imported = std::fs::File::open(file).map_err(|_| "Could not read ico.ini").and_then(|file| {
        use std::io::Read;
        let mut bytes = Vec::new();
        file.take(256 * 1024 + 1).read_to_end(&mut bytes).map_err(|_| "Could not read ico.ini")?;
        crate::data::desktop_dock::import_mydock(&bytes)
    });
    with_app(|a| match imported {
        Ok(pins) => {
            let mut count = 0;
            for pin in pins {
                count += usize::from(a.settings.desktop_dock.add(pin));
            }
            a.ui.sw.dock_status = format!("{}: {count}", tr("Imported applications"));
            a.save_desktop_dock();
        }
        Err(message) => {
            a.ui.sw.dock_status = tr(message).into_owned();
            a.render_settings_win();
        }
    });
}

fn capture_application(entry: &crate::ui::desktop_dock::Entry) {
    let windows = current_windows(entry);
    with_app(|a| {
        if windows.iter().any(|w| w.island_settings) {
            a.settings.capture_hidden = !a.settings.capture_hidden;
            a.ui.cfg = a.settings.clone();
            a.apply_capture();
            a.settings.save();
            a.show_notice_for(
                '\u{E72E}',
                &tr(if a.settings.capture_hidden {
                    "Island is hidden from screen capture"
                } else {
                    "Island is visible in screen capture"
                }),
                3000,
            );
            a.render_settings_win();
        } else if windows.is_empty() {
            a.show_notice_for('\u{E783}', &tr("Start the application before changing capture protection"), 3000);
        } else if crate::platform::app_capture::start(a.hwnd, windows, a.settings.desktop_dock.experimental_capture) {
            a.show_notice_for('\u{E72E}', &tr("Checking capture protection…"), 1800);
        }
    });
}

pub(in crate::island) fn capture_result() {
    let Some(result) = crate::platform::app_capture::take_result() else { return };
    with_app(|a| {
        let message = if result.failed == 0 {
            format!(
                "{}: {}",
                tr(if result.hidden { "Windows excluded from capture" } else { "Capture protection removed" }),
                result.applied
            )
        } else if result.applied == 0 {
            tr(result.failure.unwrap_or(crate::platform::app_capture::Failure::Rejected).text()).into_owned()
        } else {
            format!("{}: {} · {}: {}", tr("Applied"), result.applied, tr("Not confirmed"), result.failed)
        };
        if result.failure == Some(crate::platform::app_capture::Failure::Disabled)
            && let Some(group) = view::GROUPS.iter().position(|g| g.name == "Desktop dock")
        {
            a.open_settings_at(group);
        }
        a.show_notice_for(if result.failed == 0 { '\u{E72E}' } else { '\u{E783}' }, &message, 6000);
        if let Some(dock) = &mut a.desktop_dock {
            dock.refresh_windows();
        }
    });
}

fn menu_title(value: &str) -> String {
    let mut text: String = value.chars().take(90).collect();
    if value.chars().count() > 90 {
        text.push('…');
    }
    text
}

fn current_windows(entry: &crate::ui::desktop_dock::Entry) -> Vec<desktop::Window> {
    desktop::windows()
        .into_iter()
        .filter(|w| entry.windows.iter().any(|old| old.handle == w.handle && key(&old.path) == key(&w.path)))
        .collect()
}

fn entry_app_path(entry: &crate::ui::desktop_dock::Entry) -> String {
    entry.windows.first().map(|w| w.path.clone()).unwrap_or_else(|| desktop::resolved_target(&entry.pin.target))
}

fn dock_context(entry: Option<&crate::ui::desktop_dock::Entry>) -> Option<usize> {
    use crate::ui::menu::{self, Entry};
    let mut point = windows::Win32::Foundation::POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    let scale = with_app_value(|a| a.desktop_dock.as_ref().map_or(a.dpi_scale, Dock::scale)).unwrap_or(1.0);
    let config = with_app_value(|a| a.settings.desktop_dock.clone()).unwrap_or_default();
    let mut page = 0;
    loop {
        let mut items = Vec::new();
        if let Some(entry) = entry {
            let own_settings = entry.windows.iter().any(|w| w.island_settings);
            items.push(Entry::info(&menu_title(&entry.pin.name)));
            match page {
                0 => {
                    if !entry.windows.is_empty() {
                        items.push(
                            Entry::item(25, &tr(if own_settings { "Island settings" } else { "Show application" }))
                                .glyph('\u{E8A7}'),
                        );
                    }
                    if !own_settings || entry.windows.is_empty() {
                        items.push(
                            Entry::item(1, &tr(if entry.windows.is_empty() { "Open" } else { "Open new window" }))
                                .glyph('\u{E710}'),
                        );
                    }
                    if !entry.windows.is_empty() {
                        if own_settings || crate::platform::app_capture::supported(&entry_app_path(entry)) {
                            items.push(
                                Entry::item(26, &tr("Toggle capture protection · Alt + right-click")).glyph('\u{E72E}'),
                            );
                        }
                        items.push(Entry::item(24, &tr("Window preview and controls")).glyph('\u{E714}'));
                        if entry.windows.len() > 1 {
                            items.push(
                                Entry::item(18, &format!("{} ({})…", tr("Windows"), entry.windows.len()))
                                    .glyph('\u{E737}'),
                            );
                        }
                        if entry.windows.iter().any(|w| w.minimized) {
                            items.push(Entry::item(11, &tr("Restore all windows")).glyph('\u{E923}'));
                        }
                        if entry.windows.iter().any(|w| !w.minimized) {
                            items.push(Entry::item(12, &tr("Minimize all windows")).glyph('\u{E921}'));
                        }
                        items.push(Entry::item(13, &tr("Close windows normally")).glyph('\u{E711}').danger());
                    }
                    items.push(Entry::divider());
                    items.push(Entry::item(8, &tr("More options…")).glyph('\u{E712}'));
                }
                1 => {
                    let state = if entry.windows.is_empty() { "Not running" } else { "Running" };
                    items.push(Entry::info(&format!(
                        "{} · {}: {} · {}: {}",
                        tr(state),
                        tr("Windows"),
                        entry.windows.len(),
                        tr("Minimized"),
                        entry.windows.iter().filter(|w| w.minimized).count()
                    )));
                    let target = std::path::Path::new(&entry.pin.target);
                    if target.exists() {
                        items.push(Entry::item(14, &tr("Show in folder")).glyph('\u{E838}'));
                        items.push(Entry::item(15, &tr("Properties")).glyph('\u{E946}'));
                        if !own_settings
                            && target
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("exe") || e.eq_ignore_ascii_case("lnk"))
                        {
                            items.push(Entry::item(17, &tr("Run as administrator")).glyph('\u{E7EF}'));
                        }
                    }
                    items.push(Entry::item(16, &tr("Copy application path")).glyph('\u{E8C8}'));
                    items.push(Entry::divider());
                    items.push(
                        Entry::item(
                            2,
                            &tr(if entry.pinned { "Unpin from desktop dock" } else { "Pin to desktop dock" }),
                        )
                        .glyph('\u{E718}'),
                    );
                    items.push(Entry::item(20, &tr("Dock behavior…")).glyph('\u{E713}'));
                    items.push(Entry::item(27, &tr("Edit dock shortcut")).glyph('\u{E70F}'));
                }
                2 => {
                    if let Some(i) = config.pins.iter().position(|p| key(&p.target) == key(&entry.pin.target)) {
                        if i > 0 {
                            items.push(Entry::item(3, &tr("Move left")).glyph('\u{E72B}'));
                        }
                        if i + 1 < config.pins.len() {
                            items.push(Entry::item(4, &tr("Move right")).glyph('\u{E72A}'));
                        }
                    }
                    let path = key(&entry_app_path(entry));
                    let hidden = config.hidden_apps.iter().any(|p| key(p) == path);
                    if !own_settings
                        && std::path::Path::new(&path).extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
                    {
                        items.push(
                            Entry::item(
                                10,
                                &tr(if hidden {
                                    "Stop hiding dock for this app"
                                } else {
                                    "Hide dock while this app is active"
                                }),
                            )
                            .glyph(if hidden { '\u{E73E}' } else { '\u{E739}' }),
                        );
                    }
                    items.push(
                        Entry::item(23, &tr(if config.auto_hide { "Keep dock visible" } else { "Auto-hide dock" }))
                            .glyph('\u{E8B3}'),
                    );
                }
                _ => {
                    for (i, window) in entry.windows.iter().enumerate() {
                        let state = if window.minimized {
                            "Minimized"
                        } else if window.active {
                            "Active"
                        } else {
                            "Open"
                        };
                        items.push(
                            Entry::item(100 + i, &format!("{} · {}", menu_title(&window.title), tr(state)))
                                .glyph(if window.active { '\u{E73E}' } else { '\u{E737}' }),
                        );
                    }
                }
            }
        } else {
            items.push(Entry::info("Night Island Dock"));
            items.push(Entry::info(&format!("{}: {}", tr("Pinned applications"), config.pins.len())));
            items.push(Entry::item(IMPORT_TASKBAR, &tr("Import Windows taskbar pins")).glyph('\u{E8B5}'));
            items.push(Entry::item(22, &tr("Import applications from MyDockFinder")).glyph('\u{E8B5}'));
            items.push(
                Entry::item(23, &tr(if config.auto_hide { "Keep dock visible" } else { "Auto-hide dock" }))
                    .glyph('\u{E8B3}'),
            );
        }
        if entry.is_none() || page == 1 {
            if with_app_value(|a| a.settings.memory_tools).unwrap_or(false) {
                items.push(Entry::item(MEMORY_MANAGER, &tr("Memory manager")).glyph('\u{E964}'));
            }
            items.push(Entry::item(5, &tr("Add application or file")).glyph('\u{E710}'));
            items.push(Entry::item(6, &tr("Desktop dock settings")).glyph('\u{E713}'));
            items.push(Entry::item(7, &tr("Island settings")).glyph('\u{E80F}'));
        }
        if page != 0 {
            items.push(Entry::divider());
            items.push(Entry::item(21, &tr("Back")).glyph('\u{E72B}'));
        }
        match menu::show(items, (point.x, point.y), true, scale, &|f| with_app(|a| f(&a.gpu))) {
            Some(8) => page = 1,
            Some(20) => page = 2,
            Some(18) => page = 3,
            Some(21) => page = if page == 2 { 1 } else { 0 },
            other => return other,
        }
    }
}

pub(in crate::island) fn handle_request(hwnd: HWND, command: usize) {
    if command == crate::platform::taskbar_pins::REQUEST_IMPORT {
        crate::platform::taskbar_pins::start(hwnd, true);
        return;
    }
    if command == crate::platform::taskbar_pins::IMPORT_READY {
        if let Some(result) = crate::platform::taskbar_pins::take() {
            with_app(|a| {
                let untouched = a.settings.desktop_dock.pins.is_empty()
                    || a.settings.desktop_dock.pins == crate::data::desktop_dock::DockSettings::default().pins;
                let mut added = 0;
                if result.manual || untouched {
                    let mut seen: std::collections::HashSet<_> = a
                        .settings
                        .desktop_dock
                        .pins
                        .iter()
                        .map(|pin| key(&desktop::resolved_target(&pin.target)))
                        .collect();
                    for (pin, target) in result.pins {
                        if seen.insert(key(&target)) {
                            added += usize::from(a.settings.desktop_dock.add(pin));
                        }
                    }
                }
                a.settings.desktop_dock.taskbar_imported = true;
                a.ui.sw.dock_status = format!("{}: {added}", tr("Imported applications"));
                a.save_desktop_dock();
            });
        }
        return;
    }
    if command == 1 {
        add_file(hwnd);
        return;
    }
    if command == 2 {
        import_dock(hwnd);
        return;
    }
    if command == 3 {
        with_app(|a| {
            if let Some(group) = view::GROUPS.iter().position(|g| g.name == "Integrations") {
                a.open_settings_at(group);
            }
        });
        return;
    }
    let mut request = None;
    with_app(|a| request = a.desktop_dock.as_mut().and_then(Dock::take_request));
    match request {
        Some(Request::Save) => with_app(|a| {
            if let Some(dock) = &a.desktop_dock {
                a.settings.desktop_dock = dock.config();
            }
            a.save_desktop_dock();
        }),
        Some(Request::Add) => add_file(hwnd),
        Some(Request::Capture(entry)) => capture_application(&entry),
        Some(Request::Launch(entry)) => {
            let live: Vec<_> = desktop::windows()
                .into_iter()
                .filter(|w| entry.windows.iter().any(|old| old.handle == w.handle && key(&old.path) == key(&w.path)))
                .collect();
            if live.is_empty() {
                cmd::shell_open(&entry.pin.target, None);
            } else {
                let index = live.iter().position(|w| w.active).map_or(0, |i| (i + 1) % live.len());
                desktop::activate(&live[index], live.len() == 1);
            }
        }
        Some(Request::Menu(entry)) => {
            let entry = entry.map(|mut entry| {
                entry.windows = current_windows(&entry);
                entry
            });
            let chosen = dock_context(entry.as_ref());
            with_app(|a| {
                if let Some(dock) = &mut a.desktop_dock {
                    dock.close_menu();
                }
            });
            match chosen {
                Some(26) => {
                    if let Some(entry) = entry {
                        capture_application(&entry);
                    }
                }
                Some(1) => {
                    if let Some(entry) = entry {
                        if let Some(window) = entry.windows.iter().find(|w| w.island_settings) {
                            desktop::activate(window, false);
                        } else {
                            cmd::shell_open(&entry.pin.target, None);
                        }
                    }
                }
                Some(2..=4) => {
                    if let Some(entry) = entry {
                        with_app(|a| {
                            let pins = &mut a.settings.desktop_dock.pins;
                            let index = pins.iter().position(|p| key(&p.target) == key(&entry.pin.target));
                            match (chosen, index) {
                                (Some(2), Some(i)) => {
                                    pins.remove(i);
                                }
                                (Some(2), None) => {
                                    let mut pin = entry.pin;
                                    if entry.windows.iter().any(|w| w.island_settings) {
                                        pin.name = "Night Island".into();
                                    }
                                    a.settings.desktop_dock.add(pin);
                                }
                                (Some(3), Some(i)) if i > 0 => pins.swap(i, i - 1),
                                (Some(4), Some(i)) if i + 1 < pins.len() => pins.swap(i, i + 1),
                                _ => {}
                            }
                            a.save_desktop_dock();
                        });
                    }
                }
                Some(5) => add_file(hwnd),
                Some(6) => with_app(|a| {
                    if let Some(group) = view::GROUPS.iter().position(|g| g.name == "Desktop dock") {
                        a.open_settings_at(group);
                    }
                }),
                Some(7) => with_app(|a| {
                    if let Some(i) = view::GROUPS.iter().position(|g| g.name == "Island") {
                        a.open_settings_at(i);
                    }
                }),
                Some(10) => {
                    if let Some(entry) = &entry {
                        let path = key(&entry_app_path(entry));
                        with_app(|a| {
                            let hidden = &mut a.settings.desktop_dock.hidden_apps;
                            if let Some(i) = hidden.iter().position(|p| key(p) == path) {
                                hidden.remove(i);
                            } else {
                                hidden.push(path);
                            }
                            a.save_desktop_dock();
                        });
                    }
                }
                Some(11..=13) => {
                    if let Some(entry) = &entry {
                        let action = match chosen {
                            Some(11) => desktop::WindowAction::Restore,
                            Some(12) => desktop::WindowAction::Minimize,
                            _ => desktop::WindowAction::Close,
                        };
                        for window in current_windows(entry) {
                            desktop::change_window(&window, action);
                        }
                    }
                }
                Some(14) => {
                    if let Some(entry) = &entry {
                        cmd::reveal(std::path::Path::new(&entry.pin.target));
                    }
                }
                Some(15) => {
                    if let Some(entry) = &entry {
                        desktop::file_properties(hwnd, &entry.pin.target);
                    }
                }
                Some(16) => {
                    if let Some(entry) = &entry {
                        clips::sys::write_text(hwnd, &entry_app_path(entry));
                    }
                }
                Some(17) => {
                    if let Some(entry) = &entry {
                        cmd::shell_open(&entry.pin.target, Some("runas"));
                    }
                }
                Some(25) => {
                    if let Some(window) = entry.as_ref().and_then(|e| e.windows.first()) {
                        desktop::activate(window, false);
                    }
                }
                Some(27) => {
                    if let Some(entry) = &entry
                        && let Some(gpu) = with_app_value(|a| a.gpu.popup())
                        && let Ok(Some(pin)) = crate::ui::shortcut_editor::show(hwnd, &gpu, &entry.pin, pick_file)
                    {
                        with_app(|a| {
                            let pins = &mut a.settings.desktop_dock.pins;
                            let old = pins.iter().position(|p| key(&p.target) == key(&entry.pin.target));
                            if pins
                                .iter()
                                .enumerate()
                                .any(|(i, p)| Some(i) != old && key(&p.target) == key(&pin.target))
                            {
                                a.show_notice_for('\u{E783}', &tr("Already pinned or dock is full (32)"), 3500);
                                return;
                            }
                            if let Some(i) = old {
                                pins[i] = pin;
                            } else {
                                a.settings.desktop_dock.add(pin);
                            }
                            a.save_desktop_dock();
                        });
                    }
                }
                Some(24) => {
                    if let Some(entry) = &entry {
                        with_app(|a| {
                            if let Some(dock) = &mut a.desktop_dock {
                                dock.show_preview(entry);
                            }
                        });
                    }
                }
                Some(22) => import_dock(hwnd),
                Some(IMPORT_TASKBAR) => crate::platform::taskbar_pins::start(hwnd, true),
                Some(MEMORY_MANAGER) => with_app(|a| {
                    if let Some(group) = view::GROUPS.iter().position(|g| g.name == "System") {
                        a.open_settings_at(group);
                        a.open_memory_tools();
                    }
                }),
                Some(23) => with_app(|a| {
                    a.settings.desktop_dock.auto_hide = !a.settings.desktop_dock.auto_hide;
                    a.save_desktop_dock();
                }),
                Some(id) if id >= 100 => {
                    if let Some(window) = entry.as_ref().and_then(|e| e.windows.get(id - 100)) {
                        desktop::activate(window, false);
                    }
                }
                _ => {}
            }
        }
        None => {}
    }
}
