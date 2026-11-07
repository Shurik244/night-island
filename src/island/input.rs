use crate::island::*;

#[derive(Default)]
pub(in crate::island) struct Pointer {
    pub(in crate::island) expand_pending: std::cell::Cell<bool>,
    pub(in crate::island) decoration_through: bool,
    pub(in crate::island) tracking: bool,
    pub(in crate::island) press: Option<((i32, i32), (i32, i32))>,
    pub(in crate::island) character_press: Option<(i32, i32)>,
    pub(in crate::island) moving: Option<((i32, i32), (i32, i32))>,
    pub(in crate::island) drag_from: Option<(usize, i32, i32)>,
    pub(in crate::island) selecting: bool,
    pub(in crate::island) reading: bool,
    pub(in crate::island) slider: Option<view::Sid>,
    pub(in crate::island) vol_drag: bool,
    pub(in crate::island) seek_drag: bool,
}

impl App {
    pub(in crate::island) fn mouse_down(&mut self, x: i32, y: i32) {
        if self.state == State::Expanded {
            self.ui.pressed_action = self.ui.home.hover_act;
            self.repaint_expanded();
        }
        if self.can_release_character(x, y) {
            let mut point = windows::Win32::Foundation::POINT::default();
            unsafe {
                if GetCursorPos(&mut point).is_ok() {
                    self.pointer.character_press = Some((point.x, point.y));
                    windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(self.hwnd);
                }
            }
            self.kill_timer(Timer::Expand);
            self.kill_timer(Timer::Autohide);
            self.reset_liquid_pull();
            self.set_tip(None);
            return;
        }
        if self.state == State::Collapsed && self.ui.notice.is_none() && self.on_pill(x, y) {
            let mut pt = windows::Win32::Foundation::POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut pt);
            }
            self.pointer.press = Some(((pt.x, pt.y), (x, y)));
            return;
        }
        if self.state == State::Expanded && !self.ui.cmd.open && self.ui.remind.alert.is_none() && !self.ui.editing {
            let pill = self.pill_rect(true);
            let (fx, fy) = (x as f32, y as f32);
            let in_header = fy >= pill.1 + 6.0 * self.scale
                && fy < pill.1 + 52.0 * self.scale
                && fx >= pill.0
                && fx < pill.0 + pill.2;
            if in_header
                && view::hit_tab(self.pill_full(), self.scale, fx, fy, &self.ui).is_none()
                && view::hit_head(self.pill_full(), self.scale, &self.ui, fx, fy).is_none()
                && !view::hit_vpn_btn(self.pill_full(), self.scale, &self.ui, fx, fy)
            {
                let mut pt = windows::Win32::Foundation::POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut pt);
                }
                self.pointer.press = Some(((pt.x, pt.y), (x, y)));
                return;
            }
        }
        self.click_at(x, y);
    }

    pub(in crate::island) fn drag_island(&mut self) {
        let Some((start_cursor, _)) = self.pointer.press else { return };
        if self.settings.lock_drag {
            return;
        }
        let mut pt = windows::Win32::Foundation::POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        if self.pointer.moving.is_none() {
            let far = (pt.x - start_cursor.0).abs().max((pt.y - start_cursor.1).abs()) > (6.0 * self.scale) as i32;
            if !far {
                return;
            }
            let origin = self.window_origin(Win::Small);
            self.pointer.moving = Some(((origin.0.round() as i32, origin.1.round() as i32), start_cursor));
            self.kill_timer(Timer::Expand);
            self.kill_timer(Timer::Liquid);
            self.set_bump_visible(false);
            unsafe {
                windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(self.hwnd);
            }
        }
        if let Some((origin, from)) = self.pointer.moving {
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    origin.0 + pt.x - from.0,
                    origin.1 + pt.y - from.1,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
    }

    pub(in crate::island) fn mouse_up(&mut self) {
        if self.ui.pressed_action.take().is_some() {
            self.repaint_expanded();
        }
        if self.pointer.character_press.take().is_some() {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
            self.set_character_detached(true);
            return;
        }
        let press = self.pointer.press.take();
        if self.pointer.moving.take().is_none() {
            if let Some((_, (x, y))) = press {
                self.click_at(x, y);
            }
            return;
        }
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
        }
        let mut rc = windows::Win32::Foundation::RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut rc);
        }
        let s = self.scale;
        let (sw, sh) = Self::screen_size();
        let (wl, wt, wr, wb) = self.island_work_area();
        let (cw, ch) = if self.state == State::Expanded {
            (EXPANDED.0 * s, EXPANDED.1 * s)
        } else {
            (self.collapsed().0 * s, self.collapsed().1 * s)
        };
        let pill = if self.state == State::Expanded { self.pill_full() } else { self.pill_rect(false) };
        let (px, py) = (rc.left as f32 + pill.0, rc.top as f32 + pill.1);
        let (mid_x, mid_y) = (px + cw / 2.0, py + ch / 2.0);
        if self.docked() {
            let near = [
                (py - wt, Edge::Top),
                (wb - (py + ch), Edge::Bottom),
                (px - wl, Edge::Left),
                (wr - (px + cw), Edge::Right),
            ]
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(Edge::Top, |e| e.1);
            self.settings.dock_edge = near;
            self.settings.dock_offset = match near {
                Edge::Top | Edge::Bottom => (mid_x / sw).clamp(0.0, 1.0),
                Edge::Left | Edge::Right => (mid_y / sh).clamp(0.0, 1.0),
            };
        } else {
            let near = [
                (py - wt, Edge::Top),
                (wb - (py + ch), Edge::Bottom),
                (px - wl, Edge::Left),
                (wr - (px + cw), Edge::Right),
            ]
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .filter(|e| e.0 < 36.0 * s);
            if let Some((_, edge)) = near {
                self.settings.placement = Placement::Docked;
                self.settings.dock_edge = edge;
                self.settings.dock_offset = match edge {
                    Edge::Top | Edge::Bottom => (mid_x / sw).clamp(0.0, 1.0),
                    Edge::Left | Edge::Right => (mid_y / sh).clamp(0.0, 1.0),
                };
            } else {
                self.settings.float_pos = Some([mid_x.round() as i32, py.round() as i32]);
            }
        }
        self.ui.cfg = self.settings.clone();
        self.settings.save();
        self.apply_layout();
        if self.state == State::Collapsed {
            self.start_liquid();
        }
    }

    pub(in crate::island) fn hover_at(&mut self, x: i32, y: i32) {
        if self.state != State::Expanded {
            return;
        }
        let Some((x, y)) = self.open_content_point(x, y) else { return };
        let pill = self.pill_rect(true);
        let (fx, fy) = (x as f32, y as f32);
        if self.ui.remind.alert.is_some() {
            let hover = view::hit_alert(pill, self.scale, fx, fy).map(|i| 1500 + i);
            self.ui.hand = hover.is_some();
            if hover != self.ui.home.hover_act {
                self.ui.home.hover_act = hover;
                self.repaint_expanded();
            }
            return;
        }
        if self.ui.cmd.open {
            let row = view::hit_cmd(pill, self.scale, fx, fy, self.ui.cmd.items.len());
            let button = view::hit_cmd_button(pill, self.scale, fx, fy, &self.ui).map(|i| 200 + i);
            self.ui.hand = row.is_some() || button.is_some();
            let mut changed = button != self.ui.home.hover_act;
            self.ui.home.hover_act = button;
            if let Some(i) = row
                && i != self.ui.cmd.sel
            {
                self.ui.cmd.sel = i;
                changed = true;
            }
            if changed {
                self.repaint_expanded();
            }
            return;
        }
        if self.ui.tab == view::Tab::Mixer || (self.ui.tab == view::Tab::Home && self.ui.home.mixer_open) {
            let hover = self.mixer_hit(fx, fy);
            if self.ui.mixer.hover != hover {
                self.ui.mixer.hover = hover;
                self.repaint_expanded();
            }
            self.ui.hand = hover.is_some();
        }
        if self.ui.tab == view::Tab::Yandex {
            let hover = view::hit_yandex(pill, self.scale, &self.ui.yandex, fx, fy);
            if hover != self.ui.yandex.hover {
                self.ui.yandex.hover = hover;
                self.repaint_expanded();
            }
        }
        if self.ui.tab == view::Tab::Notifications {
            let hover = view::hit_notifications(
                pill,
                self.scale,
                fx,
                fy,
                &self.ui.notifications,
                self.settings.notification_open_app,
            );
            if hover != self.ui.notifications.hover {
                self.ui.notifications.hover = hover;
                self.repaint_expanded();
            }
        }
        let hover_tab = view::hit_tab(self.pill_full(), self.scale, fx, fy, &self.ui);
        let hover_clip = match self.ui.tab {
            view::Tab::Clipboard => {
                let visible = self.clipboard.clips.len().saturating_sub(self.ui.clip.scroll);
                view::hit_clip(pill, self.scale, fx, fy, visible)
            }
            _ => None,
        };
        let hover_ctl = if self.ui.tab == view::Tab::Controls { view::hit_ctl(pill, self.scale, fx, fy) } else { None };
        let hover_note = if self.ui.tab == view::Tab::Note {
            view::hit_note_tree(pill, self.scale, fx, fy, &self.ui)
                .or_else(|| view::hit_note_panel(pill, self.scale, fx, fy, &self.ui))
        } else {
            None
        };
        let hover_text = if self.ui.tab == view::Tab::Text {
            view::hit_text(pill, self.scale, fx, fy, &self.ui.cfg, &self.ui.text)
        } else {
            None
        };
        let hover_act = if self.ui.tab == view::Tab::Home && self.ui.home.dev_menu.is_some() {
            let count = self.ui.home.dev_menu.as_ref().map_or(0, |m| m.0.len());
            match view::hit_menu(pill, self.scale, fx, fy, count, view::home_wide(&self.ui)) {
                Some(Some(i)) => Some(300 + i),
                _ => None,
            }
        } else if self.ui.tab == view::Tab::Agy {
            match view::hit_agy(
                pill,
                self.scale,
                fx,
                fy,
                self.ui.agy.chats.len(),
                self.ui.agy.scroll,
                self.ui.agy.open,
                self.ui.agy.input_h.get(),
            ) {
                Some(view::AgyHit::Row(i)) => Some(1000 + i),
                Some(view::AgyHit::Send) => Some(1100),
                Some(view::AgyHit::Back) => Some(1101),
                _ => None,
            }
        } else if self.ui.tab == view::Tab::Record {
            match view::hit_record(pill, self.scale, fx, fy) {
                Some(view::RecHit::Start) => Some(1400),
                Some(view::RecHit::Folder) => Some(1401),
                None => None,
            }
        } else if self.ui.tab == view::Tab::Privacy {
            let button = view::usage_clear_button(pill, self.scale);
            (fx >= button.0 && fx < button.2 && fy >= button.1 && fy < button.3).then_some(1300)
        } else if self.ui.tab == view::Tab::Steam {
            match view::hit_steam(pill, self.scale, fx, fy, &self.ui.steam) {
                Some(view::SteamHit::Link(i)) => Some(1400 + i),
                Some(view::SteamHit::Start) => Some(1405),
                Some(view::SteamHit::Account(i)) => Some(1410 + i),
                Some(view::SteamHit::Status(k)) => Some(1420 + k),
                Some(view::SteamHit::Game(i)) => Some(1430 + i),
                Some(view::SteamHit::Update(i)) => Some(1450 + i),
                Some(view::SteamHit::Search) => None,
                None => None,
            }
        } else if self.ui.tab == view::Tab::Activity {
            match view::hit_activity(pill, self.scale, fx, fy, self.settings.activity_track, &self.ui.activity) {
                Some(view::ActivityHit::Clear) => Some(1301),
                Some(view::ActivityHit::TurnOn) => Some(1310),
                Some(view::ActivityHit::Back) => Some(1319),
                Some(view::ActivityHit::Group(i)) => Some(1320 + i),
                Some(view::ActivityHit::App(i)) => Some(1340 + i),
                Some(view::ActivityHit::Pick(i)) => Some(1360 + i),
                _ => None,
            }
        } else if self.ui.tab == view::Tab::Chat {
            match view::hit_chat(pill, self.scale, fx, fy, &self.ui) {
                Some(view::ChatHit::Send) => Some(1200),
                Some(view::ChatHit::New) => Some(1201),
                Some(view::ChatHit::Chats) => Some(1202),
                Some(view::ChatHit::Attach) => Some(1203),
                Some(view::ChatHit::Row(i)) => Some(1220 + i),
                Some(view::ChatHit::RowDelete(i)) => Some(1240 + i),
                Some(view::ChatHit::ChipRemove(i)) => Some(1260 + i),
                Some(view::ChatHit::Setup) => Some(1210),
                Some(view::ChatHit::Pet) => Some(1204),
                Some(view::ChatHit::PetAction(view::PetStudioHit::Apply)) => Some(1280),
                Some(view::ChatHit::PetAction(view::PetStudioHit::Photo)) => Some(1281),
                Some(view::ChatHit::PetAction(view::PetStudioHit::TextOnly)) => Some(1282),
                _ => None,
            }
        } else if self.ui.tab == view::Tab::Weather {
            match view::hit_city_match(pill, self.scale, fx, fy, self.ui.city.matches.len(), self.ui.weather.is_some())
            {
                Some(i) => Some(910 + i),
                None => view::hit_city(pill, self.scale, fx, fy, self.ui.weather.is_some()).then_some(900),
            }
        } else if self.ui.tab == view::Tab::Home {
            match view::hit_home(pill, self.scale, fx, fy, &self.ui) {
                Some(view::HomeHit::Action(i)) => Some(i),
                Some(view::HomeHit::Device) => Some(100),
                Some(view::HomeHit::Media(i)) => Some(500 + i),
                Some(view::HomeHit::Extra(i)) => Some(510 + i),
                Some(view::HomeHit::Seek(_)) => Some(520),
                Some(view::HomeHit::DiscordDeaf) => Some(540),
                Some(view::HomeHit::DiscordMute) => Some(541),
                Some(view::HomeHit::DiscordOverlay) => Some(542),
                Some(view::HomeHit::Dock(i)) => Some(600 + i),
                Some(view::HomeHit::DockUnpin(i)) => Some(800 + i),
                Some(view::HomeHit::DockAdd | view::HomeHit::DockMore) => Some(700),
                Some(view::HomeHit::VolumeIcon) => Some(530),
                Some(view::HomeHit::Edit) => Some(550),
                Some(view::HomeHit::Player) => Some(560),
                Some(view::HomeHit::Wave) => Some(561),
                Some(view::HomeHit::Notification) => Some(562),
                Some(view::HomeHit::NotificationExpand) => Some(563),
                _ => None,
            }
        } else {
            None
        };
        let hover_head = view::hit_head(self.pill_full(), self.scale, &self.ui, fx, fy);
        let tip = self.tip_at(pill, hover_tab, hover_head).or_else(|| {
            let (text, r) = view::content_tip(pill, self.scale, &self.ui, self.clipboard.clips.len(), fx, fy)?;
            Some((text, ((r.0 + r.2) / 2.0, r.3 + 4.0 * self.scale)))
        });
        self.set_tip(tip);
        if hover_act != self.ui.home.hover_act || hover_head != self.ui.hover_head {
            self.ui.home.hover_act = hover_act;
            self.ui.hover_head = hover_head;
            if self.ui.tab == view::Tab::Activity {
                self.set_timer(Timer::UiAnim, self.frame_ms());
            }
            self.repaint_expanded();
        }
        self.ui.hand = hover_act.is_some()
            || hover_head.is_some()
            || hover_tab.is_some()
            || hover_clip.is_some()
            || hover_ctl.is_some()
            || hover_note.is_some()
            || hover_text.is_some()
            || ((self.ui.tab == view::Tab::Mixer || (self.ui.tab == view::Tab::Home && self.ui.home.mixer_open))
                && self.ui.mixer.hover.is_some())
            || (self.ui.tab == view::Tab::Yandex && self.ui.yandex.hover.is_some())
            || (self.ui.tab == view::Tab::Notifications && self.ui.notifications.hover.is_some());
        let hover_sub = view::hit_subtab(self.pill_full(), self.scale, fx, fy, &self.ui);
        self.ui.hand |= hover_sub.is_some();
        if hover_tab != self.ui.hover_tab
            || hover_sub != self.ui.hover_sub
            || hover_clip != self.ui.clip.hover
            || hover_ctl != self.ui.hover_ctl
            || hover_text != self.ui.text.hover
            || hover_note != self.ui.notes.hover
        {
            self.ui.notes.hover = hover_note;
            self.ui.text.hover = hover_text;
            self.ui.hover_tab = hover_tab;
            self.ui.hover_sub = hover_sub;
            self.ui.clip.hover = hover_clip;
            self.ui.hover_ctl = hover_ctl;
            self.repaint_expanded();
        }
    }

    fn tip_at(
        &self,
        _pill: view::Rect,
        tab: Option<usize>,
        head: Option<view::HeadHit>,
    ) -> Option<(String, (f32, f32))> {
        let s = self.scale;
        let below = |r: view::Rect| ((r.0 + r.2) / 2.0, r.3 + 4.0 * s);
        if let Some(i) = tab.filter(|&i| view::TABS[i].0 != view::bar_tab(self.ui.tab)) {
            let rect = view::tab_rects(self.pill_full(), s, &self.ui)[i];
            return Some((view::TABS[i].2.to_string(), below(rect)));
        }
        let (text, rect) = match head? {
            view::HeadHit::Pin => {
                (if self.ui.pinned { "Let it close" } else { "Keep open" }, view::pin_rect(self.pill_full(), s))
            }
            view::HeadHit::More => ("More tabs and actions", view::more_rect(self.pill_full(), s)),
            view::HeadHit::Mic => {
                let muted = self.ui.mic_muted == Some(true);
                (
                    if muted { "Unmute microphone" } else { "Mute microphone" },
                    view::privacy_dots(self.pill_full(), s, &self.ui).0?,
                )
            }
            view::HeadHit::Cam => ("Camera in use", view::privacy_dots(self.pill_full(), s, &self.ui).1?),
        };
        Some((text.to_string(), below(rect)))
    }

    pub(in crate::island) fn set_tip(&mut self, tip: Option<(String, (f32, f32))>) {
        if tip.as_ref().map(|t| &t.0) == self.ui.tip.as_ref().map(|t| &t.0) {
            return;
        }
        crate::ui::tip::hide();
        self.kill_timer(Timer::Tip);
        if tip.is_some() {
            self.set_timer(Timer::Tip, 450);
        }
        self.ui.tip = tip;
    }

    pub(in crate::island) fn show_tip(&mut self) {
        let Some((text, (x, y))) = &self.ui.tip else { return };
        let mut pt = windows::Win32::Foundation::POINT { x: x.round() as i32, y: y.round() as i32 };
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::ClientToScreen(self.hwnd, &mut pt);
        }
        crate::ui::tip::show(&self.gpu, &crate::ui::i18n::tr(text), (pt.x, pt.y), self.scale);
    }

    pub(in crate::island) fn click_at(&mut self, x: i32, y: i32) {
        self.set_tip(None);
        if self.state == State::Collapsed {
            return self.click_collapsed(x, y);
        }
        if self.state != State::Expanded {
            return;
        }
        let Some((x, y)) = self.open_content_point(x, y) else { return };
        if self.ui.remind.alert.is_some() {
            return self.click_alert(x, y);
        }
        if self.ui.cmd.open {
            return self.click_cmd(x, y);
        }
        match view::hit_head(self.pill_full(), self.scale, &self.ui, x as f32, y as f32) {
            Some(view::HeadHit::Pin) => {
                self.ui.pinned = !self.ui.pinned;
                if self.ui.pinned {
                    self.kill_timer(Timer::Collapse);
                }
                self.repaint_expanded();
                return;
            }
            Some(view::HeadHit::More) => {
                self.want_header_menu = true;
                return;
            }
            Some(view::HeadHit::Mic) => return self.toggle_mic(),
            Some(view::HeadHit::Cam) => return,
            None => {}
        }
        if view::hit_vpn_btn(self.pill_full(), self.scale, &self.ui, x as f32, y as f32) {
            return self.toggle_vpn();
        }
        if let Some(sub) = view::hit_subtab(self.pill_full(), self.scale, x as f32, y as f32, &self.ui) {
            if self.ui.tab != sub {
                self.ui.clip.hover = None;
                self.ui.hover_ctl = None;
                self.disarm();
                self.select_tab(sub);
            }
            return;
        }
        if let Some(i) = view::hit_tab(self.pill_full(), self.scale, x as f32, y as f32, &self.ui) {
            if view::TABS[i].0 == view::Tab::Settings {
                return self.open_settings_window();
            }
            if self.settings.tab_drag {
                self.ui.tab_press = Some((i, x, y));
                return;
            }
            if view::bar_tab(self.ui.tab) != view::TABS[i].0 {
                self.ui.clip.hover = None;
                self.ui.hover_ctl = None;
                self.disarm();
                let target = self.landing_tab(view::TABS[i].0);
                self.select_tab(target);
            }
            return;
        }
        match self.ui.tab {
            view::Tab::Clipboard => self.click_clip(x, y),
            view::Tab::Controls => self.click_ctl(x, y),
            view::Tab::Yandex => self.click_yandex(x, y),
            view::Tab::Mixer => self.click_mixer(x, y),
            view::Tab::Notifications => self.click_notifications(x, y),
            view::Tab::Note => self.click_note(x, y),
            view::Tab::Text => self.click_text(x, y),
            view::Tab::Record => self.click_record(x, y),
            view::Tab::Home => self.click_home(x, y),
            view::Tab::Weather => self.click_weather(x, y),
            view::Tab::Ai => {}
            view::Tab::Agy => self.click_agy(x, y),
            view::Tab::Chat => self.click_chat(x, y),
            view::Tab::Privacy => self.click_privacy(x, y),
            view::Tab::Activity => self.click_activity(x, y),
            view::Tab::Steam => self.click_steam(x, y),
            view::Tab::Settings => {}
        }
    }

    pub(in crate::island) fn wheel(&mut self, delta: i32) {
        if self.state == State::Collapsed {
            return self.adjust_volume(delta);
        }
        if self.state != State::Expanded || self.ui.cmd.open {
            return;
        }
        if let Some((items, scroll)) = &mut self.ui.home.dev_menu {
            let max = items.len().saturating_sub(view::menu_visible(items.len()));
            *scroll = if delta > 0 { scroll.saturating_sub(1) } else { (*scroll + 1).min(max) };
            return self.repaint_expanded();
        }
        if self.ui.tab == view::Tab::Agy && self.ui.agy.open {
            let step = 54.0 * self.scale * if delta > 0 { 1.0 } else { -1.0 };
            self.ui.agy.hist = (self.ui.agy.hist + step).clamp(0.0, self.ui.agy.max.get());
            return self.repaint_expanded();
        }
        if self.ui.tab == view::Tab::Home && self.ui.home.edit {
            let scroll = &mut self.ui.home.edit_scroll;
            *scroll = if delta > 0 { scroll.saturating_sub(1) } else { (*scroll + 1).min(view::home_edit_rows()) };
            return self.repaint_expanded();
        }
        if self.ui.tab == view::Tab::Steam {
            let most = self.ui.steam.games.len().saturating_sub(view::steam_rows(self.pill_rect(true), self.scale));
            let scroll = &mut self.ui.steam.scroll;
            *scroll = if delta > 0 { scroll.saturating_sub(1) } else { (*scroll + 1).min(most) };
            return self.repaint_expanded();
        }
        if self.ui.tab == view::Tab::Activity && self.ui.activity.open.is_some() {
            let most = self.ui.activity.open_total().map_or(0, |t| t.apps.len().saturating_sub(1));
            let scroll = &mut self.ui.activity.scroll;
            *scroll = if delta > 0 { scroll.saturating_sub(1) } else { (*scroll + 1).min(most) };
            return self.repaint_expanded();
        }
        if self.ui.tab == view::Tab::Chat && self.ui.chat.list_open {
            let most = self.ui.chat.list.len().saturating_sub(view::chat_rows());
            let scroll = &mut self.ui.chat.list_scroll;
            *scroll = if delta > 0 { scroll.saturating_sub(1) } else { (*scroll + 1).min(most) };
            return self.repaint_expanded();
        }
        if self.ui.tab == view::Tab::Chat {
            let step = 54.0 * self.scale * if delta > 0 { 1.0 } else { -1.0 };
            self.ui.chat.hist = (self.ui.chat.hist + step).clamp(0.0, self.ui.chat.max.get());
            return self.repaint_expanded();
        }
        if self.ui.tab == view::Tab::Notifications {
            if self.ui.notifications.full {
                let amount = if delta > 0 { -48.0 } else { 48.0 } * self.scale;
                self.ui.notifications.detail_scroll =
                    (self.ui.notifications.detail_scroll + amount).clamp(0.0, self.ui.notifications.detail_max.get());
                return self.repaint_expanded();
            }
            let max = self
                .ui
                .notifications
                .state
                .records
                .len()
                .saturating_sub(view::notifications_rows(self.pill_rect(true), self.scale));
            self.ui.notifications.scroll = if delta > 0 {
                self.ui.notifications.scroll.saturating_sub(1)
            } else {
                (self.ui.notifications.scroll + 1).min(max)
            };
            return self.repaint_expanded();
        }
        if self.ui.tab == view::Tab::Mixer || (self.ui.tab == view::Tab::Home && self.ui.home.mixer_open) {
            if self.ui.mixer.dragging.is_none() {
                let max = self.ui.mixer.state.apps.len().saturating_sub(if self.ui.tab == view::Tab::Home {
                    3
                } else {
                    view::mixer_visible_rows(self.pill_rect(true), self.scale)
                });
                self.ui.mixer.scroll = if delta > 0 {
                    self.ui.mixer.scroll.saturating_sub(1)
                } else {
                    (self.ui.mixer.scroll + 1).min(max)
                };
                self.repaint_expanded();
            }
            return;
        }
        let rows = if self.ui.tab == view::Tab::Agy {
            view::agy_rows()
        } else if self.ui.tab == view::Tab::Privacy {
            view::privacy_rows(self.pill_rect(true), self.scale)
        } else {
            view::clip_rows(self.pill_rect(true), self.scale)
        };
        if self.ui.tab == view::Tab::Note && self.cursor_over_tree() {
            return self.scroll_tree(delta);
        }
        if self.ui.tab == view::Tab::Text
            && let Some((x, y)) = self.cursor_client()
            && view::over_text_side(self.pill_rect(true), self.scale, x, y, &self.ui.text)
        {
            let max = view::chip_scroll_max(self.pill_rect(true), self.scale, &self.ui.cfg, &self.ui.text);
            let scroll = &mut self.ui.text.chip_scroll;
            let next = if delta > 0 { scroll.saturating_sub(1) } else { (*scroll + 1).min(max) };
            if next != *scroll {
                *scroll = next;
                self.ui.text.hover = None;
                self.repaint_expanded();
            }
            return;
        }
        if matches!(self.ui.tab, view::Tab::Note | view::Tab::Text) {
            let Some(layout) = self.note_layout() else { return };
            let inner = self.note_inner();
            let max = (edit::content_height(&layout) - (inner.3 - inner.1)).max(0.0);
            let step = 42.0 * self.scale * if delta > 0 { -1.0 } else { 1.0 };
            let next = (self.multi().scroll + step).clamp(0.0, max);
            if next != self.multi().scroll {
                self.multi_mut().scroll = next;
                self.repaint_expanded();
            }
            return;
        }
        let (len, scroll) = match self.ui.tab {
            view::Tab::Clipboard => (self.clipboard.clips.len(), &mut self.ui.clip.scroll),
            view::Tab::Agy => (self.ui.agy.chats.len(), &mut self.ui.agy.scroll),
            view::Tab::Privacy => (self.ui.privacy_log.rows.len(), &mut self.ui.privacy_log.scroll),
            _ => return,
        };
        let max = len.saturating_sub(rows);
        let next = if delta > 0 { scroll.saturating_sub(1) } else { (*scroll + 1).min(max) };
        if next != *scroll {
            *scroll = next;
            self.ui.clip.hover = None;
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn on_interactive_body(&self, x: i32, y: i32) -> bool {
        if self.can_release_character(x, y) {
            return true;
        }
        let (at, size, radius) = self.pill(self.win, false, false);
        if let Some(tween) = self.pill_motion.width {
            let rect = tween.at(Instant::now());
            let origin = self.window_origin(self.win);
            return view::rounded_body_contains(rect, radius.X, (x as f32 + origin.0, y as f32 + origin.1));
        }
        view::rounded_body_contains((at.X, at.Y, size.X, size.Y), radius.X, (x as f32, y as f32))
    }

    pub(in crate::island) fn set_decoration_through(&mut self, through: bool) {
        if self.pointer.decoration_through == through {
            return;
        }
        self.pointer.decoration_through = through;
        if through && self.pointer.tracking {
            self.on_mouse_leave();
        }
        let through = through || (self.ghosted && self.settings.ghost_click_through);
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let bits = (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as isize;
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, if through { ex | bits } else { ex & !bits });
            if through {
                let _ = SetLayeredWindowAttributes(self.hwnd, windows::Win32::Foundation::COLORREF(0), 255, LWA_ALPHA);
            }
        }
    }

    pub(in crate::island) fn update_decoration_input(&mut self) {
        let mut point = windows::Win32::Foundation::POINT::default();
        let mut window = windows::Win32::Foundation::RECT::default();
        let valid = unsafe { GetCursorPos(&mut point).is_ok() && GetWindowRect(self.hwnd, &mut window).is_ok() };
        let through = valid
            && self.state == State::Collapsed
            && (self.liquid_on() || self.pill_motion.width.is_some())
            && !self.character_interacting()
            && (!self.pill_motion_busy() || self.pill_motion.width.is_some())
            && !self.on_interactive_body(point.x - window.left, point.y - window.top);
        let was_through = self.pointer.decoration_through;
        self.set_decoration_through(through);
        if was_through
            && !through
            && valid
            && self.state == State::Collapsed
            && !self.character_interacting()
            && self.on_interactive_body(point.x - window.left, point.y - window.top)
        {
            self.on_mouse_move(point.x - window.left, point.y - window.top);
        }
    }

    pub(in crate::island) fn on_pill(&self, x: i32, y: i32) -> bool {
        let s = self.scale;
        let (left, top, w, h) = self.pill_rect(false);
        let tol = 3.0 * s;
        let (fx, fy) = (x as f32, y as f32);
        let above = if self.liquid_on() { tol } else { tol + 40.0 };
        fx >= left - tol && fx < left + w + tol && fy >= top - above && fy < top + h + tol
    }

    pub(in crate::island) fn on_mouse_move(&mut self, x: i32, y: i32) {
        if self.character_interacting() {
            self.kill_timer(Timer::Expand);
            return;
        }
        unsafe {
            if !self.pointer.tracking {
                self.pointer.tracking = true;
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: self.hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
            }
        }
        self.kill_timer(Timer::Collapse);
        self.kill_timer(Timer::Autohide);
        if self.state == State::Collapsing && self.transition_hit(x, y) == Some(true) {
            self.expand();
        }
        if self.state != State::Expanded {
            let hidden = matches!(self.state, State::Hidden | State::Hiding);
            let over_mic = self.state == State::Collapsed
                && view::hit_mic_dot(self.pill_rect(false), self.scale, &self.ui, x as f32, y as f32);
            let over_spark = self.can_release_character(x, y);
            self.ui.hand = self.state == State::Collapsed
                && self.on_pill(x, y)
                && (over_mic || over_spark || self.recording.is_some() || self.notice_link.is_some());
            let hover = over_mic.then_some(view::HeadHit::Mic);
            if self.state == State::Collapsed && hover != self.ui.hover_head {
                self.ui.hover_head = hover;
                self.repaint_collapsed();
            }
            let tip = if over_spark {
                let (w, h) = self.char_size();
                Some(("Click or drag to release the character".to_string(), (x as f32 + w / 2.0, y as f32 + h)))
            } else {
                hover.and_then(|_| {
                    let dot = view::indicators(self.pill_rect(false), self.scale, &self.ui).0?;
                    let text = if self.ui.mic_muted == Some(true) { "Unmute microphone" } else { "Mute microphone" };
                    Some((text.to_string(), ((dot.0 + dot.2) / 2.0, dot.3 + 10.0 * self.scale)))
                })
            };
            self.set_tip(tip);
            if !over_mic && !over_spark && (hidden || self.on_pill(x, y)) {
                self.set_timer(Timer::Expand, self.settings.open_delay_ms.max(1));
            } else {
                self.kill_timer(Timer::Expand);
            }
        }
    }

    pub(in crate::island) fn on_mouse_leave(&mut self) {
        self.ui.pressed_action = None;
        self.pointer.tracking = false;
        let had_hover = self.ui.hover_tab.take().is_some()
            | self.ui.clip.hover.take().is_some()
            | self.ui.hover_ctl.take().is_some()
            | self.ui.text.hover.take().is_some()
            | self.ui.notes.hover.take().is_some()
            | self.ui.home.hover_act.take().is_some()
            | self.ui.hover_head.take().is_some();
        self.ui.hand = false;
        self.set_tip(None);
        if had_hover && self.state == State::Collapsed {
            self.repaint_collapsed();
        }
        if had_hover {
            self.repaint_expanded();
        }
        self.kill_timer(Timer::Expand);
        if self.pointer.character_press.is_some() || self.pet.desktop.as_ref().is_some_and(|desktop| desktop.dragging())
        {
            return;
        }
        match self.state {
            State::Expanded
                if self.ui.remind.alert.is_some()
                    || self.ui.pinned
                    || self.popup_open
                    || self.pointer.moving.is_some() => {}
            State::Expanded if self.ui.editing => {
                self.set_timer(Timer::Collapse, self.settings.close_delay_ms.max(1500))
            }
            State::Expanded => self.set_timer(Timer::Collapse, self.settings.close_delay_ms.max(1)),
            State::Collapsed => self.schedule_autohide(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;

    #[test]
    fn decorative_pull_does_not_enlarge_the_clickable_body() {
        for scale in [0.75, 1.0, 1.5, 2.0] {
            let frame = (50.0 * scale, 40.0 * scale, 220.0 * scale, 36.0 * scale);
            for point in [(160.0, 50.0), (60.0, 58.0), (260.0, 58.0)] {
                assert!(view::rounded_body_contains(frame, 18.0 * scale, (point.0 * scale, point.1 * scale)));
            }
            for point in [(160.0, 85.0), (35.0, 58.0), (285.0, 58.0), (51.0, 41.0), (160.0, 30.0)] {
                assert!(!view::rounded_body_contains(frame, 18.0 * scale, (point.0 * scale, point.1 * scale)));
            }
        }
    }
}
