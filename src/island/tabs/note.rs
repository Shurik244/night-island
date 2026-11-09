use crate::island::*;

pub(in crate::island) const NOTE_SAVE_MS: u32 = 800;
const DELETE_ARM_MS: u32 = 3000;

impl App {
    pub(in crate::island) fn rename_active(&self) -> bool {
        self.ui.tab == view::Tab::Note && self.ui.notes.renaming.is_some() && self.ui.editing
    }

    pub(in crate::island) fn note_inner(&self) -> view::Rect {
        self.field_frame().1
    }

    pub(in crate::island) fn note_layout(&self) -> Option<windows::Win32::Graphics::DirectWrite::IDWriteTextLayout> {
        let inner = self.note_inner();
        let one_line = self.rename_active();
        crate::ui::gfx::make_layout(
            self.gpu.dwrite(),
            &self.multi().text(),
            if one_line { 100_000.0 } else { inner.2 - inner.0 },
            if self.rename_active() {
                13.0
            } else if self.ui.tab == view::Tab::Agy {
                view::AGY_FONT
            } else {
                view::NOTE_FONT
            } * self.scale,
        )
        .ok()
    }

    pub(in crate::island) fn save_note(&self) {
        self.kill_timer(Timer::NoteSave);
        if let (Some(store), Some(id)) = (&self.store, self.ui.notes.current) {
            let text = self.ui.note.text();
            store.note_set_text(id, &text);
            if let Some((rid, _)) = self.ui.notes.remind {
                let line: String =
                    text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().chars().take(200).collect();
                if !line.is_empty() {
                    store.reminder_set_text(rid, &line);
                }
            }
        }
    }

    pub(in crate::island) fn note_changed(&mut self) {
        if self.wave_search() {
            return self.wave_search_changed();
        }
        if self.extra_field().is_some() {
            self.set_timer(Timer::Geo, 450);
        }
        if self.extra_field().is_some() {
            self.ui.caret_on = true;
            self.set_timer(Timer::Caret, CARET_BLINK_MS);
            return self.repaint_expanded();
        }
        if self.clip_search() {
            self.ui.caret_on = true;
            self.set_timer(Timer::Caret, CARET_BLINK_MS);
            self.ui.clip.scroll = 0;
            self.apply_clip_filter();
            return self.repaint_expanded();
        }
        if self.steam_search() {
            self.ui.caret_on = true;
            self.set_timer(Timer::Caret, CARET_BLINK_MS);
            self.apply_steam_filter();
            return self.repaint_expanded();
        }
        if self.ui.cmd.open {
            self.ui.caret_on = true;
            self.set_timer(Timer::Caret, CARET_BLINK_MS);
            self.refresh_cmd();
            return self.repaint_expanded();
        }
        if self.ui.tab == view::Tab::Note && self.ui.notes.renaming.is_none() {
            self.set_timer(Timer::NoteSave, NOTE_SAVE_MS);
        }
        self.ui.caret_on = true;
        if self.ui.editing {
            self.set_timer(Timer::Caret, CARET_BLINK_MS);
        }
        self.ensure_caret_visible();
        self.repaint_expanded();
    }

    pub(in crate::island) fn init_notes(&mut self) {
        self.reload_notes();
        let last = self.store.as_ref().and_then(Store::note_last);
        let first_sheet = self.ui.notes.items.iter().find(|i| !i.folder).map(|i| i.id);
        let pick = last.filter(|id| self.ui.notes.item(*id).is_some_and(|i| !i.folder)).or(first_sheet);
        match pick {
            Some(id) => self.open_note(id),
            None => self.ensure_sheet(),
        }
    }

    pub(in crate::island) fn reload_notes(&mut self) {
        let items = self.store.as_ref().map(Store::note_items).unwrap_or_default();
        let n = &mut self.ui.notes;
        n.items = items;
        let ids: std::collections::HashSet<i64> = n.items.iter().map(|i| i.id).collect();
        n.expanded.retain(|id| ids.contains(id));
        n.sel = n.sel.filter(|id| ids.contains(id));
        n.current = n.current.filter(|id| ids.contains(id));
        let rows = n.rows().len();
        n.scroll = n.scroll.min(rows.saturating_sub(1));
    }

