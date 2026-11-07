use crate::island::*;

pub(in crate::island) const CARET_BLINK_MS: u32 = 530;

impl App {
    pub(in crate::island) fn field_frame(&self) -> (view::Rect, view::Rect) {
        let pill = self.pill_rect(true);
        if self.ui.tab == view::Tab::Text {
            view::text_frame(pill, self.scale)
        } else if self.ui.tab == view::Tab::Agy {
            view::agy_input(pill, self.scale, self.ui.agy.input_h.get())
        } else if self.ui.tab == view::Tab::Chat {
            view::agy_input(pill, self.scale, self.ui.chat.input_h.get())
        } else if self.rename_active() {
            view::rename_frame(pill, self.scale, &self.ui)
                .unwrap_or_else(|| view::note_frame_for(pill, self.scale, self.ui.notes.remind.is_some()))
        } else {
            view::note_frame_for(pill, self.scale, self.ui.notes.remind.is_some())
        }
    }

    pub(in crate::island) fn multi(&self) -> &edit::TextEdit {
        match self.ui.tab {
            view::Tab::Text => &self.ui.text.input,
            view::Tab::Agy => &self.ui.agy.input,
            view::Tab::Chat => &self.ui.chat.input,
            view::Tab::Note if self.ui.notes.renaming.is_some() => &self.ui.notes.name_input,
            _ => &self.ui.note,
        }
    }

    pub(in crate::island) fn multi_mut(&mut self) -> &mut edit::TextEdit {
        match self.ui.tab {
            view::Tab::Text => &mut self.ui.text.input,
            view::Tab::Agy => &mut self.ui.agy.input,
            view::Tab::Chat => &mut self.ui.chat.input,
            view::Tab::Note if self.ui.notes.renaming.is_some() => &mut self.ui.notes.name_input,
            _ => &mut self.ui.note,
        }
    }

