use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::data::groups::{self, Source};
use crate::tools::appgroups;

use crate::island::*;

const SAMPLE_MS: u32 = 5000;
const AWAY_SECS: u64 = 120;
const MAX_STEP_SECS: i64 = 15;
const FLUSH_SECS: i64 = 60;
const SORT_MIN_SECS: i64 = 120;
const SORT_BATCH: usize = 20;
const SORT_PAUSE: Duration = Duration::from_secs(3600);

#[derive(Default)]
struct Pending {
    secs: HashMap<String, i64>,
    day: i64,
    last_look: Option<Instant>,
    last_flush: Option<Instant>,
}

thread_local! {
    static PENDING: RefCell<Pending> = RefCell::new(Pending::default());
    static SORTING: RefCell<(Vec<String>, Option<Instant>)> = const { RefCell::new((Vec::new(), None)) };
}

impl App {
    pub(in crate::island) fn start_activity(&mut self) {
        if self.settings.activity_track {
            PENDING.with(|p| p.borrow_mut().last_look = Some(Instant::now()));
            self.set_timer(Timer::Activity, SAMPLE_MS);
        } else {
            self.kill_timer(Timer::Activity);
            self.flush_activity();
        }
    }

    pub(in crate::island) fn on_activity_timer(&mut self) {
        if !self.settings.activity_track {
            return;
        }
        let today = activity::today();
        let app = (activity::idle_secs() < AWAY_SECS).then(activity::foreground_app).flatten();
        let due = PENDING.with(|p| {
            let mut p = p.borrow_mut();
            let now = Instant::now();
            let step = p.last_look.map_or(0, |t| now.duration_since(t).as_secs() as i64).min(MAX_STEP_SECS);
            p.last_look = Some(now);
            if p.day != today {
                p.day = today;
            }
            if let Some(app) = app.filter(|a| !a.eq_ignore_ascii_case("night-island")) {
                *p.secs.entry(app).or_insert(0) += step;
            }
            let last = *p.last_flush.get_or_insert(now);
            now.duration_since(last).as_secs() as i64 >= FLUSH_SECS
        });
        if due {
            self.flush_activity();
        }
        self.set_timer(Timer::Activity, SAMPLE_MS);
    }

