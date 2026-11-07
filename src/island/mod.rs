use std::cell::RefCell;
use std::mem::size_of;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use windows::UI::Color;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{
    AnimationIterationBehavior, CompositionAnimation, CompositionColorBrush, CompositionGeometricClip,
    CompositionPathGeometry, CompositionRoundedRectangleGeometry, Compositor, ContainerVisual, SpriteVisual,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
    RO_INIT_SINGLETHREADED, RoInitialize,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, MOD_NOREPEAT, RegisterHotKey, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    UnregisterHotKey, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_MENU, VK_RETURN,
    VK_RIGHT, VK_SHIFT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, Result, h, w};

use crate::data::clips::{self, Clip, Reminder, Store};
use crate::data::settings::{Edge, Hang, Notice, Placement, Settings, Style};
use crate::data::usage::UseKind;
use crate::platform::activity;
use crate::platform::apps;
use crate::platform::audio;
use crate::platform::capture;
use crate::platform::dictation;
use crate::platform::dnd;
use crate::platform::downloads;
use crate::platform::lockkeys;
use crate::platform::media;
use crate::platform::memory;
use crate::platform::metrics;
use crate::platform::privacy;
use crate::platform::recognize;
use crate::platform::recorder;
use crate::platform::steam;
use crate::platform::steam_art;
use crate::platform::system::{self, RadioSwitch};
use crate::platform::volume;
use crate::platform::vpn;
use crate::platform::winevent;
use crate::services::agents;
use crate::services::agy;
use crate::services::ai;
use crate::services::discord;
use crate::services::everything;
use crate::services::lyrics;
use crate::services::spotify;
use crate::services::update;
use crate::services::weather;
use crate::tools::cmd;
use crate::tools::edit;
use crate::tools::remind;
use crate::tools::texttools;
use crate::tools::voicecmd;
use crate::ui::characters::{self, Character, Reaction};
use crate::ui::gfx::{Gpu, Layer};
use crate::ui::overlay;
use crate::ui::tray;
use crate::view;
use windows_numerics::{Vector2, Vector3};

pub(crate) mod appearance;
mod character;
mod companion_chat;
mod cover_motion;
mod demo;
mod desktop_dock;
mod editing;
mod geometry;
mod hotkeys;
mod input;
mod liquid;
mod lyric;
mod memory_tools;
mod motion;
mod pet_studio;
mod pill;
mod pill_motion;
mod settings_win;
mod startup;
mod system_notifications;
mod tabs;
mod timers;
mod top_panel;
mod voice;
mod voice_info;
mod window;

use self::{
    character::*, editing::*, geometry::*, hotkeys::*, input::Pointer, liquid::*, motion::*, tabs::ClipboardState,
    timers::Timer, window::*,
};
pub use startup::run;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, PartialEq)]
enum State {
    Collapsed,
    Expanded,
    Collapsing,
    Hiding,
    Hidden,
}

#[derive(Clone, Copy, PartialEq)]
enum Win {
    Strip,
    Small,
    Big,
}

struct App {
    hwnd: HWND,
    scale: f32,
    compositor: Compositor,
    geometry: CompositionRoundedRectangleGeometry,
    open_motion: Option<motion::OpenMotion>,
    state: State,
    win: Win,
    pointer: Pointer,
    settings: Settings,
    dpi_scale: f32,
    _target: DesktopWindowTarget,
    _visual: ContainerVisual,
    backdrop: SpriteVisual,
    tint: SpriteVisual,
    settings_win: Option<HWND>,
    windows_taskbar: Option<crate::platform::taskbar::TaskbarGuard>,
    desktop_dock: Option<crate::ui::desktop_dock::Dock>,
    top_panel: Option<crate::ui::top_panel::TopPanel>,
    top_panel_poll: Instant,
    top_panel_radios: Instant,
    top_panel_data: crate::ui::top_panel::Snapshot,
    power_last: Option<(u8, bool)>,
    lock_pending: Vec<u16>,
    ui: view::UiState,
    store: Option<Store>,
    clipboard: ClipboardState,
    popup_open: bool,
    overlay: Option<overlay::Overlay>,
    want_dock_dialog: bool,
    want_chat_dialog: bool,
    want_header_menu: bool,
    pending_power: Option<(voicecmd::Power, std::time::Instant)>,
    pending_ask: Option<(voicecmd::Command, std::time::Instant)>,
    wake_session: bool,
    agent: Option<crate::services::assistant::agent::Run>,
    dialog: Vec<crate::services::assistant::agent::Exchange>,
    run_serial: u64,
    last_spoken: String,
    reminders: Vec<Reminder>,
    recording: Option<recorder::Recording>,
    notice_link: Option<std::path::PathBuf>,
    pet: Pet,
    prev_foreground: HWND,
    cw: f32,
    pill_motion: pill_motion::PillMotion,
    system_notices: system_notifications::SystemNotices,
    pending_high_surrogate: Option<u16>,
    music: MusicBars,
    gaming: bool,
    pill_hidden: bool,
    ghost_stage: u8,
    metrics_on: Option<u32>,
    steam_game: Option<(u32, std::time::Instant)>,
    ghosted: bool,
    root: ContainerVisual,
    gpu: Gpu,
    liquid: Liquid,
    ears: [Layer; 2],
    collapsed: Layer,
    progress_fill: Layer,
    expanded: Layer,
    cover_layer: Layer,
    cover_flight_until: Option<Instant>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app_value<T>(f: impl FnOnce(&App) -> T) -> Option<T> {
    APP.with(|cell| cell.try_borrow().ok().and_then(|guard| guard.as_ref().map(f)))
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(app) = guard.as_mut()
        {
            f(app);
        }
    });
}
