use std::collections::VecDeque;

use crate::island::*;
use crate::platform::notifications::{self, Record, Status};
use crate::ui::notification_card::{self, NotificationCard};

const ASSISTANT_REPLY_ID: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::island) enum CardPhase {
    Opening,
    Showing,
    Closing,
}

#[derive(Default)]
pub(in crate::island) struct SystemNotices {
    pub origin: Option<view::Rect>,
    pub compact_reply: bool,
    window: Option<NotificationCard>,
    queue: VecDeque<Record>,
    shown: VecDeque<(u32, i64, String)>,
    active: Option<Record>,
    pub phase: Option<CardPhase>,
    hovered: bool,
    locked: bool,
    main_hidden: bool,
    replaces_island: bool,
    pet_source: Option<view::Rect>,
    expanded_source: Option<view::Rect>,
    expires: Option<Instant>,
    remaining: Duration,
    failed: bool,
}

fn same(a: &Record, b: &Record) -> bool {
    a.id == b.id && a.created == b.created && a.app_id == b.app_id
}
fn example(record: &Record) -> bool {
    record.app_id == "night-island-example" && record.id == u32::MAX
}

fn local(record: &Record) -> bool {
    record.app_id == notifications::LOCAL_APP_ID
}

impl App {
    pub(in crate::island) fn pet_notification_visible(&self) -> bool {
        self.system_notices.phase.is_some() && self.system_notices.pet_source.is_some()
    }

    pub(in crate::island) fn post_local_notification(&mut self, kind: Notice, body: &str) {
        let title = match kind {
            Notice::Ai => "Assistant",
            Notice::Error => "Error",
            _ => "Notification",
        };
        let name = if self.settings.pet_notifications && self.pet.desktop.is_some() {
            if self.settings.character == Character::Custom {
                self.settings.custom_pet.name.clone()
            } else {
                crate::ui::i18n::tr(self.settings.character.name()).into_owned()
            }
        } else {
            "Night Island".into()
        };
        if self.system_notices.queue.len() >= 32 {
            self.system_notices.queue.pop_front();
        }
        self.system_notices.queue.push_back(Record {
            id: if kind == Notice::Ai { ASSISTANT_REPLY_ID } else { 0 },
            created: notifications::now_ticks(),
            app_id: notifications::LOCAL_APP_ID.into(),
            app: name,
            title: crate::ui::i18n::tr(title).into_owned(),
            body: body.chars().take(4000).collect(),
            icon: None,
            avatar: None,
        });
        self.maybe_show_notification();
    }

    pub(in crate::island) fn notification_occupied_rect(&self) -> Option<view::Rect> {
        self.notification_card_visible()
            .then(|| self.system_notices.window.as_ref().map(NotificationCard::target_rect))
            .flatten()
    }

    pub(in crate::island) fn notification_card_visible(&self) -> bool {
        self.system_notices.phase.is_some() && self.system_notices.replaces_island
    }

    pub(in crate::island) fn configure_system_notifications(&mut self) {
        self.system_notices.failed = false;
        notifications::configure(self.hwnd, self.settings.system_notifications);
        if !self.settings.system_notifications {
            self.system_notices.queue.retain(local);
            if self.system_notices.active.as_ref().is_none_or(|record| !local(record)) {
                self.hide_notification_card();
            }
            self.ui.notifications.preview = None;
            self.ui.notifications.full = false;
            self.ui.notifications.state = Default::default();
        }
    }

    pub(in crate::island) fn on_system_notifications(&mut self) {
        self.accept_system_notifications(notifications::take_snapshot());
    }

