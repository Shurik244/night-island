use std::cell::Cell;

use crate::island::*;

thread_local! {
    static MONITOR_HZ: Cell<(Option<Instant>, u32)> = const { Cell::new((None, 0)) };
    static FRAME_COST: Cell<f32> = const { Cell::new(0.0) };
}

#[link(name = "winmm")]
unsafe extern "system" {
    fn timeBeginPeriod(period: u32) -> u32;
}

const TRIM_EVERY: Duration = Duration::from_secs(180);

thread_local! {
    static LAST_TRIM: Cell<Option<Instant>> = const { Cell::new(None) };
}

impl App {
    fn trim_memory_when_idle(&mut self) {
        let quiet = matches!(self.state, State::Collapsed | State::Hidden)
            && self.recording.is_none()
            && !dictation::active()
            && self.agent.is_none();
        let due = LAST_TRIM.get().is_none_or(|at| at.elapsed() >= TRIM_EVERY);
        if !quiet || !due {
            return;
        }
        LAST_TRIM.set(Some(Instant::now()));
        let before = memory::usage();
        memory::trim();
        if let (Some(before), Some(after)) = (before, memory::usage())
            && memory::worth_logging(before, after)
        {
            crate::log_info!(
                "Memory trimmed: working set {} -> {} MB, private {} MB",
                before.working_mb(),
                after.working_mb(),
                after.private_mb()
            );
        }
    }
}

