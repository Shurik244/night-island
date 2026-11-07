use crate::island::*;

impl App {
    pub(in crate::island) fn refresh_cmd(&mut self) {
        let text = self.ui.cmd.input.text();
        if cmd::is_file_search(&text) && self.ui.cmd.files_shown {
            cmd::start_file_search(self.hwnd, &text);
        } else {
            self.ui.cmd.files_shown = false;
            if !cmd::is_file_search(&text) {
                everything::cancel();
            }
            self.ui.cmd.items = cmd::search(self.hwnd, &text);
        }
        self.ui.cmd.ai_pending = false;
        self.ui.cmd.sel = 0;
        self.ui.armed = None;
    }

    pub(in crate::island) fn toggle_command(&mut self) {
        if self.ui.cmd.open {
            self.close_command(true);
        } else {
            self.open_command();
        }
    }

    pub(in crate::island) fn open_command(&mut self) {
        self.kill_timer(Timer::Collapse);
        self.ui.cmd.open = true;
        self.ui.cmd.input.set_text("");
        self.refresh_cmd();
        self.expand();
        self.begin_edit();
        self.repaint_expanded();
    }

    pub(in crate::island) fn close_command(&mut self, restore_focus: bool) {
        self.ui.cmd.open = false;
        self.ui.armed = None;
        self.end_edit(restore_focus);
        self.collapse();
    }

    pub(in crate::island) fn run_cmd(&mut self, index: usize, ctrl: bool, shift: bool) {
        let Some(item) = self.ui.cmd.items.get(index) else { return };
        match &item.action {
            cmd::Action::Launch { launch, path } => {
                let (launch, path) = (launch.clone(), path.clone());
                if ctrl && !path.as_os_str().is_empty() {
                    cmd::reveal(&path);
                } else {
                    let elevate = shift && !path.as_os_str().is_empty();
                    cmd::shell_open(&launch, elevate.then_some("runas"));
                }
                self.close_command(false);
            }
            cmd::Action::Copy(text) => {
                let text = text.clone();
                clips::sys::write_text(self.hwnd, &text);
                self.close_command(true);
            }
            cmd::Action::Web(url) => {
                let url = url.clone();
                cmd::shell_open(&url, None);
                self.close_command(false);
            }
            cmd::Action::Ask(question) => {
                let question = question.clone();
                if ai::ask(self.hwnd, &question) {
                    self.ui.cmd.items = vec![cmd::ai_thinking_item()];
                    self.ui.cmd.ai_pending = true;
                    self.repaint_expanded();
                }
            }
            cmd::Action::OpenAiSettings => {
                self.close_command(false);
                self.open_ai_settings();
            }
            cmd::Action::System(ctl) => {
                let ctl = *ctl;
                if view::needs_confirm(ctl) && self.ui.armed != Some(index) {
                    self.ui.armed = Some(index);
                    return self.repaint_expanded();
                }
                self.close_command(true);
                self.run_ctl(ctl);
            }
        }
    }

    pub(in crate::island) fn on_files(&mut self) {
        let Some(answer) = everything::take_result() else { return };
        if !self.ui.cmd.open || !cmd::is_file_search(&self.ui.cmd.input.text()) {
            return;
        }
        self.ui.cmd.items = cmd::file_items(answer);
        self.ui.cmd.files_shown = true;
        self.ui.cmd.sel = 0;
        self.ui.armed = None;
        self.repaint_expanded();
    }

    pub(in crate::island) fn on_ai(&mut self) {
        let Some(answer) = ai::take_result() else { return };
        if self.ui.cmd.open && self.ui.cmd.ai_pending {
            self.ui.cmd.ai_pending = false;
            self.ui.cmd.items = cmd::ai_answer_items(answer);
            self.ui.cmd.sel = 0;
            return self.repaint_expanded();
        }
        let (glyph, text) = match answer {
            Ok(text) => ('\u{E99A}', text),
            Err(why) => ('\u{E783}', why),
        };
        self.notify_for(Notice::Ai, glyph, &text, 7000);
    }

    pub(in crate::island) fn click_cmd(&mut self, x: i32, y: i32) {
        let pill = self.pill_rect(true);
        if let Some(b) = view::hit_cmd_button(pill, self.scale, x as f32, y as f32, &self.ui) {
            return self.run_cmd(self.ui.cmd.sel, b == 0, b == 1);
        }
        if let Some(i) = view::hit_cmd(pill, self.scale, x as f32, y as f32, self.ui.cmd.items.len()) {
            self.ui.cmd.sel = i;
            return self.run_cmd(i, false, false);
        }
        let (frame, inner) = view::cmd_field(pill, self.scale);
        let (fx, fy) = (x as f32, y as f32);
        if fx >= frame.0
            && fx < frame.2
            && fy >= frame.1
            && fy < frame.3
            && let Some(layout) = self.active_layout()
        {
            let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
            self.ui.cmd.input.click(&layout, fx - inner.0, 4.0, shift);
            self.ui.caret_on = true;
            self.repaint_expanded();
        }
    }
}