    pub(in crate::island) fn flush_activity(&mut self) {
        let (day, secs) = PENDING.with(|p| {
            let mut p = p.borrow_mut();
            p.last_flush = Some(Instant::now());
            (p.day, std::mem::take(&mut p.secs))
        });
        if let Some(store) = &self.store {
            for (app, s) in secs {
                store.activity_add(day, &app, s);
            }
        }
        self.sort_unknown();
        if self.state == State::Expanded && self.ui.tab == view::Tab::Home {
            self.refresh_screen_time();
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn refresh_screen_time(&mut self) {
        self.ui.home.screen_secs = if self.settings.activity_track {
            let today = activity::today();
            self.store.as_ref().map(|s| s.activity_since(today, 1000).iter().map(|(_, secs)| *secs).sum())
        } else {
            None
        };
    }

    pub(in crate::island) fn reload_groups(&mut self) {
        let Some(store) = &self.store else { return };
        self.ui.activity.groups = store.act_groups();
        self.ui.activity.ai_used = store.ai_sort_used(activity::today());
        self.ui.cfg = self.settings.clone();
        self.render_settings_win();
    }

    fn sort_unknown(&mut self) {
        if !self.settings.activity_ai || !self.settings.activity_track {
            return;
        }
        let Some(store) = &self.store else { return };
        if !ai::has_key(self.settings.ai.provider) || SORTING.with(|s| !s.borrow().0.is_empty()) {
            return;
        }
        if SORTING.with(|s| s.borrow().1.is_some_and(|t| Instant::now() < t)) {
            return;
        }
        let today = activity::today();
        if store.ai_sort_used(today) >= ai::SORT_REQUESTS_PER_DAY {
            return;
        }
        let groups = store.act_groups();
        let assigned = store.act_assignments();
        let mut names: Vec<String> = Vec::new();
        for (app, secs) in store.activity_since(today - 6, 300) {
            if secs < SORT_MIN_SECS || appgroups::builtin_group(&app).is_some() {
                continue;
            }
            if let Some(name) = appgroups::clean_app(&app)
                && !assigned.contains_key(&name)
                && !names.contains(&name)
            {
                names.push(name);
            }
            if names.len() >= SORT_BATCH {
                break;
            }
        }
        if names.is_empty() {
            return;
        }
        let group_names: Vec<String> = groups.iter().map(|g| g.name.clone()).collect();
        if ai::ask_sort(self.hwnd, &names, &group_names) {
            store.ai_sort_count(today);
            SORTING.with(|s| s.borrow_mut().0 = names);
            self.reload_groups();
        }
    }

    pub(in crate::island) fn on_sort(&mut self) {
        let Some(answer) = ai::take_sort_result() else { return };
        let asked = SORTING.with(|s| std::mem::take(&mut s.borrow_mut().0));
        let (Some(store), Ok(reply)) = (&self.store, &answer) else {
            SORTING.with(|s| s.borrow_mut().1 = Some(Instant::now() + SORT_PAUSE));
            if let Err(why) = answer {
                let short: String = why.chars().take(70).collect();
                self.notify_for(Notice::Error, '\u{E783}', &format!("Sorting programs: {short}"), 4500);
            }
            return;
        };
        let groups = store.act_groups();
        let names: Vec<String> = groups.iter().map(|g| g.name.clone()).collect();
        let found = appgroups::parse_assignments(reply, &asked, &names);
        if found.is_empty() {
            SORTING.with(|s| s.borrow_mut().1 = Some(Instant::now() + SORT_PAUSE));
            return;
        }
        for (app, group) in &found {
            if let Some(g) = groups.iter().find(|g| g.name == *group) {
                store.act_app_set(app, g.id, Source::Ai);
            }
        }
        for app in asked.iter().filter(|a| !found.iter().any(|(f, _)| f == *a)) {
            store.act_app_set(app, appgroups::OTHER_ID, Source::Ai);
        }
        self.notify_for(Notice::Ai, '\u{E99A}', &format!("AI sorted {} programs into groups", found.len()), 4000);
        self.reload_activity();
        self.repaint_expanded();
    }

    pub(in crate::island) fn reload_activity(&mut self) {
        self.flush_activity();
        let today = activity::today();
        let first = today - if self.ui.activity.week { 6 } else { 0 };
        let Some(store) = &self.store else { return };
        let groups = store.act_groups();
        let assigned = store.act_assignments();
        let rows = store.activity_since(first, 400);
        self.ui.activity.totals = groups::totals(&rows, &assigned, &groups);
        self.ui.activity.days = if self.ui.activity.week {
            groups::stack_days(&store.activity_days(today - 6), today, &assigned, &groups)
        } else {
            Vec::new()
        };
        self.ui.activity.ai_used = store.ai_sort_used(today);
        self.ui.activity.groups = groups;
        if self.ui.activity.open_total().is_none() {
            self.ui.activity.open = None;
            self.ui.activity.picking = None;
        }
        let most = self.ui.activity.open_total().map_or(0, |t| t.apps.len());
        self.ui.activity.scroll = self.ui.activity.scroll.min(most.saturating_sub(1));
    }

    pub(in crate::island) fn restart_activity_anim(&mut self) {
        self.ui.activity.restart_draw();
        self.set_timer(Timer::UiAnim, self.frame_ms());
    }

    pub(in crate::island) fn ui_anim_tick(&mut self) {
        if self.cover_flight_until.is_some_and(|until| Instant::now() >= until) {
            self.finish_cover_motion();
        }
        if !self.ui.home.track_transition.active() {
            self.ui.home.track_transition.finish();
        }
        if self.state != State::Expanded {
            self.ui.tab_change = None;
            return;
        }
        let live_moving = self.ui.live_changed.is_some_and(|at| at.elapsed().as_millis() < 280);
        let mut again = self.ui.home.widgets.editor_active()
            || self.ui.home.widgets.motion.borrow().active()
            || live_moving
            || matches!(self.ui.tab, view::Tab::Home | view::Tab::Yandex) && self.ui.yandex.animate_colours();
        if let Some((_, start)) = self.ui.home.mixer_motion {
            if start.elapsed().as_millis() >= 220 {
                self.ui.home.mixer_motion = None;
            } else {
                again = true;
            }
        }
        if let Some(change) = self.ui.tab_change {
            if change.start.elapsed().as_millis() as u32 >= change.ms {
                self.ui.tab_change = None;
            } else {
                again = true;
            }
        }
        if self.ui.home.reorder.is_some() {
            again |= self.step_reorder();
        }
        if self.ui.tab_drag.is_some() {
            again |= self.step_tab_drag();
        }
        if self.ui.notes.reorder.is_some() {
            again |= self.step_tree_reorder();
        }
        if self.ui.home.tile_reorder.is_some() {
            again |= self.step_tile_reorder();
        }
        if self.ui.tab == view::Tab::Activity {
            let moving = self.ui.activity.step_glow(self.ui.home.hover_act);
            again |= moving || self.ui.activity.animating();
        }
        self.repaint_expanded();
        let preview = if !self.gaming && !self.pill_hidden && self.ui.tab == view::Tab::Chat {
            self.ui.chat.pet.preview_interval()
        } else {
            None
        };
        if again {
            self.set_timer(Timer::UiAnim, self.frame_ms());
        } else if let Some(interval) = preview {
            self.set_timer(Timer::UiAnim, interval);
        }
    }

    pub(in crate::island) fn start_tab_change(&mut self, from: view::Tab, to: view::Tab) {
        use crate::data::settings::TabAnim;
        if from == to || self.state != State::Expanded || self.settings.tab_anim == TabAnim::None {
            self.ui.tab_change = None;
            return;
        }
        let index = |t: view::Tab| view::TABS.iter().position(|e| e.0 == t).unwrap_or(0) as i32;
        let dir = if index(to) >= index(from) { 1.0 } else { -1.0 };
        let ms = (240.0 / self.settings.tab_anim_speed.max(0.1)) as u32;
        self.ui.tab_change = Some(view::TabChange { from, start: std::time::Instant::now(), ms, dir });
        self.set_timer(Timer::UiAnim, self.frame_ms());
    }

    fn pick_app(&mut self, shown: usize) {
        let Some(group) = self.ui.activity.open_total() else { return };
        let Some((app, _)) = group.apps.get(self.ui.activity.scroll + shown).cloned() else { return };
        self.ui.activity.picking = Some(app);
        self.ui.activity.picked_at = Some(std::time::Instant::now());
        self.ui.activity.glow.clear();
        self.set_timer(Timer::UiAnim, self.frame_ms());
    }

    fn move_app(&mut self, choice: usize) {
        let Some(app) = self.ui.activity.picking.take() else { return };
        let Some(group) = self.ui.activity.groups.get(choice).cloned() else { return };
        if let Some(store) = &self.store {
            store.act_app_set(&app, group.id, Source::User);
        }
        self.reload_activity();
        self.ui.activity.glow.clear();
        self.ui.activity.opened_at = Some(std::time::Instant::now());
        self.set_timer(Timer::UiAnim, self.frame_ms());
        let name = crate::ui::i18n::tr(&group.name).into_owned();
        self.notify_for(Notice::Integrations, '\u{E73E}', &format!("{app} \u{2192} {name}"), 2500);
    }

    pub(in crate::island) fn click_activity(&mut self, x: i32, y: i32) {
        let pill = self.pill_rect(true);
        match view::hit_activity(pill, self.scale, x as f32, y as f32, self.settings.activity_track, &self.ui.activity)
        {
            Some(view::ActivityHit::TurnOn) => {
                self.settings.activity_track = true;
                self.settings.save();
                self.ui.cfg = self.settings.clone();
                self.start_activity();
            }
            Some(view::ActivityHit::Today) => {
                self.ui.activity.week = false;
                self.ui.activity.picking = None;
                self.reload_activity();
                self.restart_activity_anim();
            }
            Some(view::ActivityHit::Week) => {
                self.ui.activity.week = true;
                self.ui.activity.picking = None;
                self.reload_activity();
                self.restart_activity_anim();
            }
            Some(view::ActivityHit::Group(i)) => {
                self.ui.activity.open = self.ui.activity.totals.get(i).map(|t| t.id);
                self.ui.activity.scroll = 0;
                self.ui.activity.glow.clear();
                self.ui.activity.opened_at = Some(std::time::Instant::now());
                self.set_timer(Timer::UiAnim, self.frame_ms());
            }
            Some(view::ActivityHit::Back) => {
                if self.ui.activity.picking.take().is_none() {
                    self.ui.activity.open = None;
                    self.ui.activity.scroll = 0;
                    self.ui.activity.opened_at = Some(std::time::Instant::now());
                    self.ui.activity.drawn_at = Some(std::time::Instant::now());
                }
                self.ui.activity.glow.clear();
                self.set_timer(Timer::UiAnim, self.frame_ms());
            }
            Some(view::ActivityHit::App(i)) => self.pick_app(i),
            Some(view::ActivityHit::Pick(i)) => self.move_app(i),
            Some(view::ActivityHit::Clear) => {
                if self.ui.activity.clear_armed {
                    self.ui.activity.clear_armed = false;
                    self.kill_timer(Timer::Disarm);
                    PENDING.with(|p| p.borrow_mut().secs.clear());
                    if let Some(store) = &self.store {
                        store.activity_clear();
                    }
                    self.reload_activity();
                } else {
                    self.ui.activity.clear_armed = true;
                    self.set_timer(Timer::Disarm, 3000);
                }
            }
            None => {
                if self.ui.activity.clear_armed {
                    self.ui.activity.clear_armed = false;
                }
                self.ui.activity.picking = None;
            }
        }
        self.repaint_expanded();
    }
}