pub(in crate::island) fn raise_timer_resolution() {
    unsafe {
        timeBeginPeriod(1);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::island) enum Timer {
    Expand = 1,
    Collapse = 2,
    Shrink = 3,
    Autohide = 4,
    HideDone = 5,
    Clock = 6,
    Clip = 7,
    Disarm = 8,
    Paste = 9,
    Liquid = 10,
    Caret = 11,
    NoteSave = 12,
    Volume = 13,
    Notice = 14,
    Spectrum = 15,
    Remind = 16,
    Rec = 17,
    DlStale = 18,
    Char = 19,
    Progress = 20,
    Agy = 21,
    Geo = 22,
    Game = 23,
    Lyric = 24,
    LockKey = 25,
    SwCaret = 26,
    PillBack = 27,
    GhostHold = 28,
    Activity = 29,
    Settle = 30,
    UiAnim = 31,
    VpnWait = 32,
    SteamPoll = 33,
    Tip = 34,
    SwAnim = 35,
    Weather = 36,
    WidthMotion = 37,
    MusicIntro = 38,
    NotificationStage = 39,
    NotificationExpire = 40,
    NotificationHover = 41,
    TopPanel = 42,
    WaveSearch = 43,
    Taskbar = 44,
    LiveActivity = 45,
}

impl Timer {
    const ALL: [Timer; 45] = [
        Timer::Expand,
        Timer::Collapse,
        Timer::Shrink,
        Timer::Autohide,
        Timer::HideDone,
        Timer::Clock,
        Timer::Clip,
        Timer::Disarm,
        Timer::Paste,
        Timer::Liquid,
        Timer::Caret,
        Timer::NoteSave,
        Timer::Volume,
        Timer::Notice,
        Timer::Spectrum,
        Timer::Remind,
        Timer::Rec,
        Timer::DlStale,
        Timer::Char,
        Timer::Progress,
        Timer::Agy,
        Timer::Geo,
        Timer::Game,
        Timer::Lyric,
        Timer::LockKey,
        Timer::SwCaret,
        Timer::PillBack,
        Timer::GhostHold,
        Timer::Activity,
        Timer::Settle,
        Timer::UiAnim,
        Timer::VpnWait,
        Timer::SteamPoll,
        Timer::Tip,
        Timer::SwAnim,
        Timer::Weather,
        Timer::WidthMotion,
        Timer::MusicIntro,
        Timer::NotificationStage,
        Timer::NotificationExpire,
        Timer::NotificationHover,
        Timer::TopPanel,
        Timer::WaveSearch,
        Timer::Taskbar,
        Timer::LiveActivity,
    ];

    fn from_id(id: usize) -> Option<Timer> {
        Self::ALL.into_iter().find(|t| *t as usize == id)
    }
}

impl App {
    pub(in crate::island) fn kill_timer(&self, timer: Timer) {
        if matches!(timer, Timer::Expand) {
            self.pointer.expand_pending.set(false);
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), timer as usize);
        }
    }

    pub(in crate::island) fn set_timer(&self, timer: Timer, ms: u32) {
        if matches!(timer, Timer::Expand) && self.pointer.expand_pending.replace(true) {
            return;
        }
        unsafe {
            SetTimer(Some(self.hwnd), timer as usize, ms, None);
        }
    }

    pub(in crate::island) fn frame_ms(&self) -> u32 {
        use crate::data::settings::FrameRate;
        let monitor = (1000.0 / self.monitor_hz()).round().clamp(4.0, 33.0) as u32;
        match self.settings.frame_rate {
            FrameRate::Fps30 => 33,
            FrameRate::Fps60 => 16,
            FrameRate::Monitor => monitor,
            FrameRate::Adaptive => {
                let cost = FRAME_COST.get();
                if cost > monitor as f32 * 0.8 { ((cost * 1.25).ceil() as u32).min(33) } else { monitor }
            }
        }
    }

    fn monitor_hz(&self) -> f32 {
        use windows::Win32::Graphics::Gdi::{
            DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST,
            MONITORINFO, MONITORINFOEXW, MonitorFromWindow,
        };
        let (at, hz) = MONITOR_HZ.get();
        if let Some(at) = at
            && hz > 0
            && at.elapsed() < Duration::from_secs(3)
        {
            return hz as f32;
        }
        let mut found = 60;
        unsafe {
            let monitor = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, (&raw mut info).cast::<MONITORINFO>()).as_bool() {
                let mut mode = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
                if EnumDisplaySettingsW(windows::core::PCWSTR(info.szDevice.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode)
                    .as_bool()
                    && mode.dmDisplayFrequency > 1
                {
                    found = mode.dmDisplayFrequency;
                }
            }
        }
        MONITOR_HZ.set((Some(Instant::now()), found));
        found as f32
    }

    pub(in crate::island) fn on_timer(&mut self, id: usize) {
        let started = Instant::now();
        let frame = matches!(
            Timer::from_id(id),
            Some(
                Timer::Spectrum
                    | Timer::Liquid
                    | Timer::UiAnim
                    | Timer::Lyric
                    | Timer::Char
                    | Timer::Weather
                    | Timer::Progress
            )
        );
        self.on_timer_inner(id);
        if frame {
            FRAME_COST.set(FRAME_COST.get() * 0.8 + started.elapsed().as_secs_f32() * 1000.0 * 0.2);
        }
    }

    fn on_timer_inner(&mut self, id: usize) {
        let Some(timer) = Timer::from_id(id) else { return };
        self.kill_timer(timer);
        match timer {
            Timer::LiveActivity => self.live_activity_tick(),
            Timer::Taskbar => self.configure_taskbar_replacement(),
            Timer::TopPanel => self.update_top_panel(false),
            Timer::NotificationStage => self.notification_stage(),
            Timer::NotificationExpire => self.dismiss_notification_card(),
            Timer::WidthMotion => self.width_motion_done(),
            Timer::MusicIntro => self.music_intro_step(),
            Timer::Expand if self.character_interacting() => {}
            Timer::Expand if matches!(self.state, State::Hidden | State::Hiding) => self.reveal(),
            Timer::Expand => self.expand(),
            Timer::Collapse => {
                if self.interacting_with_top_panel() || self.ui.home.widgets.interacting() {
                    self.set_timer(Timer::Collapse, self.settings.close_delay_ms.max(80));
                } else {
                    self.collapse();
                }
            }
            Timer::Shrink => self.shrink_window(),
            Timer::Autohide => self.hide(),
            Timer::HideDone => self.hide_done(),
            Timer::Liquid => self.liquid_tick(),
            Timer::Caret => self.on_caret_timer(),
            Timer::Spectrum => self.spectrum_tick(),
            Timer::Remind => self.on_remind_timer(),
            Timer::Rec => self.on_rec_timer(),
            Timer::Char => self.char_tick(),
            Timer::Game => self.on_game_timer(),
            Timer::Lyric => self.lyric_tick(),
            Timer::LockKey => self.report_lockkeys(),
            Timer::SwCaret => self.sw_caret_tick(),
            Timer::SwAnim => self.sw_anim_tick(),
            Timer::Weather => self.weather_tick(),
            Timer::PillBack => self.pill_back_tick(),
            Timer::GhostHold => self.ghost_hold_tick(),
            Timer::Activity => self.on_activity_timer(),
            Timer::Settle => {
                if self.cover_flight_until.is_some_and(|until| Instant::now() >= until) {
                    self.finish_cover_motion();
                }
                self.settle_layers();
            }
            Timer::UiAnim => self.ui_anim_tick(),
            Timer::VpnWait => self.vpn_wait_over(),
            Timer::SteamPoll => self.on_steam_poll(),
            Timer::Tip => self.show_tip(),
            Timer::WaveSearch => self.submit_wave_search(),
            Timer::Geo => {
                if self.ui.tab == view::Tab::Weather && self.ui.editing {
                    weather::search(self.hwnd, &self.ui.city.input.text());
                }
            }
            Timer::Agy => {
                if self.state == State::Expanded
                    && self.ui.tab == view::Tab::Agy
                    && self.ui.agy.open
                    && self.ui.agy.sel.as_deref().is_some_and(agy::running)
                {
                    self.load_history(true);
                    self.repaint_expanded();
                    self.start_agy_timer();
                }
            }
            Timer::Progress => {
                if self.state == State::Expanded && matches!(self.ui.tab, view::Tab::Home | view::Tab::Yandex) {
                    self.repaint_expanded();
                    self.start_progress_timer();
                } else if self.progress_pill_visible() && view::progress_strip_on(&self.ui) {
                    let c = self.collapsed_surface_rect();
                    let _ = self.collapsed.paint(&self.gpu, |p| {
                        let _ = view::paint_collapsed(p, c, self.scale, &self.ui);
                    });
                    self.paint_progress_fill();
                    self.start_progress_timer();
                }
            }
            Timer::NotificationHover => self.notification_hover(),
            Timer::DlStale => {
                self.ui.downloading = None;
                self.update_eq();
                self.repaint_collapsed();
            }
            Timer::Notice => {
                self.ui.notice = None;
                self.notice_link = None;
                self.update_eq();
                self.maybe_show_notification();
                self.repaint_collapsed();
            }
            Timer::Volume => {
                self.ui.volume = None;
                self.update_eq();
                self.repaint_collapsed();
                self.maybe_show_notification();
            }
            Timer::NoteSave => self.save_note(),
            Timer::Clip => self.on_clipboard(),
            Timer::Paste => self.on_paste_timer(),
            Timer::Disarm => {
                self.ui.armed = None;
                self.ui.notes.armed_delete = false;
                self.ui.clip.clear_armed = false;
                self.ui.privacy_log.clear_armed = false;
                self.ui.activity.clear_armed = false;
                self.ui.steam.armed = None;
                self.repaint_expanded();
            }
            Timer::Clock => {
                self.refresh_reminder_labels();
                self.repaint();
                self.schedule_clock();
                self.trim_memory_when_idle();
            }
        }
    }
}
