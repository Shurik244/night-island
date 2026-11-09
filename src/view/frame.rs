use crate::view::*;

pub fn tab_in_bar(cfg: &Settings, tab: Tab, name: &str) -> bool {
    match tab {
        Tab::Home | Tab::Settings => true,
        Tab::Agy | Tab::Chat => false,
        Tab::Steam => crate::platform::steam::client_installed() && !cfg.tab_hidden(name),
        _ => !cfg.tab_hidden(name),
    }
}

pub fn tab_order(cfg: &Settings) -> Vec<usize> {
    let movable: Vec<usize> = (0..TABS.len()).filter(|&i| !matches!(TABS[i].0, Tab::Home | Tab::Settings)).collect();
    let mut order: Vec<usize> =
        cfg.tab_order.iter().filter_map(|n| movable.iter().copied().find(|&i| TABS[i].2 == n)).collect();
    order.dedup();
    for i in movable {
        if !order.contains(&i) {
            order.push(i);
        }
    }
    let mut all = vec![TABS.iter().position(|t| t.0 == Tab::Home).unwrap_or(0)];
    all.extend(order);
    all.push(TABS.iter().position(|t| t.0 == Tab::Settings).unwrap_or(TABS.len() - 1));
    all
}

pub fn movable_tabs(ui: &UiState) -> Vec<usize> {
    ui.cfg
        .header_tabs
        .iter()
        .filter_map(|name| TABS.iter().position(|tab| tab.2 == name))
        .filter(|&i| tab_in_bar(&ui.cfg, TABS[i].0, TABS[i].2))
        .collect()
}

pub fn header_has_labels(ui: &UiState) -> bool {
    movable_tabs(ui).len() <= 5
}

fn header_icon_color(tab: Tab, ui: &UiState, alpha: f32) -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    let rgb = if ui.cfg.header_icon_colors {
        match tab {
            Tab::Record => (1.0, 0.32, 0.36),
            Tab::Weather => (1.0, 0.78, 0.32),
            Tab::Clipboard => (0.38, 0.72, 1.0),
            Tab::Note => (1.0, 0.8, 0.42),
            Tab::Ai | Tab::Chat => (0.7, 0.58, 1.0),
            Tab::Privacy => (0.36, 0.85, 0.63),
            Tab::Notifications => (1.0, 0.62, 0.3),
            Tab::Mixer | Tab::Controls => (0.4, 0.8, 0.86),
            Tab::Yandex => (1.0, 0.93, 0.0),
            _ => (1.0, 1.0, 1.0),
        }
    } else {
        (1.0, 1.0, 1.0)
    };
    rgba(rgb.0, rgb.1, rgb.2, alpha)
}

