use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, MSG, WM_APP, WM_MOUSEMOVE, WM_PAINT, WM_TIMER};

const EVERY: Duration = Duration::from_secs(10);
const SHOWN: usize = 12;

struct Totals {
    since: Instant,
    rows: HashMap<String, (u32, u64)>,
    classes: HashMap<isize, String>,
}

thread_local! {
    static TOTALS: RefCell<Totals> =
        RefCell::new(Totals { since: Instant::now(), rows: HashMap::new(), classes: HashMap::new() });
}

pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("NI_PROFILE").is_some())
}

pub fn span_name(code: u32) -> &'static str {
    match code {
        1 => "span:pet.render_designed",
        2 => "span:pet.attached_paint",
        3 => "span:pet.behavior",
        4 => "span:pet.paint_dc",
        5 => "span:pet.update_layered",
        6 => "span:pet.draw_character",
        _ => "span:?",
    }
}

pub struct Span(u32, Option<Instant>);

pub fn span(code: u32) -> Span {
    Span(code, on().then(Instant::now))
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(started) = self.1 {
            record(span_name(self.0).to_string(), started.elapsed());
        }
    }
}

pub fn dispatch(msg: &MSG, spent: Duration) {
    let name = TOTALS.with(|totals| {
        let mut totals = totals.borrow_mut();
        let class = totals.classes.entry(msg.hwnd.0 as isize).or_insert_with(|| class_of(msg.hwnd)).clone();
        let what = match msg.message {
            WM_TIMER if class == "NightIsland.Window" || class.is_empty() => {
                format!("timer {}", super::timers::timer_name(msg.wParam.0))
            }
            WM_TIMER => format!("timer {}", msg.wParam.0),
            WM_PAINT => "WM_PAINT".into(),
            WM_MOUSEMOVE => "WM_MOUSEMOVE".into(),
            m if m >= WM_APP => format!("WM_APP+{}", m - WM_APP),
            m => format!("msg {m:#x}"),
        };
        format!("{class} {what}")
    });
    record(name, spent);
}

fn class_of(hwnd: HWND) -> String {
    let mut buffer = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..n])
}

pub fn record(name: String, spent: Duration) {
    TOTALS.with(|totals| {
        let mut totals = totals.borrow_mut();
        let row = totals.rows.entry(name).or_insert((0, 0));
        row.0 += 1;
        row.1 += spent.as_nanos() as u64;
        let secs = totals.since.elapsed().as_secs_f32();
        if secs < EVERY.as_secs_f32() {
            return;
        }
        let mut rows: Vec<(String, u32, u64)> = totals.rows.iter().map(|(k, (n, ns))| (k.clone(), *n, *ns)).collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.2));
        let text: Vec<String> = rows
            .iter()
            .take(SHOWN)
            .map(|(k, n, ns)| format!("{k} {:.1}/s {:.1}ms/s", *n as f32 / secs, *ns as f32 / 1e6 / secs))
            .collect();
        crate::log_info!("profile {secs:.0}s (wall time in handlers): {}", text.join(" | "));
        totals.rows.clear();
        totals.since = Instant::now();
    });
}
