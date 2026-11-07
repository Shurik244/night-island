use std::collections::HashSet;

use crate::platform::memory_tools::{Snapshot, megabytes};
use crate::ui::i18n::tr;
use crate::view::*;

#[derive(Default)]
pub struct MemoryUi {
    pub open: bool,
    pub busy: bool,
    pub failed: bool,
    pub snapshot: Option<Snapshot>,
    pub selected: HashSet<String>,
    pub page: usize,
}

impl MemoryUi {
    pub fn update(&mut self, mut snapshot: Snapshot, excluded: &[String]) {
        if let Some(previous) = &self.snapshot {
            snapshot
                .apps
                .sort_by_key(|app| previous.apps.iter().position(|old| old.path == app.path).unwrap_or(usize::MAX));
        }
        if self.snapshot.is_none() {
            self.selected = snapshot
                .apps
                .iter()
                .filter(|app| app.minimized && !excluded.contains(&app.path))
                .map(|app| app.path.clone())
                .collect();
        }
        self.selected.retain(|path| {
            !excluded.contains(path) && snapshot.apps.iter().any(|app| app.path == *path && app.minimized)
        });
        self.snapshot = Some(snapshot);
        self.busy = crate::platform::memory_tools::busy();
        self.failed = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemoryAction {
    Back,
    Refresh,
    Clean,
    Watch,
    Sort,
    Select(usize),
    Exclude(usize),
    Page(usize),
}

fn rows(rect: Rect, scale: f32) -> usize {
    (((rect.3 - rect.1) / scale - 300.0) / 54.0).floor().max(1.0) as usize
}

fn page(rect: Rect, scale: f32, ui: &UiState) -> usize {
    ui.sw
        .memory
        .page
        .min(ui.sw.memory.snapshot.as_ref().map_or(0, |s| s.apps.len().saturating_sub(1) / rows(rect, scale)))
}

fn controls(rect: Rect, s: f32, ui: &UiState) -> Vec<(MemoryAction, Rect)> {
    let (l, t, r, b) = rect;
    let row = |x1: f32, y1: f32, x2: f32, y2: f32| (l + x1 * s, t + y1 * s, l + x2 * s, t + y2 * s);
    let width = (r - l) / s;
    let mut out = vec![
        (MemoryAction::Sort, row(0.0, 223.0, 178.0, 245.0)),
        (MemoryAction::Back, row(width - 82.0, 0.0, width, 34.0)),
        (MemoryAction::Watch, row(0.0, 190.0, width, 222.0)),
    ];
    let current = page(rect, s, ui);
    let pages = ui.sw.memory.snapshot.as_ref().map_or(1, |snapshot| snapshot.apps.len().div_ceil(rows(rect, s)).max(1));
    if current > 0 {
        out.push((MemoryAction::Page(current - 1), (r - 160.0 * s, b - 36.0 * s, r - 110.0 * s, b)));
    }
    if current + 1 < pages {
        out.push((MemoryAction::Page(current + 1), (r - 50.0 * s, b - 36.0 * s, r, b)));
    }
    if !ui.sw.memory.busy {
        out.push((MemoryAction::Refresh, row(0.0, 144.0, 110.0, 180.0)));
        if !ui.sw.memory.selected.is_empty() {
            out.push((MemoryAction::Clean, row(122.0, 144.0, width, 180.0)));
        }
        if let Some(snapshot) = &ui.sw.memory.snapshot {
            let count = rows(rect, s);
            for (index, app) in snapshot.apps.iter().enumerate().skip(page(rect, s, ui) * count).take(count) {
                let y = 247.0 + (index % count) as f32 * 54.0;
                if app.minimized && !ui.cfg.memory_excluded.contains(&app.path) {
                    out.push((MemoryAction::Select(index), row(0.0, y, width - 96.0, y + 48.0)));
                }
                out.push((MemoryAction::Exclude(index), row(width - 92.0, y + 8.0, width, y + 40.0)));
            }
        }
    }
    out
}

pub fn hit_memory(rect: Rect, scale: f32, ui: &UiState, x: f32, y: f32) -> Option<MemoryAction> {
    controls(rect, scale, ui).into_iter().find(|(_, r)| inside(*r, x, y)).map(|(action, _)| action)
}

pub fn paint_memory(p: &Painter, rect: Rect, s: f32, ui: &UiState) -> Result<()> {
    let (l, t, r, b) = rect;
    let dim = rgba(1.0, 1.0, 1.0, 0.64);
    p.text(
        "Memory manager",
        (l, t, r - 90.0 * s, t + 36.0 * s),
        25.0 * s,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    p.text_wrapped(
        "Minimize apps you are not using, then choose which ones to clean. Apps stay open.",
        (l, t + 42.0 * s, r, t + 83.0 * s),
        12.5 * s,
        DWRITE_FONT_WEIGHT_NORMAL,
        dim,
    )?;
    let panel = (l, t + 88.0 * s, r, t + 133.0 * s);
    p.fill_round_rect(panel, 12.0 * s, acc(0.13))?;
    let status = if ui.sw.memory.failed || ui.sw.memory.snapshot.as_ref().is_some_and(|snapshot| snapshot.failed) {
        tr("Could not read memory usage. Try refreshing.").into_owned()
    } else if let Some(snapshot) = &ui.sw.memory.snapshot {
        if let (Some(before), Some(after)) = (snapshot.available_before, snapshot.available_after) {
            format!(
                "{}: {} → {} MB · {}: {} MB",
                tr("Free at cleanup"),
                megabytes(before),
                megabytes(after),
                tr("Now"),
                megabytes(snapshot.available)
            )
        } else {
            format!("{}: {} MB / {} MB", tr("Free memory"), megabytes(snapshot.available), megabytes(snapshot.total))
        }
    } else {
        tr("Reading running applications…").into_owned()
    };
    p.text(
        &status,
        (l + 12.0 * s, panel.1, r - 12.0 * s, panel.3),
        12.5 * s,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    p.text(
        "",
        (l, t + 223.0 * s, r, t + 245.0 * s),
        11.0 * s,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        dim,
    )?;
    let now_x = l + (r - l - 96.0 * s) * 0.53;
    p.text(
        "RAM / after / growth",
        (now_x, t + 223.0 * s, r - 96.0 * s, t + 245.0 * s),
        10.5 * s,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_TRAILING,
        dim,
    )?;
    if let Some(snapshot) = &ui.sw.memory.snapshot {
        let count = rows(rect, s);
        for (index, app) in snapshot.apps.iter().enumerate().skip(page(rect, s, ui) * count).take(count) {
            let y = t + (247.0 + (index % count) as f32 * 54.0) * s;
            let selected = ui.sw.memory.selected.contains(&app.path);
            p.fill_round_rect(
                (l, y, r, y + 48.0 * s),
                10.0 * s,
                if selected { acc(0.12) } else { rgba(1.0, 1.0, 1.0, 0.045) },
            )?;
            p.glyph(
                if selected { '\u{E73D}' } else { '\u{E739}' },
                (l + 3.0 * s, y, l + 29.0 * s, y + 48.0 * s),
                15.0 * s,
                if app.minimized { WHITE } else { dim },
            )?;
            p.text(
                &app.name,
                (l + 34.0 * s, y + 3.0 * s, now_x - 4.0 * s, y + 25.0 * s),
                13.0 * s,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                WHITE,
            )?;
            p.text(
                if app.minimized { "Minimized" } else { "In use · skipped" },
                (l + 34.0 * s, y + 25.0 * s, now_x - 4.0 * s, y + 44.0 * s),
                10.5 * s,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                dim,
            )?;
            let numbers = if let Some(after) = app.after {
                format!("{} / {} / +{} MB", megabytes(app.working), megabytes(after), megabytes(app.growth))
            } else {
                format!("{} MB", megabytes(app.working))
            };
            p.text(
                &numbers,
                (now_x, y, r - 98.0 * s, y + 48.0 * s),
                11.5 * s,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                if app.growth >= 32 * 1024 * 1024 { rgba(1.0, 0.72, 0.43, 1.0) } else { dim },
            )?;
        }
        if snapshot.apps.is_empty() {
            p.text_wrapped(
                "No accessible desktop apps found.",
                (l, t + 260.0 * s, r, t + 320.0 * s),
                13.0 * s,
                DWRITE_FONT_WEIGHT_NORMAL,
                dim,
            )?;
        }
    }
    let current = page(rect, s, ui);
    let pages = ui.sw.memory.snapshot.as_ref().map_or(1, |snapshot| snapshot.apps.len().div_ceil(rows(rect, s)).max(1));
    p.text(
        &format!("{} / {pages}", current + 1),
        (r - 108.0 * s, b - 36.0 * s, r - 52.0 * s, b),
        11.0 * s,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_CENTER,
        dim,
    )?;
    if ui.sw.memory.busy || ui.sw.memory.selected.is_empty() {
        guide_button(
            p,
            (l + 122.0 * s, t + 144.0 * s, r, t + 180.0 * s),
            s,
            if ui.sw.memory.busy { "Working…" } else { "Select minimized apps" },
            false,
            false,
        )?;
    }
    for (action, button) in controls(rect, s, ui) {
        let hover = ui.sw.hover == Some(SwHit::Memory(action));
        let label = match action {
            MemoryAction::Back => tr("Back").into_owned(),
            MemoryAction::Sort => tr("Sort by growth").into_owned(),
            MemoryAction::Refresh => tr("Refresh").into_owned(),
            MemoryAction::Clean => format!("{} ({})", tr("Free selected memory"), ui.sw.memory.selected.len()),
            MemoryAction::Watch => {
                let glyph = if ui.cfg.memory_watch { "●" } else { "○" };
                format!("{glyph} {}", tr("Watch regrowth for 2 minutes"))
            }
            MemoryAction::Exclude(index) => {
                tr(if ui.cfg.memory_excluded.contains(&ui.sw.memory.snapshot.as_ref().unwrap().apps[index].path) {
                    "Excluded"
                } else {
                    "Exclude"
                })
                .into_owned()
            }
            MemoryAction::Page(target) => if target < page(rect, s, ui) { "‹" } else { "›" }.into(),
            MemoryAction::Select(_) => continue,
        };
        guide_button(p, button, s, &label, action == MemoryAction::Clean, hover)?;
    }
    let note = if ui.sw.memory.busy {
        tr("Working…").into_owned()
    } else if let Some(snapshot) = ui.sw.memory.snapshot.as_ref().filter(|s| s.available_after.is_some()) {
        format!(
            "{}: {} · {}: {} · {}",
            tr("Processes cleaned"),
            snapshot.trimmed,
            tr("Skipped"),
            snapshot.skipped,
            tr(if snapshot.watching { "Watching regrowth" } else { "Observation finished" })
        )
    } else {
        tr("Windows may load these pages again when apps need them.").into_owned()
    };
    p.text_wrapped(&note, (l, b - 39.0 * s, r - 170.0 * s, b + 2.0 * s), 10.5 * s, DWRITE_FONT_WEIGHT_NORMAL, dim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::memory_tools::Application;

    #[test]
    fn refresh_preserves_row_positions_and_removes_unsafe_selections() {
        let app = |path: &str, minimized| Application { path: path.into(), minimized, ..Default::default() };
        let mut ui = MemoryUi::default();
        ui.update(Snapshot { apps: vec![app("a", true), app("b", true)], ..Default::default() }, &["b".into()]);
        assert!(ui.selected.contains("a"));
        assert!(!ui.selected.contains("b"));
        ui.update(Snapshot { apps: vec![app("b", true), app("a", false)], ..Default::default() }, &[]);
        assert!(ui.selected.is_empty());
        assert_eq!(ui.snapshot.as_ref().unwrap().apps[0].path, "a");
    }

    #[test]
    fn pagination_uses_visible_pages_and_fits_the_panel() {
        let mut ui = UiState::new();
        ui.sw.memory.snapshot = Some(Snapshot {
            apps: (0..9).map(|i| Application { path: i.to_string(), ..Default::default() }).collect(),
            ..Default::default()
        });
        for s in [0.8, 1.0, 2.0] {
            let rect = (250.0 * s, 100.0 * s, 910.0 * s, 668.0 * s);
            ui.sw.memory.page = usize::MAX;
            let current = page(rect, s, &ui);
            let items = controls(rect, s, &ui);
            assert!(!items.iter().any(|(a, _)| matches!(a, MemoryAction::Page(target) if *target > current)));
            assert!(items.iter().any(|(a, _)| *a == MemoryAction::Page(current - 1)));
            for (_, r) in items {
                assert!(r.0 >= rect.0 && r.1 >= rect.1 && r.2 <= rect.2 && r.3 <= rect.3);
            }
        }
    }
}