    fn ensure_sheet(&mut self) {
        let existing = self.ui.notes.items.iter().find(|i| !i.folder).map(|i| i.id);
        let id = existing.or_else(|| self.store.as_ref()?.note_add(None, false, &crate::ui::i18n::tr("New note")));
        self.reload_notes();
        if let Some(id) = id {
            self.open_note(id);
        }
    }

    pub(in crate::island) fn open_note(&mut self, id: i64) {
        if self.ui.notes.current != Some(id) {
            self.save_note();
        }
        let Some(store) = &self.store else { return };
        let text = store.note_text(id);
        store.note_set_last(id);
        self.ui.note.set_text(&text);
        self.ui.notes.current = Some(id);
        self.ui.notes.sel = Some(id);
        self.ui.notes.armed_delete = false;
        self.ui.notes.reveal(id);
        self.ensure_row_visible(id);
        self.ui.notes.picker = None;
        self.refresh_note_remind();
        self.repaint_expanded();
    }

    pub(in crate::island) fn refresh_note_remind(&mut self) {
        let remind = match (&self.store, self.ui.notes.current) {
            (Some(store), Some(id)) => store.note_remind(id),
            _ => None,
        };
        self.ui.notes.remind = remind;
        if let Some(store) = &self.store {
            self.ui.notes.items = store.note_items();
        }
    }

    pub(in crate::island) fn toggle_note_type(&mut self) {
        let (Some(id), Some(store)) = (self.ui.notes.current, &self.store) else { return };
        match self.ui.notes.remind {
            Some((rid, _)) => {
                store.reminder_remove(rid);
                store.note_set_remind(id, 0);
            }
            None => {
                let name = self.ui.notes.item(id).map(|i| i.name.clone()).unwrap_or_default();
                let text = self.ui.note.text();
                let line: String =
                    text.lines().find(|l| !l.trim().is_empty()).unwrap_or(&name).trim().chars().take(200).collect();
                let due = chrono::Local::now().timestamp() + 3600;
                if let Some(rid) = store.reminder_add_id(due, if line.is_empty() { "Reminder" } else { &line }) {
                    store.note_set_remind(id, rid);
                }
            }
        }
        self.reload_reminders();
        self.repaint_expanded();
    }

    fn change_remind(
        &mut self,
        f: impl FnOnce(chrono::DateTime<chrono::Local>) -> Option<chrono::DateTime<chrono::Local>>,
    ) {
        use chrono::{Duration, Local, TimeZone};
        let Some((rid, due)) = self.ui.notes.remind else { return };
        let now = Local::now();
        let current = Local.timestamp_opt(due, 0).single().unwrap_or(now);
        let Some(next) = f(current) else { return };
        let next = next.max(now + Duration::minutes(1));
        if let Some(store) = &self.store {
            store.reminder_set_due(rid, next.timestamp());
        }
        self.reload_reminders();
    }