fn header_label_widths() -> [f32; TABS.len()] {
    use windows::Win32::Graphics::DirectWrite::*;
    type LabelWidths = (bool, crate::data::settings::InterfaceFont, u64, [f32; TABS.len()]);
    thread_local! {
        static WIDTHS: std::cell::RefCell<Option<LabelWidths>> = const { std::cell::RefCell::new(None) };
    }
    let russian = crate::ui::i18n::is_ru();
    WIDTHS.with_borrow_mut(|cached| {
        if let Some((language, font, revision, widths)) = cached.as_ref()
            && *language == russian
            && *font == crate::ui::fonts::selected()
            && *revision == crate::ui::fonts::revision()
        {
            return *widths;
        }
        let measured = (|| -> windows::core::Result<[f32; TABS.len()]> {
            unsafe {
                let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
                let format = crate::ui::fonts::format(&factory, 13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                let mut widths = [0.0; TABS.len()];
                for (index, (_, _, name)) in TABS.iter().enumerate() {
                    let text: Vec<u16> = crate::ui::i18n::tr(name).encode_utf16().collect();
                    let layout = factory.CreateTextLayout(&text, &format, 10000.0, 40.0)?;
                    let mut metrics = DWRITE_TEXT_METRICS::default();
                    layout.GetMetrics(&mut metrics)?;
                    widths[index] = metrics.widthIncludingTrailingWhitespace;
                }
                Ok(widths)
            }
        })();
        let widths = measured
            .unwrap_or_else(|_| TABS.map(|(_, _, name)| crate::ui::i18n::tr(name).chars().count() as f32 * 8.0));
        *cached = Some((russian, crate::ui::fonts::selected(), crate::ui::fonts::revision(), widths));
        widths
    })
}

pub fn tab_rects(pill: Rect, scale: f32, ui: &UiState) -> [Rect; TABS.len()] {
    let active = bar_tab(ui.tab);
    let (x, y, w, _) = pill;
    let (bh, gap, icon_w) = (36.0 * scale, 4.0 * scale, 36.0 * scale);
    let top = y + 12.0 * scale;
    let mut left = x + 12.0 * scale;
    let more = more_rect(pill, scale);
    let right = vpn_btn_rect(pill, scale, ui).map_or(more.0, |r| r.0);
    let dots = [mic_on(ui), ui.privacy.cam].iter().filter(|on| **on).count() as f32 * DOT_STEP * scale;
    let room = (right - dots - 12.0 * scale - left).max(icon_w);
    let home = TABS.iter().position(|tab| tab.0 == Tab::Home).unwrap();
    let available: Vec<_> = std::iter::once(home).chain(movable_tabs(ui)).collect();
    let active_index = available.iter().copied().find(|&i| TABS[i].0 == active);
    let labels = header_has_labels(ui);
    let label_widths = header_label_widths();
    let padding = if ui.cfg.header_icons { 32.0 } else { 16.0 };
    let desired: f32 = available.iter().map(|&i| label_widths[i]).sum();
    let fixed = padding * available.len() as f32 + gap / scale * (available.len() - 1) as f32;
    let label_scale =
        ((room / scale - fixed) / desired.max(1.0)).clamp(if ui.cfg.header_icons { 0.78 } else { 0.0 }, 1.0);
    let width = |i: usize| {
        if labels { (padding + label_widths[i] * label_scale) * scale } else { icon_w }
    };
    let mut selected = Vec::new();
    let mut used = 0.0;
    for i in available.iter().copied().filter(|&i| TABS[i].0 == Tab::Home || Some(i) == active_index) {
        let extra = width(i) + if selected.is_empty() { 0.0 } else { gap };
        if used + extra <= room + 0.01 {
            selected.push(i);
            used += extra;
        }
    }
    for &i in &available {
        if !selected.contains(&i) && used + gap + width(i) <= room + 0.01 {
            selected.push(i);
            used += gap + width(i);
        }
    }
    let mut rects = [(0.0, 0.0, 0.0, 0.0); TABS.len()];
    for i in available.into_iter().filter(|i| selected.contains(i)) {
        rects[i] = (left, top, left + width(i), top + bh);
        left += width(i) + gap;
    }
    let r = x + w - 12.0 * scale;
    let settings = TABS.iter().position(|t| t.0 == Tab::Settings).unwrap();
    rects[settings] = (r - 32.0 * scale, top, r, top + bh);
    rects
}

pub fn overflow_tabs(pill: Rect, scale: f32, ui: &UiState) -> Vec<usize> {
    let rects = tab_rects(pill, scale, ui);
    tab_order(&ui.cfg)
        .into_iter()
        .filter(|&i| {
            TABS[i].0 != Tab::Settings && tab_in_bar(&ui.cfg, TABS[i].0, TABS[i].2) && rects[i].2 <= rects[i].0
        })
        .collect()
}

pub fn tab_drag_target_for(rects: &[Rect], r: &Reorder, items: &[usize], j: usize) -> (f32, f32) {
    let mut order: Vec<usize> = (0..items.len()).collect();
    let moved = order.remove(r.from);
    order.insert(r.to.min(order.len()), moved);
    let first = items.iter().map(|&i| rects[i].0).fold(f32::MAX, f32::min);
    let gap = 1.0;
    let mut left = first;
    for &k in &order {
        let w = rects[items[k]].2 - rects[items[k]].0;
        if k == j {
            return (left, rects[items[j]].1);
        }
        left += w + gap;
    }
    (rects[items[j]].0, rects[items[j]].1)
}

pub fn subtab_strip(pill: Rect, scale: f32, ui: &UiState) -> Option<(Rect, Vec<(Tab, Rect)>)> {
    let group = bar_tab(ui.tab);
    let (_, subs) = SUBTABS.iter().find(|g| g.0 == group)?;
    let chip_w = |name: &str| (34.0 + crate::ui::i18n::tr(name).chars().count() as f32 * 7.6 + 12.0) * scale;
    let (h, gap, pad) = (30.0 * scale, 4.0 * scale, 3.0 * scale);
    let total: f32 = subs.iter().map(|s| chip_w(s.2)).sum::<f32>() + gap * (subs.len() - 1) as f32 + 2.0 * pad;
    let left = pill.0 + (pill.2 - total) / 2.0;
    let top = pill.1 + 50.0 * scale;
    let mut x = left + pad;
    let chips = subs
        .iter()
        .map(|s| {
            let r = (x, top + pad / 2.0, x + chip_w(s.2), top + h - pad / 2.0);
            x += chip_w(s.2) + gap;
            (s.0, r)
        })
        .collect();
    Some(((left, top, left + total, top + h), chips))
}

pub fn hit_subtab(pill: Rect, scale: f32, px: f32, py: f32, ui: &UiState) -> Option<Tab> {
    subtab_strip(pill, scale, ui)?.1.into_iter().find(|(_, r)| inside(*r, px, py)).map(|(t, _)| t)
}

pub fn hit_tab(pill: Rect, scale: f32, px: f32, py: f32, ui: &UiState) -> Option<usize> {
    tab_rects(pill, scale, ui).iter().position(|&(l, t, r, b)| px >= l && px < r && py >= t && py < b)
}

pub fn pin_rect(pill: Rect, scale: f32) -> Rect {
    let (x, y, w, _) = pill;
    let r = x + w - 12.0 * scale - 32.0 * scale - 2.0 * scale;
    (r - 32.0 * scale, y + 12.0 * scale, r, y + 48.0 * scale)
}

const LOCK_STEP: f32 = 34.0;
const DOT_STEP: f32 = 20.0;

pub fn more_rect(pill: Rect, scale: f32) -> Rect {
    let pin = pin_rect(pill, scale);
    (pin.0 - 66.0 * scale, pin.1, pin.0 - 6.0 * scale, pin.3)
}

pub fn vpn_btn_rect(pill: Rect, scale: f32, ui: &UiState) -> Option<Rect> {
    vpn_button(ui).then(|| {
        let more = more_rect(pill, scale);
        (more.0 - LOCK_STEP * scale, more.1, more.0 - 2.0 * scale, more.3)
    })
}

pub fn hit_vpn_btn(pill: Rect, scale: f32, ui: &UiState, px: f32, py: f32) -> bool {
    vpn_btn_rect(pill, scale, ui).is_some_and(|r| inside(r, px, py))
}

pub fn mic_on(ui: &UiState) -> bool {
    ui.privacy.mic || ui.mic_muted == Some(true)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HeadHit {
    Pin,
    More,
    Mic,
    Cam,
}

pub fn privacy_dots(pill: Rect, scale: f32, ui: &UiState) -> (Option<Rect>, Option<Rect>) {
    let sw = more_rect(pill, scale);
    let size = 11.0 * scale;
    let mid = (sw.1 + sw.3) / 2.0;
    let mut right = vpn_btn_rect(pill, scale, ui).map_or(sw.0, |r| r.0) - 6.0 * scale;
    let mut next = |on: bool| {
        on.then(|| {
            let r = (right - size, mid - size / 2.0, right, mid + size / 2.0);
            right -= DOT_STEP * scale;
            r
        })
    };
    let mic = next(mic_on(ui));
    let cam = next(ui.privacy.cam);
    (mic, cam)
}

pub fn hit_head(pill: Rect, scale: f32, ui: &UiState, px: f32, py: f32) -> Option<HeadHit> {
    if inside(pin_rect(pill, scale), px, py) {
        return Some(HeadHit::Pin);
    }
    if inside(more_rect(pill, scale), px, py) {
        return Some(HeadHit::More);
    }
    let pad = 5.0 * scale;
    let near =
        |r: Rect| px >= r.0 - pad && px < r.2 + pad && py >= pin_rect(pill, scale).1 && py < pin_rect(pill, scale).3;
    let (mic, cam) = privacy_dots(pill, scale, ui);
    if mic.is_some_and(near) {
        Some(HeadHit::Mic)
    } else if cam.is_some_and(near) {
        Some(HeadHit::Cam)
    } else {
        None
    }
}

fn active_header_rect(rects: &[Rect; TABS.len()], pill: Rect, scale: f32, ui: &UiState) -> Rect {
    let index = TABS.iter().position(|tab| tab.0 == bar_tab(ui.tab)).unwrap_or(0);
    let rect = rects[index];
    if let Some(drag) = &ui.tab_drag
        && drag.items.get(drag.reorder.from) == Some(&index)
    {
        return drag.reorder.dragged_rect((rect.2 - rect.0, rect.3 - rect.1));
    }
    if rect.2 > rect.0 { rect } else { more_rect(pill, scale) }
}

fn paint_active_header(p: &Painter, rect: Rect, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let coordinates = [rect.0, rect.1, rect.2, rect.3].map(|target| target / scale);
    let mut values = coordinates;
    if ui.cfg.island_animations {
        for (index, value) in values.iter_mut().enumerate() {
            *value = ui.feedback.follow(310_000 + index as u64, *value, *value, 260);
        }
    }
    let rect = (values[0] * scale, values[1] * scale + 3.0 * scale, values[2] * scale, values[3] * scale - 3.0 * scale);
    let radius = (rect.3 - rect.1) / 2.0;
    let (r, g, b) = crate::ui::theme::accent();
    let color = (0.4 + r * 0.6, 0.4 + g * 0.6, 0.4 + b * 0.6);
    p.push_clip((pill.0, pill.1, pill.0 + pill.2, pill.1 + 52.0 * scale));
    let result = (|| {
        p.fill_round_rect(rect, radius, rgba(color.0, color.1, color.2, 0.16))?;
        p.stroke_round_rect(
            (rect.0 + 0.5 * scale, rect.1 + 0.5 * scale, rect.2 - 0.5 * scale, rect.3 - 0.5 * scale),
            radius,
            scale,
            rgba(color.0, color.1, color.2, 0.34),
        )?;
        let center = (rect.0 + rect.2) / 2.0;
        p.fill_round_rect(
            (center - 6.0 * scale, rect.3 - 2.0 * scale, center + 6.0 * scale, rect.3),
            scale,
            rgba(color.0, color.1, color.2, 0.95),
        )
    })();
    p.pop_clip();
    result
}

pub fn paint_expanded(p: &Painter, pill: Rect, scale: f32, ui: &UiState, clips: &[Clip]) -> Result<()> {
    if ui.remind.alert.is_some() {
        return paint_alert(p, pill, scale, ui);
    }
    if ui.cmd.open {
        return paint_command(p, pill, scale, ui);
    }

    let mut rects = tab_rects(pill, scale, ui);
    let carried = ui.tab_drag.as_ref();
    if let Some(d) = carried {
        for (j, &i) in d.items.iter().enumerate() {
            if j != d.reorder.from && j < d.reorder.pos.len() {
                let (w, h) = (rects[i].2 - rects[i].0, rects[i].3 - rects[i].1);
                rects[i] = (d.reorder.pos[j].0, d.reorder.pos[j].1, d.reorder.pos[j].0 + w, d.reorder.pos[j].1 + h);
            }
        }
    }
    let carried_index = carried.and_then(|d| d.items.get(d.reorder.from).copied());
    if ui.cfg.header_active_accent {
        paint_active_header(p, active_header_rect(&rects, pill, scale, ui), pill, scale, ui)?;
    }
    let mut order: Vec<usize> = (0..TABS.len()).filter(|&i| Some(i) != carried_index).collect();
    order.extend(carried_index);
    for i in order {
        let mut rect = &rects[i];
        let lifted;
        if Some(i) == carried_index
            && let Some(d) = carried
        {
            let r0 = rects[i];
            lifted = d.reorder.dragged_rect((r0.2 - r0.0, r0.3 - r0.1));
            rect = &lifted;
            p.fill_round_rect(
                (lifted.0 - 2.0 * scale, lifted.1 - 2.0 * scale, lifted.2 + 2.0 * scale, lifted.3 + 2.0 * scale),
                (lifted.3 - lifted.1) / 2.0 + 2.0 * scale,
                rgba(1.0, 1.0, 1.0, 0.1),
            )?;
        }
        let (tab, glyph, name) = TABS[i];
        if rect.2 <= rect.0 {
            continue;
        }
        let selected = tab == bar_tab(ui.tab);
        let radius = (rect.3 - rect.1) / 2.0;
        let selection = ui.feedback.sample(300_000 + i as u64, f32::from(selected), f32::from(selected), 180);
        let hover = ui.feedback.sample(400_000 + i as u64, f32::from(ui.hover_tab == Some(i)), 0.0, 140);
        let base = if !header_has_labels(ui) && tab != Tab::Settings { 0.045 } else { 0.0 };
        let opacity = if ui.cfg.header_active_accent {
            0.035 * hover * (1.0 - selection)
        } else {
            base + (0.16 - base) * selection + 0.09 * hover * (1.0 - selection)
        };
        p.fill_round_rect(*rect, radius, rgba(1.0, 1.0, 1.0, opacity))?;
        let alpha = 0.62 + 0.38 * selection;
        let color = header_icon_color(tab, ui, alpha);
        let glyph = if tab == Tab::Yandex { crate::ui::icons::YANDEX_MUSIC_GLYPH } else { glyph };
        if header_has_labels(ui) && tab != Tab::Settings {
            let inset = if ui.cfg.header_icons { 28.0 } else { 4.0 } * scale;
            if ui.cfg.header_icons {
                p.glyph(glyph, (rect.0 + 6.0 * scale, rect.1, rect.0 + 24.0 * scale, rect.3), 14.0 * scale, color)?;
            }
            let available = (rect.2 - rect.0 - inset - 6.0 * scale).max(scale);
            let mut font = (available / header_label_widths()[i].max(1.0) * 13.0).min(13.0 * scale);
            let label = crate::ui::i18n::tr(name);
            let measured =
                crate::ui::fonts::width(crate::ui::fonts::selected(), &label, font, DWRITE_FONT_WEIGHT_SEMI_BOLD);
            if measured > available {
                font *= available / measured * 0.99;
            }
            p.text(
                name,
                (rect.0 + inset, rect.1, rect.2 - 4.0 * scale, rect.3),
                font,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                rgba(1.0, 1.0, 1.0, alpha),
            )?;
        } else {
            p.glyph(glyph, *rect, 17.0 * scale, color)?;
        }
    }
    let pin = pin_rect(pill, scale);
    if ui.pinned || ui.hover_head == Some(HeadHit::Pin) {
        p.fill_round_rect(pin, (pin.3 - pin.1) / 2.0, rgba(1.0, 1.0, 1.0, if ui.pinned { 0.16 } else { 0.09 }))?;
    }
    p.glyph(
        if ui.pinned { '' } else { '' },
        pin,
        16.0 * scale,
        rgba(1.0, 1.0, 1.0, if ui.pinned { 1.0 } else { 0.62 }),
    )?;
    let more = more_rect(pill, scale);
    let overflow_active = overflow_tabs(pill, scale, ui).iter().any(|&i| TABS[i].0 == bar_tab(ui.tab));
    if !ui.cfg.header_active_accent && overflow_active || ui.hover_head == Some(HeadHit::More) {
        p.fill_round_rect(
            more,
            (more.3 - more.1) / 2.0,
            if ui.cfg.header_active_accent {
                rgba(1.0, 1.0, 1.0, 0.035)
            } else if overflow_active {
                acc(0.2)
            } else {
                rgba(1.0, 1.0, 1.0, 0.09)
            },
        )?;
    }
    p.text(
        "More",
        more,
        13.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_CENTER,
        rgba(1.0, 1.0, 1.0, 0.78),
    )?;
    if let Some(r) = vpn_btn_rect(pill, scale, ui) {
        p.glyph(VPN_GLYPH, r, 16.0 * scale, vpn_color(ui))?;
    }
    let (mic, cam) = privacy_dots(pill, scale, ui);
    if let Some(r) = mic {
        let (glyph, color) = super::pill::mic_look(ui);
        if ui.hover_head == Some(HeadHit::Mic) {
            let grow = 3.0 * scale;
            let halo = (r.0 - grow, r.1 - grow, r.2 + grow, r.3 + grow);
            p.fill_round_rect(halo, (halo.2 - halo.0) / 2.0, rgba(color.r, color.g, color.b, 0.25))?;
        }
        let (cx, cy) = ((r.0 + r.2) / 2.0, (r.1 + r.3) / 2.0);
        let half = 8.0 * scale;
        p.glyph(glyph, (cx - half, cy - half, cx + half, cy + half), 14.0 * scale, color)?;
    }
    if let Some(r) = cam {
        let color = rgba(0.3, 0.85, 0.45, 1.0);
        if ui.hover_head == Some(HeadHit::Cam) {
            let grow = 3.0 * scale;
            let halo = (r.0 - grow, r.1 - grow, r.2 + grow, r.3 + grow);
            p.fill_round_rect(halo, (halo.2 - halo.0) / 2.0, rgba(color.r, color.g, color.b, 0.25))?;
        }
        let (cx, cy) = ((r.0 + r.2) / 2.0, (r.1 + r.3) / 2.0);
        let half = 8.0 * scale;
        p.glyph('\u{E714}', (cx - half, cy - half, cx + half, cy + half), 14.0 * scale, color)?;
    }

    if let Some((strip, chips)) = subtab_strip(pill, scale, ui) {
        p.fill_round_rect(strip, (strip.3 - strip.1) / 2.0, rgba(1.0, 1.0, 1.0, 0.06))?;
        let group = bar_tab(ui.tab);
        let subs = SUBTABS.iter().find(|g| g.0 == group).map_or(&[][..], |g| &g.1[..]);
        for (tab, r) in chips {
            let on = tab == ui.tab;
            let hot = ui.hover_sub == Some(tab);
            if on || hot {
                p.fill_round_rect(r, (r.3 - r.1) / 2.0, rgba(1.0, 1.0, 1.0, if on { 0.18 } else { 0.09 }))?;
            }
            let Some(&(_, glyph, name)) = subs.iter().find(|s| s.0 == tab) else { continue };
            let alpha = if on { 1.0 } else { 0.65 };
            p.glyph(
                glyph,
                (r.0 + 4.0 * scale, r.1, r.0 + 30.0 * scale, r.3),
                15.0 * scale,
                rgba(1.0, 1.0, 1.0, alpha),
            )?;
            p.text(
                name,
                (r.0 + 30.0 * scale, r.1, r.2 - 4.0 * scale, r.3),
                12.5 * scale,
                if on { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
                DWRITE_TEXT_ALIGNMENT_LEADING,
                rgba(1.0, 1.0, 1.0, alpha),
            )?;
        }
    }
    if let Some(change) = ui.tab_change.filter(|c| c.from != ui.tab) {
        let t = (change.start.elapsed().as_secs_f32() * 1000.0 / change.ms.max(1) as f32).clamp(0.0, 1.0);
        let progress = crate::ui::anim::ease(ui.cfg.anim_curve, t);
        let (old, new) = crate::ui::anim::tab_looks(ui.cfg.tab_anim, progress, change.dir);
        let bar_bottom = tab_rects(pill, scale, ui).iter().map(|r| r.3).fold(pill.1, f32::max) + 4.0 * scale;
        let top = if bar_tab(ui.tab) != ui.tab { body_rect(pill, scale, ui.tab).1 } else { bar_bottom };
        let area = (pill.0, top, pill.0 + pill.2, pill.1 + pill.3);
        let (w, h) = (area.2 - area.0, area.3 - area.1);
        p.push_clip(area);
        let mut result = Ok(());
        for (tab, look) in [(change.from, old), (ui.tab, new)] {
            if look.alpha <= 0.003 {
                continue;
            }
            let bitmap = p.offscreen((w, h), (area.0, area.1), |q| paint_tab(q, tab, pill, scale, ui, clips));
            match bitmap {
                Ok(bitmap) => {
                    let (cx, cy) = (area.0 + w / 2.0 + look.dx * w, area.1 + h / 2.0 + look.dy * h);
                    let (hw, hh) = (w * look.scale / 2.0, h * look.scale / 2.0);
                    p.draw_bitmap(&bitmap, (cx - hw, cy - hh, cx + hw, cy + hh), look.alpha.clamp(0.0, 1.0));
                }
                Err(e) => result = result.and(Err(e)),
            }
        }
        p.pop_clip();
        if result.is_err() {
            result = paint_tab(p, ui.tab, pill, scale, ui, clips);
        }
        return result;
    }
    paint_tab(p, ui.tab, pill, scale, ui, clips)
}

fn paint_tab(p: &Painter, tab: Tab, pill: Rect, scale: f32, ui: &UiState, clips: &[Clip]) -> Result<()> {
    let pill = body_rect(pill, scale, tab);
    match tab {
        Tab::Home => paint_home(p, pill, scale, ui)?,
        Tab::Weather => paint_weather(p, pill, scale, ui)?,
        Tab::Ai => {}
        Tab::Agy => paint_agy(p, pill, scale, ui)?,
        Tab::Chat => paint_chat(p, pill, scale, ui)?,
        Tab::Privacy => paint_privacy(p, pill, scale, ui)?,
        Tab::Activity => paint_activity(p, pill, scale, ui)?,
        Tab::Steam => paint_steam(p, pill, scale, ui)?,
        Tab::Clipboard => paint_clipboard(p, pill, scale, ui, clips)?,
        Tab::Controls => {
            paint_controls(p, pill, scale, ui)?;
            paint_metrics_row(p, pill, scale, ui)?;
        }
        Tab::Yandex => paint_yandex(
            p,
            pill,
            scale,
            &ui.yandex,
            (ui.editing && !ui.cmd.open, ui.caret_on),
            ui.home.seek,
            crate::services::yandex::progress(),
        )?,
        Tab::Mixer => paint_mixer(p, pill, scale, &ui.mixer)?,
        Tab::Notifications => paint_notifications(
            p,
            pill,
            scale,
            &ui.notifications,
            ui.cfg.system_notifications,
            ui.cfg.notification_open_app,
        )?,
        Tab::Note => paint_note(p, pill, scale, ui)?,
        Tab::Text => paint_text(p, pill, scale, ui)?,
        Tab::Settings => {}
        Tab::Record => paint_record(p, pill, scale, ui)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_header_targets_the_current_tab_or_its_overflow_button() {
        let mut ui = UiState::new();
        for scale in [0.75, 1.0, 1.5, 2.0] {
            for width in [460.0, 640.0, 800.0] {
                let pill = (12.0, 8.0, width * scale, 384.0 * scale);
                for &(tab, _, _) in &TABS {
                    ui.tab = tab;
                    let rects = tab_rects(pill, scale, &ui);
                    let target = active_header_rect(&rects, pill, scale, &ui);
                    assert!(target.2 > target.0 && target.3 > target.1);
                    let active = TABS.iter().position(|entry| entry.0 == bar_tab(tab)).unwrap();
                    assert_eq!(
                        target,
                        if rects[active].2 > rects[active].0 { rects[active] } else { more_rect(pill, scale) }
                    );
                }
            }
        }
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(settings.header_active_accent);
        super::settings::toggle_set(super::settings::Sid::HeaderActiveAccent, &mut settings, false);
        let saved: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert!(!saved.header_active_accent);
    }

    #[test]
    fn removing_an_active_header_tab_moves_it_to_more_without_hiding_the_feature() {
        let mut ui = UiState::new();
        ui.tab = Tab::Weather;
        assert!(ui.cfg.set_header_tab("Weather", false, None));
        let pill = (0.0, 0.0, 620.0, 400.0);
        let index = TABS.iter().position(|tab| tab.0 == Tab::Weather).unwrap();
        assert!(overflow_tabs(pill, 1.0, &ui).contains(&index));
        assert_eq!(tab_rects(pill, 1.0, &ui)[index], (0.0, 0.0, 0.0, 0.0));
        assert!(ui.cfg.set_header_tab("Weather", true, None));
        assert!(!overflow_tabs(pill, 1.0, &ui).contains(&index));
        assert!(tab_rects(pill, 1.0, &ui)[index].2 > 0.0);
    }

    #[test]
    fn compact_header_keeps_active_tabs_and_tools_separate() {
        let mut ui = UiState::new();
        ui.privacy.mic = true;
        ui.privacy.cam = true;
        ui.vpn = Some(true);
        ui.cfg.vpn_pill = true;
        for scale in [0.75, 1.0, 1.25, 1.5, 2.0] {
            for width in [460.0, 520.0, 620.0, 800.0] {
                let pill = (17.0, 9.0, width * scale, 400.0 * scale);
                for (i, &(tab, _, name)) in TABS.iter().enumerate() {
                    if tab == Tab::Settings || !tab_in_bar(&ui.cfg, tab, name) {
                        continue;
                    }
                    ui.tab = tab;
                    ui.cfg.header_tabs = crate::data::settings::DEFAULT_HEADER_TABS.map(String::from).to_vec();
                    ui.cfg.set_header_tab(name, true, Some("Record"));
                    let rects = tab_rects(pill, scale, &ui);
                    assert!(rects[i].2 > rects[i].0, "active {name} disappeared at {width}/{scale}");
                    let overflow = overflow_tabs(pill, scale, &ui);
                    assert!(!overflow.contains(&i));
                    let mut boxes: Vec<_> = rects.into_iter().filter(|r| r.2 > r.0).collect();
                    assert!(boxes.len() <= 7);
                    boxes.extend([more_rect(pill, scale), pin_rect(pill, scale)]);
                    boxes.extend(vpn_btn_rect(pill, scale, &ui));
                    let (mic, cam) = privacy_dots(pill, scale, &ui);
                    boxes.extend(mic);
                    boxes.extend(cam);
                    boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
                    for pair in boxes.windows(2) {
                        assert!(pair[0].2 <= pair[1].0 + 0.01, "overlap at {width}/{scale}: {pair:?}");
                    }
                    for (j, &(other, _, label)) in TABS.iter().enumerate() {
                        if other != Tab::Settings && tab_in_bar(&ui.cfg, other, label) {
                            assert_eq!(overflow.contains(&j), rects[j].2 <= rects[j].0);
                        }
                    }
                    let more = more_rect(pill, scale);
                    assert!(matches!(
                        hit_head(pill, scale, &ui, (more.0 + more.2) / 2.0, (more.1 + more.3) / 2.0),
                        Some(HeadHit::More)
                    ));
                }
            }
        }
    }

    #[test]
    fn sixth_extra_tab_switches_the_whole_header_to_compact_icons() {
        let mut ui = UiState::new();
        assert!(header_has_labels(&ui));
        ui.cfg.set_header_tab("Notifications", true, None);
        assert!(!header_has_labels(&ui));
        for scale in [0.75, 1.0, 1.5, 2.0] {
            let pill = (0.0, 0.0, 640.0 * scale, 384.0 * scale);
            for &tab in &movable_tabs(&ui) {
                let rect = tab_rects(pill, scale, &ui)[tab];
                assert_eq!(rect.2 - rect.0, 36.0 * scale);
                assert!(!overflow_tabs(pill, scale, &ui).contains(&tab));
            }
        }
        ui.cfg.set_header_tab("Notifications", false, None);
        assert!(header_has_labels(&ui));
        ui.cfg.header_tabs =
            ["Privacy", "Activity", "Controls", "App mixer", "Notifications"].map(String::from).to_vec();
        ui.cfg.header_icons = false;
        let rects = tab_rects((0.0, 0.0, 640.0, 384.0), 1.0, &ui);
        assert!(movable_tabs(&ui).iter().all(|&i| rects[i].2 > rects[i].0));
        ui.cfg.header_icons = true;
        ui.tab = Tab::Notifications;
        let rects = tab_rects((0.0, 0.0, 640.0, 384.0), 1.0, &ui);
        let active = TABS.iter().position(|tab| tab.0 == Tab::Notifications).unwrap();
        assert!(rects[active].2 > rects[active].0);
        assert!(ui.cfg.header_icons);
        let saved: Settings = serde_json::from_str(&serde_json::to_string(&ui.cfg).unwrap()).unwrap();
        assert!(saved.header_icons);
        assert!(!saved.header_icon_colors);
    }

    #[test]
    fn every_hideable_tab_exists_and_the_fixed_ones_stay() {
        for name in crate::data::settings::HIDEABLE_TABS {
            assert!(TABS.iter().any(|t| t.2 == name), "{name} is not a tab");
        }
        let all_hidden = Settings {
            hidden_tabs: crate::data::settings::HIDEABLE_TABS.map(String::from).to_vec(),
            ..Settings::default()
        };
        assert!(tab_in_bar(&all_hidden, Tab::Home, "Home"));
        assert!(tab_in_bar(&all_hidden, Tab::Settings, "Settings"));
        assert!(!tab_in_bar(&all_hidden, Tab::Note, "Note"));
        assert!(tab_in_bar(&Settings::default(), Tab::Note, "Note"));
    }
}