    fn accept_system_notifications(&mut self, mut state: notifications::Snapshot) {
        if !self.settings.system_notifications {
            state = Default::default();
        }
        self.system_notices
            .queue
            .retain(|record| local(record) || example(record) || state.records.iter().any(|item| same(item, record)));
        for pending in &mut self.system_notices.queue {
            if let Some(record) = state.records.iter().find(|item| same(item, pending)) {
                *pending = record.clone();
            }
        }
        if state.status != Status::Ready {
            self.system_notices.queue.retain(|record| local(record) || example(record));
            if self.system_notices.active.as_ref().is_some_and(|record| !local(record) && !example(record)) {
                self.hide_notification_card();
            }
        }
        if let Some(active) = &self.system_notices.active
            && !local(active)
            && !example(active)
            && !state.records.iter().any(|item| same(item, active))
        {
            self.dismiss_notification_card();
        }
        for record in state.added.drain(..) {
            if self.settings.system_notifications
                && state.status == Status::Ready
                && !self.system_notices.active.as_ref().is_some_and(|active| same(active, &record))
                && !self.system_notices.queue.iter().any(|pending| same(pending, &record))
                && !self.ui.notifications.preview.as_ref().is_some_and(|preview| same(preview, &record))
                && !self
                    .system_notices
                    .shown
                    .iter()
                    .any(|(id, created, app)| *id == record.id && *created == record.created && *app == record.app_id)
            {
                self.system_notices.queue.push_back(record);
            }
        }
        if let Some(active) = &mut self.system_notices.active
            && let Some(record) = state.records.iter().find(|record| same(record, active))
        {
            *active = record.clone();
        }
        self.ui.notifications.state = state;
        if let Some(preview) = &mut self.ui.notifications.preview
            && let Some(record) = self.ui.notifications.state.records.iter().find(|record| same(record, preview))
        {
            *preview = record.clone();
        }
        let rows = view::notifications_rows(self.pill_rect(true), self.scale);
        self.ui.notifications.scroll =
            self.ui.notifications.scroll.min(self.ui.notifications.state.records.len().saturating_sub(rows));
        self.update_notification_card();
        self.maybe_show_notification();
        if self.state == State::Expanded && matches!(self.ui.tab, view::Tab::Notifications | view::Tab::Home) {
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn click_notifications(&mut self, x: i32, y: i32) {
        match view::hit_notifications(
            self.pill_rect(true),
            self.scale,
            x as f32,
            y as f32,
            &self.ui.notifications,
            self.settings.notification_open_app,
        ) {
            Some(view::NotificationsHit::Row(i)) => {
                self.ui.notifications.preview = self.ui.notifications.state.records.get(i).cloned();
                self.expand_notification_text();
            }
            Some(view::NotificationsHit::Back) => {
                self.ui.notifications.full = false;
                self.select_tab(view::Tab::Home);
            }
            Some(view::NotificationsHit::Dismiss) => {
                self.ui.notifications.preview = None;
                self.ui.notifications.full = false;
                self.select_tab(view::Tab::Home);
            }
            Some(view::NotificationsHit::OpenApp) => self.activate_notification(),
            Some(view::NotificationsHit::Connect) => {
                match self.ui.notifications.state.status {
                    Status::NeedsAccess => {
                        if notifications::request_access(self.hwnd).is_err() {
                            self.ui.notifications.state.status = Status::Unavailable;
                        }
                    }
                    Status::Denied => cmd::shell_open("ms-settings:privacy-notifications", None),
                    Status::Unavailable => {
                        self.settings.system_notifications = true;
                        self.configure_system_notifications();
                    }
                    _ => {
                        self.settings.system_notifications = !self.settings.system_notifications;
                        self.configure_system_notifications();
                    }
                }
                self.ui.cfg = self.settings.clone();
                self.settings.save();
                self.render_settings_win();
                self.repaint_expanded();
            }
            Some(view::NotificationsHit::WindowsSettings) => cmd::shell_open("ms-settings:notifications", None),
            Some(view::NotificationsHit::Setup) => {
                self.ui.notifications.setup = true;
                self.repaint_expanded();
            }
            Some(view::NotificationsHit::History) => {
                self.ui.notifications.setup = false;
                self.repaint_expanded();
            }
            Some(view::NotificationsHit::Example) => {
                self.system_notices.queue.push_front(Record {
                    id: u32::MAX,
                    created: notifications::now_ticks(),
                    app_id: "night-island-example".into(),
                    app: "Night Island".into(),
                    title: crate::ui::i18n::tr("Notifications from the island").into_owned(),
                    body: crate::ui::i18n::tr("Your Windows notifications will appear here. Hover to keep reading.")
                        .into_owned(),
                    icon: None,
                    avatar: None,
                });
                self.collapse();
            }
            None => {}
        }
    }

    pub(in crate::island) fn maybe_show_notification(&mut self) {
        let pet = self
            .pet
            .desktop
            .as_ref()
            .filter(|pet| self.settings.pet_notifications && unsafe { IsWindowVisible(pet.hwnd()).as_bool() });
        let local_pending = self.system_notices.queue.front().is_some_and(local);
        let expanded = self.state == State::Expanded;
        if self.system_notices.phase.is_some()
            || self.system_notices.queue.is_empty()
            || (self.system_notices.compact_reply && self.ui.notice.is_some())
            || self.system_notices.locked
            || self.system_notices.failed
            || self.gaming
            || (pet.is_none() && (!matches!(self.state, State::Collapsed | State::Expanded) || self.pill_hidden))
            || self.character_interacting()
            || self.pill_motion_busy()
            || self.pointer.moving.is_some()
            || self.pointer.press.is_some()
            || (pet.is_none() && !unsafe { IsWindowVisible(self.hwnd).as_bool() })
            || (pet.is_none() && !local_pending && !expanded && (self.ui.notice.is_some() || self.ui.volume.is_some()))
        {
            return;
        }
        let record = self.system_notices.queue.pop_front().unwrap();
        if !local(&record)
            && !example(&record)
            && (!self.settings.system_notifications || self.ui.notifications.state.status != Status::Ready)
        {
            return;
        }
        let pet_anchor = pet.map(|pet| pet.notification_anchor());
        if pet_anchor.is_none()
            && !expanded
            && self.settings.ai_compact_replies
            && local(&record)
            && record.id == ASSISTANT_REPLY_ID
        {
            let work = self.island_work_area();
            let available = if self.settings.vertical() { work.3 - work.1 } else { work.2 - work.0 };
            if view::compact_reply_fits(&self.ui, &record.body, available / self.scale - 32.0) {
                self.show_compact_reply(record.body.trim());
                return;
            }
        }
        self.reset_liquid_pull();
        if self.system_notices.window.is_none() {
            match NotificationCard::new(self.hwnd, &self.compositor, &self.gpu, self.scale) {
                Ok(window) => self.system_notices.window = Some(window),
                Err(error) => {
                    crate::log_warn!("notification card could not open: {error}");
                    self.system_notices.failed = true;
                    self.ui.notifications.state.status = Status::Unavailable;
                    self.system_notices.queue.push_front(record);
                    return;
                }
            }
        }
        let content = self.settings.notification_adaptive_size.then_some(&record);
        let (source, target) = if let Some((source, work)) = pet_anchor {
            (source, notification_card::pet_card_target(source, work, self.scale, false, content))
        } else if expanded {
            notification_card::expanded_card_layout(
                self.pill_screen(true),
                self.pill_screen(false),
                self.island_work_area(),
                self.scale,
                content,
            )
        } else {
            let source = self.pill_screen(false);
            let target = notification_card::card_target(
                source,
                self.island_work_area(),
                self.docked().then_some(self.edge()),
                self.scale,
                content,
            );
            (source, target)
        };
        let window = self.system_notices.window.as_mut().unwrap();
        window.set_capture_hidden(self.settings.capture_hidden);
        if let Err(error) = window.show(&record, self.system_notices.queue.len(), source, target, self.scale) {
            crate::log_warn!("notification card could not render: {error}");
            window.hide();
            self.system_notices.failed = true;
            self.ui.notifications.state.status = Status::Unavailable;
            self.system_notices.queue.push_front(record);
            return;
        }
        self.system_notices.pet_source = pet_anchor.map(|(source, _)| source);
        self.system_notices.expanded_source = (pet_anchor.is_none() && expanded).then_some(source);
        self.system_notices.replaces_island = pet_anchor.is_none() && !expanded;
        if !local(&record) && !example(&record) {
            if self.system_notices.shown.len() >= 64 {
                self.system_notices.shown.pop_front();
            }
            self.system_notices.shown.push_back((record.id, record.created, record.app_id.clone()));
        }
        self.system_notices.active = Some(record);
        self.system_notices.phase = Some(CardPhase::Opening);
        self.system_notices.hovered = false;
        self.system_notices.remaining = Duration::from_secs(7);
        self.system_notices.expires = None;
        self.set_timer(Timer::NotificationStage, notification_card::OPEN_MS + 25);
        if !self.system_notices.replaces_island {
            return;
        }
        self.kill_timer(Timer::Autohide);
        self.kill_timer(Timer::Expand);
        self.kill_timer(Timer::Liquid);
        self.set_tip(None);
        if let Ok(fade) = self.compositor.CreateScalarKeyFrameAnimation() {
            let _ = fade.InsertKeyFrame(1.0, 0.0);
            let _ = fade.SetDuration(Duration::from_millis(120).into());
            let _ = self.root.StartAnimation(h!("Opacity"), &fade);
        }
        self.update_eq();
        self.update_weather_timer();
        self.start_progress_timer();
        self.update_top_panel(false);
    }

    pub(in crate::island) fn update_notification_card(&self) {
        if let (Some(window), Some(record)) = (&self.system_notices.window, &self.system_notices.active) {
            window.set_capture_hidden(self.settings.capture_hidden);
            let _ = window.update(record, self.system_notices.queue.len());
        }
    }

    fn start_notification_timeout(&mut self) {
        if self.system_notices.phase == Some(CardPhase::Showing) && !self.system_notices.hovered {
            let remaining = self.system_notices.remaining.max(Duration::from_millis(750));
            self.system_notices.expires = Some(Instant::now() + remaining);
            self.set_timer(Timer::NotificationExpire, remaining.as_millis().min(u128::from(u32::MAX)) as u32);
        }
    }

    pub(in crate::island) fn on_notification_card(&mut self, action: usize, source: isize) {
        if self.system_notices.window.as_ref().is_none_or(|window| window.hwnd().0 as isize != source)
            || self.system_notices.phase.is_none()
        {
            return;
        }
        match action {
            value if value == notification_card::Action::Enter as usize => {
                self.system_notices.hovered = true;
                if let Some(expires) = self.system_notices.expires.take() {
                    self.system_notices.remaining = expires.saturating_duration_since(Instant::now());
                }
                self.kill_timer(Timer::NotificationExpire);
                self.set_timer(Timer::NotificationHover, notification_card::OPEN_MS + 200);
            }
            value if value == notification_card::Action::Leave as usize => {
                self.system_notices.hovered = false;
                self.kill_timer(Timer::NotificationHover);
                self.update_notification_card();
                self.start_notification_timeout();
            }
            value if value == notification_card::Action::Dismiss as usize => self.dismiss_notification_card(),
            value
                if value == notification_card::Action::ScrollUp as usize
                    || value == notification_card::Action::ScrollDown as usize =>
            {
                if let Some(window) = &mut self.system_notices.window {
                    window.scroll(value == notification_card::Action::ScrollDown as usize);
                }
                self.update_notification_card();
            }
            value if value == notification_card::Action::History as usize => {
                if let (Some(source), Some(pet), Some(window), Some(record)) = (
                    self.system_notices.pet_source,
                    &self.pet.desktop,
                    &mut self.system_notices.window,
                    &self.system_notices.active,
                ) && (local(record) || !self.settings.notification_open_app)
                {
                    let (_, work) = pet.notification_anchor();
                    let _ = window.expand(source, work, record, self.system_notices.queue.len());
                    return;
                }
                if self.system_notices.active.as_ref().is_some_and(local) {
                    self.ui.notifications.preview = self.system_notices.active.clone();
                    self.hide_notification_card();
                    self.restore_pill();
                    self.expand_notification_text();
                } else if self.settings.notification_open_app {
                    let record = self.system_notices.active.clone();
                    self.hide_notification_card();
                    if let Some(record) = record {
                        notifications::open_sender(record, self.hwnd);
                    }
                    self.maybe_show_notification();
                } else {
                    self.open_notification_preview();
                }
            }
            _ => {
                self.update_notification_card();
                if self.system_notices.hovered {
                    self.set_timer(Timer::NotificationHover, 350);
                }
            }
        }
    }

    pub(in crate::island) fn notification_stage(&mut self) {
        match self.system_notices.phase {
            Some(CardPhase::Opening) => {
                self.system_notices.phase = Some(CardPhase::Showing);
                if self.system_notices.replaces_island && !self.pill_hidden {
                    self.system_notices.main_hidden = true;
                    unsafe {
                        let _ = ShowWindow(self.hwnd, SW_HIDE);
                    }
                }
                self.start_notification_timeout();
            }
            Some(CardPhase::Closing) => {
                self.hide_notification_card();
                self.maybe_show_notification();
            }
            _ => {}
        }
    }

    pub(in crate::island) fn dismiss_notification_card(&mut self) {
        if self.system_notices.phase.is_none() || self.system_notices.phase == Some(CardPhase::Closing) {
            return;
        }
        self.kill_timer(Timer::NotificationExpire);
        self.system_notices.expires = None;
        let source = self
            .system_notices
            .pet_source
            .or(self.system_notices.expanded_source)
            .unwrap_or_else(|| self.pill_screen(false));
        if let (Some(window), Some(record)) = (&mut self.system_notices.window, &self.system_notices.active) {
            let _ = window.close(source, record, self.system_notices.queue.len());
        }
        self.system_notices.phase = Some(CardPhase::Closing);
        self.set_timer(Timer::NotificationStage, notification_card::CLOSE_MS + 25);
    }

    pub(in crate::island) fn hide_notification_card(&mut self) {
        self.kill_timer(Timer::NotificationHover);
        if self.system_notices.phase.take().is_none() {
            return;
        }
        self.kill_timer(Timer::NotificationStage);
        self.kill_timer(Timer::NotificationExpire);
        if let Some(window) = &self.system_notices.window {
            window.hide();
        }
        self.system_notices.active = None;
        self.system_notices.expires = None;
        self.system_notices.hovered = false;
        self.system_notices.pet_source = None;
        self.system_notices.expanded_source = None;
        if !std::mem::take(&mut self.system_notices.replaces_island) {
            return;
        }
        let _ = self.root.StopAnimation(h!("Opacity"));
        let _ = self.root.SetOpacity(1.0);
        if self.system_notices.main_hidden && !self.gaming && !self.pill_hidden {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
        }
        self.system_notices.main_hidden = false;
        self.update_eq();
        self.update_weather_timer();
        self.start_progress_timer();
        self.start_liquid();
    }

    pub(in crate::island) fn notification_hover(&mut self) {
        self.kill_timer(Timer::NotificationHover);
        if self.system_notices.hovered
            && self.system_notices.replaces_island
            && self.system_notices.active.as_ref().is_some_and(|record| !local(record))
            && self.system_notices.phase == Some(CardPhase::Showing)
            && self.system_notices.window.as_ref().is_some_and(|w| !w.hovering_close())
        {
            self.open_notification_preview();
        }
    }

    fn open_notification_preview(&mut self) {
        let Some(record) = self.system_notices.active.clone() else {
            return;
        };
        self.ui.notifications.preview = Some(record);
        self.ui.notifications.full = false;
        self.ui.home.edit = false;
        self.ui.home.tile_press = None;
        self.ui.home.tile_reorder = None;
        self.system_notices.origin = self.system_notices.window.as_ref().map(|w| w.screen_rect());
        self.open_tab(view::Tab::Home);
    }

    pub(in crate::island) fn expand_notification_text(&mut self) {
        if self.ui.notifications.preview.is_none() {
            return;
        }
        self.ui.notifications.full = true;
        self.ui.notifications.detail_scroll = 0.0;
        self.ui.notifications.hover = None;
        self.open_tab(view::Tab::Notifications);
        self.repaint_expanded();
    }

    pub(in crate::island) fn activate_notification(&mut self) {
        let Some(record) = &self.ui.notifications.preview else {
            return;
        };
        if !local(record) {
            notifications::open_sender(record.clone(), self.hwnd);
        }
    }

    pub(in crate::island) fn notification_session(&mut self, locked: bool) {
        self.system_notices.locked = locked;
        if locked {
            if let Some(record) = self.system_notices.active.clone() {
                self.system_notices.queue.push_front(record);
            }
            self.hide_notification_card();
        } else {
            self.maybe_show_notification();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "system" fn test_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn record(id: u32) -> Record {
        Record {
            id,
            created: i64::from(id),
            app_id: "test".into(),
            app: "Telegram".into(),
            title: "Александр".into(),
            body: "Привет! Встречаемся сегодня в 19:30? Я уже забронировал столик у окна.".into(),
            icon: None,
            avatar: None,
        }
    }

    fn preview(app: &mut App, path: &std::path::Path) -> Result<()> {
        use crate::ui::gfx::rgba;
        let surface = crate::ui::popup::PopupSurface::new((1320, 600))?;
        app.ui.mixer.state.ready = true;
        app.ui.mixer.state.apps = [
            ("Яндекс Музыка", 0.72, false),
            ("Discord", 0.45, false),
            ("Google Chrome", 0.25, true),
            ("System sounds", 0.6, false),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (name, level, muted))| crate::platform::audio_mixer::AppVolume {
            key: i.to_string(),
            name: name.into(),
            level,
            muted,
            active: i < 2,
            icon: None,
        })
        .collect();
        surface.paint(&app.gpu, |p| {
            p.fill_rect((0.0, 0.0, 1320.0, 600.0), rgba(0.025, 0.035, 0.065, 1.0))?;
            app.ui.tab = view::Tab::Home;
            app.ui.home.mixer_open = true;
            app.ui.home.device = "Динамики".into();
            p.fill_round_rect((10.0, 10.0, 650.0, 394.0), 28.0, rgba(0.0, 0.0, 0.0, 1.0))?;
            view::paint_expanded(p, (10.0, 10.0, 640.0, 384.0), 1.0, &app.ui, &[])?;
            app.ui.tab = view::Tab::Notifications;
            p.fill_round_rect((670.0, 10.0, 1310.0, 394.0), 28.0, rgba(0.0, 0.0, 0.0, 1.0))?;
            view::paint_expanded(p, (670.0, 10.0, 640.0, 384.0), 1.0, &app.ui, &[])?;
            p.fill_round_rect((450.0, 420.0, 870.0, 578.0), 26.0, rgba(0.0, 0.0, 0.0, 1.0))?;
            let mut notification = record(1);
            let bgra = (0..96 * 96)
                .flat_map(|i| {
                    let x = (i % 96) as f32 - 48.0;
                    let y = (i / 96) as f32;
                    if x * x + (y - 35.0).powi(2) < 16.0 * 16.0
                        || (x / 31.0).powi(2) + ((y - 92.0) / 36.0).powi(2) < 1.0
                    {
                        [235, 220, 200, 255]
                    } else {
                        [160, 94, 55, 255]
                    }
                })
                .collect();
            notification.avatar = Some(std::sync::Arc::new(apps::Icon { side: 96, bgra }));
            notification.icon = Some(std::sync::Arc::new(apps::Icon { side: 1, bgra: vec![220, 150, 30, 255] }));
            view::paint_notification_card(p, (450.0, 420.0, 420.0, 158.0), 1.0, &notification, 2, false, None)
                .map(|_| ())
        })?;
        let pixels: Vec<u8> = surface.pixels().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 1320, 600);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        Ok(())
    }

    #[test]
    #[ignore = "Requires Windows Composition; uses hidden windows and synthetic notifications"]
    fn notification_cards_render_and_restore_island_without_replaying_or_losing_pending_records() -> Result<()> {
        use windows::Win32::System::WinRT::RoUninitialize;
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
        let instance = unsafe { GetModuleHandleW(None)? };
        let class = WNDCLASSW {
            lpfnWndProc: Some(test_proc),
            hInstance: instance.into(),
            lpszClassName: w!("NightIsland.Notifications.Test"),
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&class);
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW,
                class.lpszClassName,
                w!("Notification test"),
                WS_POPUP,
                -32000,
                -32000,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        let settings =
            Settings { music_eq: false, edge_spikes: false, system_notifications: true, ..Settings::default() };
        let mut app = App::new_visuals(hwnd, 1.0, settings)?;
        app.ui.cfg = app.settings.clone();
        app.set_decoration_through(true);
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        assert_ne!(style & WS_EX_TRANSPARENT.0 as isize, 0);
        app.reset_liquid_pull();
        assert_eq!(unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize, 0);
        app.pill_hidden = true;
        let a = record(1);
        let b = record(2);
        let state = notifications::Snapshot {
            status: Status::Ready,
            records: vec![a.clone(), b.clone()],
            added: vec![a.clone(), a.clone(), b.clone()],
        };
        app.accept_system_notifications(state);
        assert_eq!(app.system_notices.queue.len(), 2);
        let mut card = NotificationCard::new(hwnd, &app.compositor, &app.gpu, 1.0)?;
        for scale in [0.75, 1.0, 2.0] {
            for edge in Edge::ALL.into_iter().map(Some).chain([None]) {
                let source = (-6000.0, -6000.0, 200.0 * scale, 34.0 * scale);
                let target =
                    notification_card::card_target(source, (-8000.0, -8000.0, -4000.0, -4000.0), edge, scale, Some(&a));
                card.show(&a, 1, source, target, scale)?;
                card.update(&b, 0)?;
                card.close(source, &b, 0)?;
                card.hide();
            }
        }
        app.system_notices.window = Some(card);
        app.system_notices.active = app.system_notices.queue.pop_front();
        app.system_notices.phase = Some(CardPhase::Opening);
        app.system_notices.replaces_island = true;
        app.pill_hidden = false;
        app.notification_stage();
        assert!(app.system_notices.phase == Some(CardPhase::Showing) && app.system_notices.main_hidden);
        let source = app.system_notices.window.as_ref().unwrap().hwnd().0 as isize;
        app.on_notification_card(notification_card::Action::Enter as usize, source);
        assert!(app.system_notices.hovered && app.system_notices.expires.is_none());
        app.on_notification_card(notification_card::Action::Leave as usize, source);
        assert!(!app.system_notices.hovered && app.system_notices.expires.is_some());
        app.notification_session(true);
        assert_eq!(app.system_notices.queue.len(), 2);
        assert!(app.system_notices.locked && !app.notification_card_visible());
        assert_eq!(app.root.Opacity()?, 1.0);
        app.pill_hidden = true;
        app.notification_session(false);
        assert!(!app.system_notices.locked);
        app.accept_system_notifications(notifications::Snapshot {
            status: Status::Ready,
            records: vec![b.clone()],
            added: Vec::new(),
        });
        assert_eq!(app.system_notices.queue.len(), 1);
        app.system_notices.active = app.system_notices.queue.pop_front();
        app.system_notices.phase = Some(CardPhase::Showing);
        app.accept_system_notifications(notifications::Snapshot { status: Status::Denied, ..Default::default() });
        assert!(app.system_notices.queue.is_empty() && !app.notification_card_visible());
        assert_eq!(app.root.Opacity()?, 1.0);
        app.ui.notifications.state =
            notifications::Snapshot { status: Status::Ready, records: vec![a, b], added: Vec::new() };
        app.system_notices.active = Some(record(1));
        app.system_notices.phase = Some(CardPhase::Showing);
        app.system_notices.replaces_island = true;
        app.pill_hidden = false;
        app.system_notices.hovered = true;
        app.notification_hover();
        assert!(app.state == State::Expanded && app.ui.tab == view::Tab::Home);
        assert!(app.ui.notifications.preview.is_some() && !app.notification_card_visible());
        app.expand_notification_text();
        assert!(app.ui.notifications.full && app.ui.tab == view::Tab::Notifications);
        app.ui.notifications.preview = None;
        app.ui.notifications.full = false;
        if let Some(path) = std::env::var_os("NI_NOTIFICATION_TILES_PREVIEW") {
            preview_tiles(&mut app, std::path::Path::new(&path))?;
        }
        if let Some(path) = std::env::var_os("NI_NOTIFICATIONS_PREVIEW") {
            preview(&mut app, std::path::Path::new(&path))?;
        }
        app.hide_notification_card();
        app.cancel_pill_motion();
        app.ui.tab = view::Tab::Home;
        app.ui.notifications.preview = None;
        let c = record(3);
        let incoming = notifications::Snapshot {
            status: Status::Ready,
            records: vec![c.clone()],
            added: vec![c.clone(), c.clone()],
        };
        app.accept_system_notifications(incoming.clone());
        assert!(app.state == State::Expanded && app.ui.notifications.preview.is_none());
        assert!(app.system_notices.phase == Some(CardPhase::Opening));
        assert!(app.system_notices.expanded_source.is_some() && !app.system_notices.replaces_island);
        assert!(app.system_notices.active.as_ref().is_some_and(|active| same(active, &c)));
        assert!(app.system_notices.queue.is_empty());
        app.notification_stage();
        assert!(!app.system_notices.main_hidden);
        app.dismiss_notification_card();
        app.notification_stage();
        app.accept_system_notifications(incoming);
        assert!(app.system_notices.active.is_none() && app.system_notices.queue.is_empty());
        assert!(app.ui.notifications.preview.is_none());
        app.state = State::Collapsed;
        app.cancel_pill_motion();
        app.notify_for(Notice::Ai, '\u{E8F2}', "Музыка на паузе.", 7000);
        assert!(app.system_notices.compact_reply);
        assert_eq!(app.ui.notice.as_ref().unwrap().1, "Музыка на паузе.");
        assert!(app.system_notices.active.is_none() && app.system_notices.phase.is_none());
        app.notify_for(Notice::Ai, '\u{E8F2}', "Готово.", 7000);
        assert_eq!(app.system_notices.queue.len(), 1);
        assert_eq!(app.ui.notice.as_ref().unwrap().1, "Музыка на паузе.");
        app.cancel_pill_motion();
        app.on_timer(Timer::Notice as usize);
        assert_eq!(app.ui.notice.as_ref().unwrap().1, "Готово.");
        app.notify_for(Notice::Ai, '\u{E8F2}', "Готово.", 7000);
        app.notify_for(Notice::Ai, '\u{E8F2}', "Готово.", 7000);
        app.cancel_pill_motion();
        app.on_timer(Timer::Notice as usize);
        assert_eq!(app.system_notices.queue.len(), 1);
        assert_eq!(app.ui.notice.as_ref().unwrap().1, "Готово.");
        app.cancel_pill_motion();
        app.on_timer(Timer::Notice as usize);
        assert!(app.system_notices.queue.is_empty());
        assert_eq!(app.ui.notice.as_ref().unwrap().1, "Готово.");
        app.cancel_pill_motion();
        app.on_timer(Timer::Notice as usize);
        app.cancel_pill_motion();
        app.maybe_show_notification();
        assert!(app.ui.notice.is_none() && app.system_notices.queue.is_empty());
        assert!(app.system_notices.phase.is_none());
        app.settings.ai_compact_replies = false;
        app.notify_for(Notice::Ai, '\u{E8F2}', "Музыка на паузе.", 7000);
        assert!(app.system_notices.phase == Some(CardPhase::Opening));
        let target = app.system_notices.window.as_ref().unwrap().target_rect();
        assert!(target.2 < view::NOTIFICATION_SIZE.0 && target.3 < view::NOTIFICATION_SIZE.1);
        app.hide_notification_card();
        app.cancel_pill_motion();
        app.settings.ai_compact_replies = true;
        app.pill_hidden = true;
        app.notify_for(Notice::Ai, '\u{E8F2}', "Музыка на паузе.", 7000);
        assert!(app.ui.notice.is_none() && app.system_notices.queue.len() == 1);
        app.pill_hidden = false;
        app.maybe_show_notification();
        assert!(app.system_notices.compact_reply && app.system_notices.queue.is_empty());
        app.agent = Some(crate::services::assistant::agent::Run::new(String::new(), "test", &[]));
        assert!(app.cancel_voice_input());
        assert!(app.ui.notice.is_none());
        app.cancel_pill_motion();
        app.state = State::Expanded;
        app.notify_for(Notice::Ai, '\u{E8F2}', "Готово.", 7000);
        assert!(app.system_notices.expanded_source.is_some());
        assert!(app.ui.notice.is_none());
        app.hide_notification_card();
        app.cancel_pill_motion();
        if let Some(path) = std::env::var_os("NI_COMPACT_REPLIES_PREVIEW") {
            preview_compact_replies(&mut app, std::path::Path::new(&path))?;
        }
        app.settings.character = Character::Spark;
        app.settings.character_detached = true;
        app.apply_character();
        app.char_tick();
        app.toggle_island_from_character();
        assert!(app.pill_hidden);
        app.post_local_notification(Notice::Ai, "Музыка на паузе.");
        app.maybe_show_notification();
        assert!(app.pet_notification_visible());
        app.notification_stage();
        assert!(app.pill_hidden && !app.system_notices.main_hidden);
        assert!(app.system_notices.active.as_ref().is_some_and(local));
        let source = app.system_notices.window.as_ref().unwrap().hwnd().0 as isize;
        app.on_notification_card(notification_card::Action::History as usize, source);
        assert!(app.pill_hidden && app.pet_notification_visible());
        app.accept_system_notifications(notifications::Snapshot::default());
        assert!(app.pet_notification_visible());
        app.pet.voice_phase = crate::ui::pet_speech::VoicePhase::Command;
        app.agent = Some(crate::services::assistant::agent::Run::new(String::new(), "test", &[]));
        assert!(app.cancel_voice_input());
        assert!(app.agent.is_none() && !app.pet.voice_phase.cue().engaged());
        app.dismiss_notification_card();
        assert!(app.system_notices.phase == Some(CardPhase::Closing));
        app.notification_stage();
        assert!(app.pill_hidden && !app.notification_card_visible());
        drop(app);
        unsafe {
            DestroyWindow(hwnd)?;
        }
        Ok(())
    }

    fn preview_compact_replies(app: &mut App, path: &std::path::Path) -> Result<()> {
        use crate::ui::gfx::rgba;
        let surface = crate::ui::popup::PopupSurface::new((1000, 430))?;
        surface.paint(&app.gpu, |p| {
            p.fill_rect((0.0, 0.0, 1000.0, 430.0), rgba(0.025, 0.035, 0.065, 1.0))?;
            let mut ui = view::UiState::new();
            ui.notice = Some(('\u{E8F2}', "Музыка на паузе.".into()));
            let width = view::collapsed_width(&ui, 180.0);
            let pill = (500.0 - width / 2.0, 28.0, width, 34.0);
            p.fill_round_rect((pill.0, pill.1, pill.0 + pill.2, pill.1 + pill.3), 17.0, rgba(0.0, 0.0, 0.0, 1.0))?;
            view::paint_collapsed(p, pill, 1.0, &ui)?;
            for (x, body) in [(40.0, "Музыка на паузе.".to_string()), (520.0, "Я приостановил музыку. Чтобы продолжить прослушивание, попроси меня включить её или нажми кнопку воспроизведения.\n\nГромкость «Моей волны» сохранена.".into())] {
                let record = Record { app: "Night Island".into(), title: "Ассистент".into(), body, ..record(1) };
                let width = view::notification_card_width(&record);
                let height = view::notification_card_height(&record, width, 1.0);
                let rect = (x, 130.0, width, height);
                p.fill_round_rect((x, 130.0, x + width, 130.0 + height), 26.0, rgba(0.0, 0.0, 0.0, 1.0))?;
                view::paint_notification_card(p, rect, 1.0, &record, 0, false, None)?;
            }
            Ok(())
        })?;
        let pixels: Vec<u8> = surface.pixels().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 1000, 430);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        Ok(())
    }

    fn preview_tiles(app: &mut App, path: &std::path::Path) -> Result<()> {
        use crate::ui::gfx::rgba;
        let surface = crate::ui::popup::PopupSurface::new((1320, 820))?;
        app.ui.tab_change = None;
        let mut notification = record(42);
        notification.app = "Discord".into();
        notification.body = "Встречаемся в голосовом канале вечером? Я подготовил несколько идей для нового проекта.\n\nЕсли будешь свободен после восьми — заходи, всё обсудим. ".repeat(8);
        notification.avatar = Some(std::sync::Arc::new(apps::Icon {
            side: 32,
            bgra: (0..1024)
                .flat_map(|i| {
                    if (i % 32 - 16_i32).pow(2) + (i / 32 - 12_i32).pow(2) < 49 {
                        [235, 220, 200, 255]
                    } else {
                        [170, 100, 70, 255]
                    }
                })
                .collect(),
        }));
        notification.icon = Some(std::sync::Arc::new(apps::Icon { side: 1, bgra: vec![240, 100, 90, 255] }));
        app.ui.home.mixer_open = false;
        app.ui.cfg.home_actions = vec![0, 3, 22];
        app.ui.cfg.notification_open_app = true;
        app.ui.home.device = "Динамики".into();
        app.ui.notifications.preview = Some(notification);
        surface.paint(&app.gpu, |p| {
            p.fill_rect((0.0, 0.0, 1320.0, 820.0), rgba(0.025, 0.035, 0.055, 1.0))?;
            for (i, count) in [1, 2, 4, 4].into_iter().enumerate() {
                let (x, y) = (10.0 + (i % 2) as f32 * 660.0, 10.0 + (i / 2) as f32 * 410.0);
                app.ui.cfg.notification_tiles = count;
                app.ui.tab = if i == 3 { view::Tab::Notifications } else { view::Tab::Home };
                app.ui.notifications.full = i == 3;
                p.fill_round_rect((x, y, x + 640.0, y + 384.0), 28.0, rgba(0.0, 0.0, 0.0, 1.0))?;
                view::paint_expanded(p, (x, y, 640.0, 384.0), 1.0, &app.ui, &[])?;
            }
            Ok(())
        })?;
        assert!(app.ui.notifications.detail_max.get() > 0.0);
        let pixels: Vec<u8> = surface.pixels().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 1320, 820);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        let saved_tabs = app.ui.cfg.header_tabs.clone();
        app.ui.tab = view::Tab::Home;
        app.ui.notifications.full = false;
        surface.paint(&app.gpu, |p| {
            p.fill_rect((0.0, 0.0, 1320.0, 820.0), rgba(0.025, 0.035, 0.055, 1.0))?;
            for i in 0..4 {
                app.ui.cfg.header_tabs = match i {
                    0 => crate::data::settings::DEFAULT_HEADER_TABS.map(String::from).to_vec(),
                    1 => ["Clipboard", "Text", "Note", "Weather", "Record", "Notifications"].map(String::from).to_vec(),
                    2 => ["Privacy", "Activity", "Controls", "App mixer", "Notifications"].map(String::from).to_vec(),
                    _ => crate::data::settings::HIDEABLE_TABS.map(String::from).to_vec(),
                };
                let (x, y) = (10.0 + (i % 2) as f32 * 660.0, 10.0 + (i / 2) as f32 * 410.0);
                p.fill_round_rect((x, y, x + 640.0, y + 384.0), 28.0, rgba(0.0, 0.0, 0.0, 1.0))?;
                view::paint_expanded(p, (x, y, 640.0, 384.0), 1.0, &app.ui, &[])?;
            }
            Ok(())
        })?;
        let pixels: Vec<u8> = surface.pixels().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        let mut encoder = png::Encoder::new(
            std::fs::File::create(path.with_file_name("notification-header-preview.png")).unwrap(),
            1320,
            820,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        app.ui.cfg.header_tabs = saved_tabs;
        for (group, filename) in [
            ("Integrations", "notification-integrations-preview.png"),
            ("Notifications", "notification-settings-preview.png"),
        ] {
            app.ui.sw.group = view::GROUPS.iter().position(|g| g.name == group).unwrap();
            app.ui.sw.scroll = 0.0;
            app.ui.yandex.state.has_token = true;
            app.ui.yandex.state.account = "Пример аккаунта".into();
            let surface = crate::ui::popup::PopupSurface::new((940, 680))?;
            surface.paint(&app.gpu, |p| view::paint_settings_window(p, (940.0, 680.0), 1.0, &app.ui))?;
            let pixels: Vec<u8> = surface.pixels().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
            let mut encoder =
                png::Encoder::new(std::fs::File::create(path.with_file_name(filename)).unwrap(), 940, 680);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        Ok(())
    }
}