    fn picker_click(&mut self, hit: view::NoteHit) {
        use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Timelike};
        match hit {
            view::NoteHit::PickMonth(d) => {
                let (y, m) = self.ui.notes.picker.unwrap_or((2026, 1));
                let index = y * 12 + m as i32 - 1 + d;
                let today = Local::now();
                if index >= today.year() * 12 + today.month() as i32 - 1 {
                    self.ui.notes.picker = Some((index.div_euclid(12), index.rem_euclid(12) as u32 + 1));
                }
            }
            view::NoteHit::PickDay(day) => {
                let (y, m) = self.ui.notes.picker.unwrap_or((2026, 1));
                if NaiveDate::from_ymd_opt(y, m, day).is_some_and(|d| d >= Local::now().date_naive()) {
                    self.change_remind(|t| {
                        let date = NaiveDate::from_ymd_opt(y, m, day)?;
                        Local.from_local_datetime(&date.and_time(t.time())).single()
                    });
                }
            }
            view::NoteHit::PickHour(d) => self.change_remind(|t| Some(t + Duration::hours(i64::from(d)))),
            view::NoteHit::PickMinute(d) => self.change_remind(|t| {
                let m = t.minute() as i32;
                let to = if d > 0 { (m / 5 + 1) * 5 } else { (m + 4) / 5 * 5 - 5 };
                Some(t + Duration::minutes(i64::from(to - m)))
            }),
            view::NoteHit::PickDone => self.ui.notes.picker = None,
            _ => {}
        }
    }

    pub(in crate::island) fn adjust_note_remind(&mut self, i: usize) {
        use chrono::{Duration, Local, TimeZone};
        let Some((rid, due)) = self.ui.notes.remind else { return };
        let now = Local::now();
        let current = Local.timestamp_opt(due, 0).single().unwrap_or(now);
        let next = match i {
            0 => current - Duration::days(1),
            1 => current + Duration::days(1),
            2 => current - Duration::hours(1),
            3 => current + Duration::hours(1),
            4 => current - Duration::minutes(10),
            5 => current + Duration::minutes(10),
            6 => now + Duration::hours(1),
            _ => (now.date_naive() + chrono::Days::new(1))
                .and_hms_opt(9, 0, 0)
                .and_then(|t| Local.from_local_datetime(&t).single())
                .unwrap_or(now + Duration::days(1)),
        };
        let next = next.max(now + Duration::minutes(1));
        if let Some(store) = &self.store {
            store.reminder_set_due(rid, next.timestamp());
        }
        self.reload_reminders();
        self.repaint_expanded();
    }

    fn ensure_row_visible(&mut self, id: i64) {
        let fit = view::tree_rows_fit(self.pill_rect(true), self.scale);
        let Some(index) = self.ui.notes.rows().iter().position(|(_, i)| i.id == id) else { return };
        let n = &mut self.ui.notes;
        if index < n.scroll {
            n.scroll = index;
        } else if index >= n.scroll + fit {
            n.scroll = index + 1 - fit;
        }
    }

    fn add_note_item(&mut self, folder: bool) {
        let parent = self.ui.notes.target_folder();
        let name = crate::ui::i18n::tr(if folder { "New folder" } else { "New note" });
        let Some(id) = self.store.as_ref().and_then(|s| s.note_add(parent, folder, &name)) else {
            return self.notify(Notice::Error, '\u{E783}', "Notes limit reached");
        };
        if let Some(p) = parent {
            self.ui.notes.expanded.insert(p);
        }
        self.reload_notes();
        if folder {
            self.ui.notes.sel = Some(id);
        } else {
            self.open_note(id);
        }
        self.ensure_row_visible(id);
        self.begin_rename(id);
    }

    pub(in crate::island) fn begin_rename(&mut self, id: i64) {
        self.finish_rename(true);
        let Some(name) = self.ui.notes.item(id).map(|i| i.name.clone()) else { return };
        self.ui.notes.name_input.set_text(&name);
        self.ui.notes.name_input.select_all();
        self.ui.notes.renaming = Some(id);
        self.begin_edit();
        self.ui.caret_on = true;
        self.repaint_expanded();
    }

    pub(in crate::island) fn finish_rename(&mut self, apply: bool) {
        let Some(id) = self.ui.notes.renaming.take() else { return };
        if apply {
            let name = self.ui.notes.name_input.text();
            if let Some(store) = &self.store {
                store.note_rename(id, &name);
            }
            self.reload_notes();
        }
        self.repaint_expanded();
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.ui.notes.sel else { return };
        if !self.ui.notes.armed_delete {
            self.ui.notes.armed_delete = true;
            self.set_timer(Timer::Disarm, DELETE_ARM_MS);
            return;
        }
        self.ui.notes.armed_delete = false;
        let current_gone = self.ui.notes.current.is_some_and(|c| {
            let mut at = Some(c);
            let mut hops = 0;
            while let Some(i) = at {
                if i == id {
                    return true;
                }
                hops += 1;
                if hops > crate::data::notes::MAX_DEPTH + 2 {
                    break;
                }
                at = self.ui.notes.item(i).and_then(|n| n.parent);
            }
            false
        });
        if let Some(store) = &self.store {
            store.note_delete(id);
        }
        self.ui.notes.sel = None;
        self.reload_notes();
        if current_gone {
            self.ui.notes.current = None;
            self.ui.note.set_text("");
            self.ensure_sheet();
        }
        self.repaint_expanded();
    }

    fn note_tree_click(&mut self, hit: view::NoteHit) {
        if hit != view::NoteHit::Delete {
            self.ui.notes.armed_delete = false;
        }
        match hit {
            view::NoteHit::Row(index) => {
                let Some(id) = self.ui.notes.rows().get(index).map(|(_, i)| i.id) else { return };
                self.ui.notes.sel = Some(id);
                self.ui.notes.drag_from = Some(id);
                self.ui.notes.drag_xy = self.cursor_client().map_or((0, 0), |(x, y)| (x as i32, y as i32));
                unsafe {
                    windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(self.hwnd);
                }
            }
            view::NoteHit::AddNote => self.add_note_item(false),
            view::NoteHit::AddFolder => self.add_note_item(true),
            view::NoteHit::Rename => {
                if let Some(id) = self.ui.notes.sel {
                    self.begin_rename(id);
                }
            }
            view::NoteHit::Delete => self.delete_selected(),
            _ => {}
        }
        self.repaint_expanded();
    }

    pub(in crate::island) fn click_note(&mut self, x: i32, y: i32) {
        self.ui.notes.drag_from = None;
        let pill = self.pill_rect(true);
        let (fx, fy) = (x as f32, y as f32);
        let inside = |r: view::Rect| fx >= r.0 && fx < r.2 && fy >= r.1 && fy < r.3;
        if self.rename_active() {
            if inside(self.field_frame().0) {
                return self.click_in_field(fx, fy);
            }
            self.finish_rename(true);
            self.end_edit(true);
        }
        if let Some(hit) = view::hit_note_tree(pill, self.scale, fx, fy, &self.ui) {
            return self.note_tree_click(hit);
        }
        let panel = view::hit_note_panel(pill, self.scale, fx, fy, &self.ui);
        if self.ui.notes.picker.is_some() {
            match panel {
                Some(hit) => self.picker_click(hit),
                None => self.ui.notes.picker = None,
            }
            return self.repaint_expanded();
        }
        match panel {
            Some(view::NoteHit::TypeToggle) => return self.toggle_note_type(),
            Some(view::NoteHit::Remind(i)) => return self.adjust_note_remind(i),
            Some(view::NoteHit::RemindOpen) => {
                use chrono::{Datelike, TimeZone};
                let due = self.ui.notes.remind.map_or(0, |r| r.1);
                let when = chrono::Local.timestamp_opt(due, 0).single().unwrap_or_else(chrono::Local::now);
                self.ui.notes.picker = Some((when.year(), when.month()));
                return self.repaint_expanded();
            }
            _ => {}
        }
        if !inside(view::note_frame_for(pill, self.scale, self.ui.notes.remind.is_some()).0) {
            self.end_edit(true);
            return;
        }
        self.click_in_field(fx, fy);
    }

    pub(in crate::island) fn click_in_field(&mut self, fx: f32, fy: f32) {
        let (_, inner) = self.field_frame();
        self.begin_edit();
        let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
        if let Some(layout) = self.note_layout() {
            let scroll = self.multi().scroll;
            self.multi_mut().click(&layout, fx - inner.0, fy - inner.1 + scroll, shift);
        }
        self.pointer.selecting = true;
        self.note_changed();
    }

    fn tree_slots(&self) -> Vec<view::Rect> {
        let pill = self.pill_rect(true);
        let tree = view::note_tree(pill, self.scale);
        let shown =
            view::tree_rows_fit(pill, self.scale).min(self.ui.notes.rows().len().saturating_sub(self.ui.notes.scroll));
        (0..shown).map(|slot| view::tree_row(tree, self.scale, slot)).collect()
    }

    pub(in crate::island) fn step_tree_reorder(&mut self) -> bool {
        let slots = self.tree_slots();
        let rows: Vec<(bool, i64)> = self
            .ui
            .notes
            .rows()
            .iter()
            .skip(self.ui.notes.scroll)
            .take(slots.len())
            .map(|(_, i)| (i.folder, i.id))
            .collect();
        let Some(r) = self.ui.notes.reorder.as_mut() else { return false };
        let cursor = r.cursor;
        let into = slots.iter().enumerate().find_map(|(k, s)| {
            let band = (s.3 - s.1) * 0.25;
            let (folder, id) = rows.get(k).copied()?;
            (folder
                && k != r.from
                && cursor.0 >= s.0
                && cursor.0 < s.2
                && cursor.1 >= s.1 + band
                && cursor.1 < s.3 - band)
                .then_some(id)
        });
        let moving = r.step(&slots, cursor);
        let moving = if into.is_some() {
            r.to = r.from;
            r.step(&slots, cursor) || moving
        } else {
            moving
        };
        self.ui.notes.into = into;
        moving
    }

    pub(in crate::island) fn drag_notes(&mut self, x: i32, y: i32) {
        if self.state != State::Expanded || self.ui.tab != view::Tab::Note {
            return;
        }
        let cursor = (x as f32, y as f32);
        if self.ui.notes.reorder.is_some() {
            if let Some(r) = self.ui.notes.reorder.as_mut() {
                r.cursor = cursor;
            }
            self.step_tree_reorder();
            self.set_timer(Timer::UiAnim, self.frame_ms());
            return self.repaint_expanded();
        }
        let Some(id) = self.ui.notes.drag_from else { return };
        let (x0, y0) = self.ui.notes.drag_xy;
        if (x - x0).abs() < 6 && (y - y0).abs() < 6 {
            return;
        }
        let slots = self.tree_slots();
        let rows = self.ui.notes.rows();
        let Some(index) = rows.iter().position(|(_, i)| i.id == id) else { return };
        let Some(slot) = index.checked_sub(self.ui.notes.scroll).filter(|s| *s < slots.len()) else { return };
        self.ui.notes.reorder = Some(view::Reorder::new(&slots, slot, cursor));
        self.set_timer(Timer::UiAnim, self.frame_ms());
        self.repaint_expanded();
    }

    pub(in crate::island) fn finish_note_drag(&mut self) {
        let Some(from_id) = self.ui.notes.drag_from.take() else { return };
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
        }
        let carried = self.ui.notes.reorder.take();
        let into = self.ui.notes.into.take();
        if let Some(r) = carried {
            if self.state != State::Expanded || self.ui.tab != view::Tab::Note {
                return;
            }
            let rows: Vec<(i64, Option<i64>)> = self.ui.notes.rows().iter().map(|(_, i)| (i.id, i.parent)).collect();
            let scroll = self.ui.notes.scroll;
            let ok = if let Some(folder) = into {
                self.store.as_ref().is_some_and(|s| s.note_place(from_id, Some(folder), None))
            } else if r.to == r.from {
                false
            } else {
                let target = rows.get(scroll + r.to).copied();
                let sibling_after = rows.get(scroll + r.to + 1).copied();
                match target {
                    Some((tid, tparent)) if r.to < r.from => {
                        self.store.as_ref().is_some_and(|s| s.note_place(from_id, tparent, Some(tid)))
                    }
                    Some((_, tparent)) => {
                        let before = sibling_after.filter(|(_, p)| *p == tparent).map(|(i, _)| i);
                        self.store.as_ref().is_some_and(|s| s.note_place(from_id, tparent, before))
                    }
                    None => false,
                }
            };
            if ok {
                if let Some(t) = into {
                    self.ui.notes.expanded.insert(t);
                }
                self.reload_notes();
                self.ui.notes.sel = Some(from_id);
                self.ensure_row_visible(from_id);
            }
            return self.repaint_expanded();
        }
        if self.state != State::Expanded || self.ui.tab != view::Tab::Note {
            return;
        }
        let pill = self.pill_rect(true);
        let Some((rx, ry)) = self.cursor_client() else { return };
        let to = view::hit_note_tree(pill, self.scale, rx, ry, &self.ui);
        let rows = self.ui.notes.rows();
        let Some(from) = rows.iter().position(|(_, i)| i.id == from_id) else { return };
        let (_, moved) = rows[from];
        if to == Some(view::NoteHit::Row(from)) {
            let (id, folder) = (moved.id, moved.folder);
            if folder {
                if !self.ui.notes.expanded.remove(&id) {
                    self.ui.notes.expanded.insert(id);
                }
                self.repaint_expanded();
            } else {
                self.open_note(id);
            }
            return;
        }
        let target: Option<i64> = match to {
            Some(view::NoteHit::Row(j)) => match rows.get(j) {
                Some(&(_, t)) if t.folder => Some(t.id),
                Some(&(_, t)) => t.parent,
                None => return,
            },
            Some(_) => return,
            None => {
                let tree = view::note_tree(pill, self.scale);
                if !(rx >= tree.0 && rx < tree.2 && ry >= tree.1 && ry < tree.3) {
                    return;
                }
                None
            }
        };
        let moved_ok = self.store.as_ref().is_some_and(|s| s.note_move(from_id, target));
        if moved_ok {
            if let Some(t) = target {
                self.ui.notes.expanded.insert(t);
            }
            self.reload_notes();
            self.ui.notes.sel = Some(from_id);
            self.ensure_row_visible(from_id);
        }
        self.repaint_expanded();
    }

    pub(in crate::island) fn scroll_tree(&mut self, delta: i32) {
        let rows = self.ui.notes.rows().len();
        let max = rows.saturating_sub(view::tree_rows_fit(self.pill_rect(true), self.scale));
        let n = &mut self.ui.notes;
        let next = if delta > 0 { n.scroll.saturating_sub(1) } else { (n.scroll + 1).min(max) };
        if next != n.scroll {
            n.scroll = next;
            n.hover = None;
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn cursor_client(&self) -> Option<(f32, f32)> {
        let mut pt = windows::Win32::Foundation::POINT::default();
        unsafe {
            if GetCursorPos(&mut pt).is_err()
                || !windows::Win32::Graphics::Gdi::ScreenToClient(self.hwnd, &mut pt).as_bool()
            {
                return None;
            }
        }
        Some((pt.x as f32, pt.y as f32))
    }

    pub(in crate::island) fn cursor_over_tree(&self) -> bool {
        if self.state != State::Expanded || self.ui.tab != view::Tab::Note {
            return false;
        }
        let Some((x, y)) = self.cursor_client() else { return false };
        let tree = view::note_tree(self.pill_rect(true), self.scale);
        x >= tree.0 && x < tree.2 && y >= tree.1 && y < tree.3
    }

    pub(in crate::island) fn cursor_over_note(&self) -> bool {
        if self.ui.tab == view::Tab::Note
            && let Some((x, y)) = self.cursor_client()
            && view::hit_note_panel(self.pill_rect(true), self.scale, x, y, &self.ui).is_some()
        {
            return false;
        }
        if self.state != State::Expanded || !matches!(self.ui.tab, view::Tab::Note | view::Tab::Text) {
            return false;
        }
        let Some((x, y)) = self.cursor_client() else { return false };
        let pill = self.pill_rect(true);
        let frame = match self.ui.tab {
            view::Tab::Note => view::note_frame_for(pill, self.scale, self.ui.notes.remind.is_some()).0,
            view::Tab::Text => view::text_frame(pill, self.scale).0,
            _ => return false,
        };
        x >= frame.0 && x < frame.2 && y >= frame.1 && y < frame.3
    }
}