    pub(in crate::island) fn begin_edit(&mut self) {
        if self.ui.editing {
            return;
        }
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground != self.hwnd {
                self.prev_foreground = foreground;
            }
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
            let _ = SetForegroundWindow(self.hwnd);
        }
        self.ui.editing = true;
        self.ui.caret_on = true;
        self.kill_timer(Timer::Collapse);
        self.set_timer(Timer::Caret, CARET_BLINK_MS);
        self.repaint_expanded();
    }

    pub(in crate::island) fn end_edit(&mut self, restore_focus: bool) {
        if !self.ui.editing {
            return;
        }
        self.ui.editing = false;
        self.pointer.selecting = false;
        self.pointer.reading = false;
        self.ui.chat.sel = None;
        self.kill_timer(Timer::Caret);
        self.finish_rename(true);
        self.save_note();
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
            if restore_focus && IsWindow(Some(self.prev_foreground)).as_bool() {
                let _ = SetForegroundWindow(self.prev_foreground);
            }
        }
        self.prev_foreground = HWND::default();
        self.repaint_expanded();
        self.arm_collapse_if_idle(self.settings.close_delay_ms.max(1));
    }

    pub(in crate::island) fn arm_collapse_if_idle(&mut self, delay_ms: u32) {
        if self.state == State::Expanded
            && !self.pointer.tracking
            && self.ui.remind.alert.is_none()
            && !self.ui.pinned
            && !self.popup_open
            && self.pointer.moving.is_none()
        {
            self.set_timer(Timer::Collapse, delay_ms);
        }
    }

    pub(in crate::island) fn ensure_caret_visible(&mut self) {
        let Some(layout) = self.note_layout() else { return };
        let inner = self.note_inner();
        let view_h = inner.3 - inner.1;
        let caret16 = self.multi().utf16_at(self.multi().caret);
        let (_, cy, ch) = edit::caret_xy(&layout, caret16);
        let scroll = &mut self.multi_mut().scroll;
        if cy < *scroll {
            *scroll = cy;
        } else if cy + ch > *scroll + view_h {
            *scroll = cy + ch - view_h;
        }
        let max = (edit::content_height(&layout) - view_h).max(0.0);
        *scroll = scroll.clamp(0.0, max);
    }

    pub(in crate::island) fn drag_select(&mut self, x: i32, y: i32) {
        if !self.pointer.selecting || !self.ui.editing {
            return;
        }
        let inner = self.note_inner();
        if let Some(layout) = self.note_layout() {
            let scroll = self.multi().scroll;
            self.multi_mut().click(&layout, x as f32 - inner.0, y as f32 - inner.1 + scroll, true);
            self.note_changed();
        }
    }

    pub(in crate::island) fn on_char(&mut self, unit: u16) {
        if !self.ui.editing {
            return;
        }
        if self.ui.chat.sel.take().is_some() && unit >= 0x20 {
            self.repaint_expanded();
        }
        let ch = match unit {
            0x08 => {
                let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
                self.edit_mut().backspace(ctrl);
                return self.note_changed();
            }
            0x0D if self.ui.cmd.open
                || self.rename_active()
                || self.clip_search()
                || self.steam_search()
                || self.wave_search()
                || self.extra_field().is_some() =>
            {
                return;
            }
            0x0D if matches!(self.ui.tab, view::Tab::Agy | view::Tab::Chat)
                && unsafe { GetKeyState(VK_SHIFT.0 as i32) } >= 0 =>
            {
                return;
            }
            0x09 if self.extra_field().is_some() || self.wave_search() => return,
            0x0D => '\n',
            0x09 => '\t',
            0xD800..=0xDBFF => {
                self.pending_high_surrogate = Some(unit);
                return;
            }
            0xDC00..=0xDFFF => {
                let Some(high) = self.pending_high_surrogate.take() else { return };
                match char::decode_utf16([high, unit]).next() {
                    Some(Ok(c)) => c,
                    _ => return,
                }
            }
            u if u < 0x20 || u == 0x7F => return,
            u => match char::from_u32(u as u32) {
                Some(c) => c,
                None => return,
            },
        };
        let mut buf = [0u8; 4];
        self.edit_mut().insert(ch.encode_utf8(&mut buf));
        self.note_changed();
        self.arm_collapse_if_idle(self.settings.close_delay_ms.max(3000));
    }

    pub(in crate::island) fn on_keydown(&mut self, vk: u16) {
        if vk == VK_ESCAPE.0 && self.cancel_voice_input() {
            return;
        }
        if vk == VK_ESCAPE.0 && self.ui.home.mixer_open {
            self.close_home_mixer();
            self.repaint_expanded();
            return;
        }
        if !self.ui.editing {
            if vk == VK_ESCAPE.0 {
                self.collapse();
            }
            return;
        }
        let down = |k: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| unsafe { GetKeyState(k.0 as i32) < 0 };
        let (ctrl, shift) = (down(VK_CONTROL), down(VK_SHIFT));
        let command = self.ui.cmd.open;
        if command {
            let last = self.ui.cmd.items.len().saturating_sub(1);
            match vk {
                v if v == VK_ESCAPE.0 => return self.close_command(true),
                v if v == VK_RETURN.0 => return self.run_cmd(self.ui.cmd.sel, ctrl, shift),
                v if v == VK_UP.0 => {
                    self.ui.cmd.sel = self.ui.cmd.sel.saturating_sub(1);
                    return self.repaint_expanded();
                }
                v if v == VK_DOWN.0 => {
                    self.ui.cmd.sel = (self.ui.cmd.sel + 1).min(last);
                    return self.repaint_expanded();
                }
                _ => {}
            }
        }
        if self.rename_active() && (vk == VK_RETURN.0 || vk == VK_ESCAPE.0) {
            self.finish_rename(vk == VK_RETURN.0);
            return self.end_edit(true);
        }
        let hwnd = self.hwnd;
        let layout = self.active_layout();
        if self.ui.tab == view::Tab::Chat && self.ui.chat.sel.is_some() {
            match vk {
                v if ctrl && v == b'C' as u16 => {
                    self.copy_chat_selection();
                    return;
                }
                v if ctrl && v == b'A' as u16 => {
                    self.ui.chat.select_all();
                    return self.repaint_expanded();
                }
                v if v == VK_ESCAPE.0 => {
                    self.ui.chat.sel = None;
                    return self.repaint_expanded();
                }
                _ => {}
            }
        }
        if self.ui.tab == view::Tab::Agy && vk == VK_RETURN.0 && !shift {
            return self.send_agy();
        }
        if self.ui.tab == view::Tab::Chat && vk == VK_RETURN.0 && !shift {
            return self.send_chat();
        }
        if self.steam_search() && vk == VK_RETURN.0 {
            return self.launch_first_steam_game();
        }
        if self.wave_search() {
            let selected = self.ui.yandex.search_selected;
            let len = self.ui.yandex.search_rows().len();
            match vk {
                v if v == VK_RETURN.0 => {
                    return if let Some(index) = selected {
                        self.play_wave_result(index)
                    } else {
                        self.submit_wave_search()
                    };
                }
                v if len > 0 && (v == VK_UP.0 || v == VK_DOWN.0) => {
                    self.ui.yandex.search_selected = Some(if v == VK_UP.0 {
                        selected.map_or(len - 1, |index| index.saturating_sub(1))
                    } else {
                        selected.map_or(0, |index| (index + 1).min(len - 1))
                    });
                    return self.repaint_expanded();
                }
                _ => {}
            }
        }
        let extra = self.extra_field();
        if extra.is_some() && vk == VK_RETURN.0 {
            return self.save_city();
        }
        let single = command || self.clip_search() || self.steam_search() || self.wave_search() || extra.is_some();
        let n = self.edit_mut();
        match vk {
            v if v == VK_ESCAPE.0 => return self.end_edit(true),
            v if v == VK_LEFT.0 => n.left(shift, ctrl),
            v if v == VK_RIGHT.0 => n.right(shift, ctrl),
            v if v == VK_UP.0 => {
                if let Some(l) = &layout {
                    n.vertical(l, -1, shift)
                }
            }
            v if v == VK_DOWN.0 => {
                if let Some(l) = &layout {
                    n.vertical(l, 1, shift)
                }
            }
            v if v == VK_HOME.0 => match (&layout, ctrl) {
                (_, true) => n.caret_to_start(shift),
                (Some(l), false) => n.line_edge(l, false, shift),
                _ => {}
            },
            v if v == VK_END.0 => match (&layout, ctrl) {
                (_, true) => n.caret_to_end(shift),
                (Some(l), false) => n.line_edge(l, true, shift),
                _ => {}
            },
            v if v == VK_DELETE.0 => n.delete(ctrl),
            v if ctrl && v == b'A' as u16 => n.select_all(),
            v if ctrl && (v == b'C' as u16 || v == b'X' as u16) => {
                if let Some(text) = n.selected_text() {
                    clips::sys::write_text(hwnd, &text);
                    if v == b'X' as u16 {
                        n.delete_selection();
                    }
                }
            }
            v if ctrl && v == b'V' as u16 => {
                if let Some(text) = clips::sys::read_text(hwnd) {
                    let text = if single { text.replace(['\r', '\n'], " ") } else { text };
                    n.insert(&text);
                }
            }
            _ => return,
        }
        self.note_changed();
    }

    pub(in crate::island) fn extra_field(&self) -> Option<u8> {
        if self.ui.cmd.open || !self.ui.editing {
            return None;
        }
        match self.ui.tab {
            view::Tab::Weather => Some(10),
            _ => None,
        }
    }

    pub(in crate::island) fn edit_mut(&mut self) -> &mut edit::TextEdit {
        if self.ui.cmd.open {
            &mut self.ui.cmd.input
        } else if let Some(k) = self.extra_field() {
            let _ = k;
            &mut self.ui.city.input
        } else if self.clip_search() {
            &mut self.ui.clip.query
        } else if self.steam_search() {
            &mut self.ui.steam.query
        } else if self.wave_search() {
            &mut self.ui.yandex.search_input
        } else {
            self.multi_mut()
        }
    }

    pub(in crate::island) fn active_layout(&self) -> Option<windows::Win32::Graphics::DirectWrite::IDWriteTextLayout> {
        if let Some(k) = self.extra_field() {
            let _ = k;
            return crate::ui::gfx::make_layout(
                self.gpu.dwrite(),
                &self.ui.city.input.text(),
                100_000.0,
                13.0 * self.scale,
            )
            .ok();
        }
        if self.clip_search() {
            return crate::ui::gfx::make_layout(
                self.gpu.dwrite(),
                &self.ui.clip.query.text(),
                100_000.0,
                view::CLIP_FONT * self.scale,
            )
            .ok();
        }
        if self.steam_search() {
            return crate::ui::gfx::make_layout(
                self.gpu.dwrite(),
                &self.ui.steam.query.text(),
                100_000.0,
                view::STEAM_SEARCH_FONT * self.scale,
            )
            .ok();
        }
        if self.wave_search() {
            return crate::ui::gfx::make_layout(
                self.gpu.dwrite(),
                &self.ui.yandex.search_input.text(),
                100_000.0,
                13.0 * self.scale,
            )
            .ok();
        }
        if self.ui.cmd.open {
            crate::ui::gfx::make_layout(
                self.gpu.dwrite(),
                &self.ui.cmd.input.text(),
                100_000.0,
                view::CMD_FONT * self.scale,
            )
            .ok()
        } else {
            self.note_layout()
        }
    }

    pub(in crate::island) fn on_caret_timer(&mut self) {
        if self.ui.editing {
            self.ui.caret_on = !self.ui.caret_on;
            self.set_timer(Timer::Caret, CARET_BLINK_MS);
            self.repaint_expanded();
        }
    }
}
