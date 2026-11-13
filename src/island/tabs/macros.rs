use crate::data::keys::{ALT, CTRL, Combo, SHIFT, WIN};
use crate::island::*;
use crate::platform::input_record;
use crate::services::macros as store;
use crate::tools::macro_script::{self, Method};

impl App {
    pub(in crate::island) fn reload_macros(&mut self, keep: Option<&str>) {
        let entries = store::list();
        let failed = store::register_hotkeys(self.hwnd, &entries);
        if !failed.is_empty() {
            crate::log_warn!("macro hotkeys taken by another program: {}", failed.join(", "));
        }
        let m = &mut self.ui.macros;
        let previous = keep.map(str::to_string).or_else(|| m.sel.and_then(|i| m.rows.get(i)).map(|r| r.stem.clone()));
        m.rows = entries
            .iter()
            .map(|e| view::MacroRow {
                stem: e.stem.clone(),
                title: e.title(),
                hotkey: e.parsed.as_ref().ok().and_then(|p| p.hotkey).map(|c| c.text()).unwrap_or_default(),
                method: e.parsed.as_ref().map_or("Scan codes", |p| p.method.name()),
                broken: e.parsed.is_err(),
            })
            .collect();
        m.sel = previous.and_then(|stem| m.rows.iter().position(|r| r.stem == stem));
        m.scroll = m.scroll.min(m.rows.len().saturating_sub(1));
        m.running = store::running();
        if !failed.is_empty() {
            m.status =
                format!("{}: {}", crate::ui::i18n::tr("The hotkey is taken by another program"), failed.join(", "));
        }
    }

    fn selected_stem(&self) -> Option<String> {
        let m = &self.ui.macros;
        m.sel.and_then(|i| m.rows.get(i)).map(|r| r.stem.clone())
    }

    fn load_macro_editor(&mut self) {
        let text = self.selected_stem().and_then(|s| store::load(&s)).map(|e| e.text).unwrap_or_default();
        self.ui.macros.editor.set_text(&text);
        self.ui.macros.editor.scroll = 0.0;
        self.ui.macros.dirty = false;
        self.ui.macros.armed_delete = false;
        self.ui.macros.status.clear();
    }

    pub(in crate::island) fn enter_macros(&mut self) {
        self.reload_macros(None);
        if self.ui.macros.sel.is_none() && !self.ui.macros.rows.is_empty() {
            self.ui.macros.sel = Some(0);
        }
        self.load_macro_editor();
    }

    pub(in crate::island) fn macro_editor_changed(&mut self) {
        let m = &mut self.ui.macros;
        m.dirty = true;
        m.armed_delete = false;
        m.status = match macro_script::parse(&m.editor.text()) {
            Ok(_) => String::new(),
            Err(why) => why,
        };
    }

    pub(in crate::island) fn save_macro_if_dirty(&mut self) {
        if self.ui.macros.dirty {
            self.save_macro_editor();
        }
    }

    fn save_macro_editor(&mut self) -> bool {
        let Some(stem) = self.selected_stem() else { return false };
        let text = self.ui.macros.editor.text();
        if !store::save_text(&stem, &text) {
            self.ui.macros.status = "Could not save the macro".into();
            return false;
        }
        self.ui.macros.dirty = false;
        let status = std::mem::take(&mut self.ui.macros.status);
        self.reload_macros(Some(&stem));
        if self.ui.macros.status.is_empty() {
            self.ui.macros.status = status;
        }
        true
    }

    fn select_macro(&mut self, index: usize) {
        self.save_macro_if_dirty();
        self.ui.macros.sel = Some(index);
        self.load_macro_editor();
        self.end_edit(true);
    }

    pub(in crate::island) fn click_macros(&mut self, x: i32, y: i32) {
        let pill = self.pill_rect(true);
        let (fx, fy) = (x as f32, y as f32);
        if self.ui.macros.capture {
            self.ui.macros.capture = false;
        }
        match view::hit_macros(pill, self.scale, fx, fy, &self.ui.macros) {
            Some(view::MacrosHit::Row(i)) => self.select_macro(i),
            Some(view::MacrosHit::Action(i)) => self.macro_action(i),
            None => {
                let frame = view::macro_field(pill, self.scale).0;
                if self.ui.macros.sel.is_some() && fx >= frame.0 && fx < frame.2 && fy >= frame.1 && fy < frame.3 {
                    self.click_in_field(fx, fy);
                } else {
                    self.end_edit(true);
                }
            }
        }
        self.repaint_expanded();
    }

    fn macro_action(&mut self, index: usize) {
        if index != view::ACT_DELETE {
            self.ui.macros.armed_delete = false;
        }
        match index {
            view::ACT_RUN => self.run_selected_macro(),
            view::ACT_SAVE => {
                if self.selected_stem().is_some() && self.save_macro_editor() {
                    self.ui.macros.status = "Saved".into();
                }
            }
            view::ACT_RECORD => {
                if self.ui.macros.recording {
                    self.finish_macro_recording(true);
                } else {
                    self.end_edit(true);
                    if input_record::start(self.hwnd) {
                        self.ui.macros.recording = true;
                        self.ui.macros.status =
                            "Recording: everything you type and click is saved. Ctrl+Alt+F12 or Stop ends it".into();
                    } else {
                        self.ui.macros.status = "Could not start recording".into();
                    }
                }
            }
            view::ACT_HOTKEY => {
                if self.selected_stem().is_some() {
                    self.ui.macros.capture = true;
                    self.ui.macros.status = "Press the new combination (Esc cancels, Delete clears)".into();
                    self.begin_edit();
                }
            }
            view::ACT_METHOD => {
                if self.selected_stem().is_some() {
                    let current = macro_script::parse(&self.ui.macros.editor.text()).map_or(Method::Scan, |m| m.method);
                    let next = current.step();
                    let text = macro_script::set_header(&self.ui.macros.editor.text(), "method", next.id());
                    self.ui.macros.editor.set_text(&text);
                    self.save_macro_editor();
                    self.ui.macros.status =
                        format!("{}: {}", crate::ui::i18n::tr("Method"), crate::ui::i18n::tr(next.name()));
                }
            }
            view::ACT_NEW => {
                self.save_macro_if_dirty();
                match store::create("New macro", macro_script::TEMPLATE) {
                    Some(stem) => {
                        self.reload_macros(Some(&stem));
                        self.load_macro_editor();
                        self.end_edit(true);
                    }
                    None => self.ui.macros.status = "Could not create the macro".into(),
                }
            }
            view::ACT_DELETE => {
                let Some(stem) = self.selected_stem() else { return };
                if self.ui.macros.armed_delete {
                    store::delete(&stem);
                    self.ui.macros.sel = None;
                    self.ui.macros.dirty = false;
                    self.reload_macros(None);
                    self.ui.macros.sel = (!self.ui.macros.rows.is_empty()).then_some(0);
                    self.load_macro_editor();
                    self.ui.macros.status = "The macro file was deleted".into();
                } else {
                    self.ui.macros.armed_delete = true;
                    self.ui.macros.status = "Press Delete again to remove the file".into();
                }
            }
            view::ACT_FOLDER => store::open_folder(),
            _ => {}
        }
    }

    fn run_selected_macro(&mut self) {
        let Some(stem) = self.selected_stem() else { return };
        if store::running().as_deref() == Some(stem.as_str()) {
            store::stop();
            return;
        }
        self.end_edit(true);
        let program = match macro_script::parse(&self.ui.macros.editor.text()) {
            Ok(program) => program,
            Err(why) => {
                self.ui.macros.status = why;
                return;
            }
        };
        self.save_macro_if_dirty();
        if store::start(self.hwnd, &stem, program, true) {
            self.ui.macros.running = Some(stem);
            self.ui.macros.status = "Running…".into();
        } else {
            self.ui.macros.status = "Another macro is running".into();
        }
    }

    pub(in crate::island) fn on_macro_hotkey(&mut self, id: i32) {
        let Some(stem) = store::stem_for_hotkey(id) else { return };
        if store::running().as_deref() == Some(stem.as_str()) {
            return store::stop();
        }
        let Some(program) = store::load(&stem).and_then(|e| e.parsed.ok()) else { return };
        if store::start(self.hwnd, &stem, program, false) {
            self.ui.macros.running = Some(stem);
            if self.ui.tab == view::Tab::Macros && self.state == State::Expanded {
                self.repaint_expanded();
            }
        }
    }

    pub(in crate::island) fn on_macro_done(&mut self) {
        let Some((stem, result)) = store::take_status() else { return };
        self.ui.macros.running = store::running();
        let on_tab = self.ui.tab == view::Tab::Macros;
        match result {
            Ok(()) if on_tab => self.ui.macros.status = "The macro finished".into(),
            Ok(()) => {}
            Err(why) if on_tab => self.ui.macros.status = why,
            Err(why) => self.notify_for(Notice::Error, '\u{E783}', &format!("{stem}: {why}"), 5000),
        }
        if self.state == State::Expanded && self.ui.tab == view::Tab::Macros {
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn on_record_stop(&mut self) {
        self.finish_macro_recording(false);
    }

    fn finish_macro_recording(&mut self, trim_click: bool) {
        if !self.ui.macros.recording {
            return;
        }
        self.ui.macros.recording = false;
        let steps = input_record::stop(trim_click);
        if steps.is_empty() {
            self.ui.macros.status = "Nothing was recorded".into();
        } else {
            let program = macro_script::Macro { name: "Recording".into(), steps, ..Default::default() };
            match store::create("Recording", &macro_script::to_text(&program)) {
                Some(stem) => {
                    self.save_macro_if_dirty();
                    self.reload_macros(Some(&stem));
                    self.load_macro_editor();
                    self.ui.macros.status = "Recorded. Check the steps, then Run".into();
                }
                None => self.ui.macros.status = "Could not save the recording".into(),
            }
        }
        if self.state == State::Expanded {
            if self.ui.tab != view::Tab::Macros {
                self.select_tab(view::Tab::Macros);
            }
            self.repaint_expanded();
        } else {
            self.open_tab(view::Tab::Macros);
        }
    }

    pub(in crate::island) fn capture_macro_hotkey(&mut self, vk: u16) {
        let down = |k: u16| unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(k as i32) } < 0;
        if matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5) {
            return;
        }
        self.ui.macros.capture = false;
        if vk == VK_ESCAPE.0 {
            self.ui.macros.status.clear();
            self.end_edit(true);
            return self.repaint_expanded();
        }
        let mut mods = 0;
        for (key, bit) in [(0x11, CTRL), (0x12, ALT), (0x10, SHIFT), (0x5B, WIN), (0x5C, WIN)] {
            if down(key) {
                mods |= bit;
            }
        }
        let value = if mods == 0 && matches!(vk, 0x08 | 0x2E) {
            String::new()
        } else {
            let combo = Combo::new(mods, vk as u32);
            if !combo.valid() {
                self.ui.macros.status = "A hotkey needs a modifier and a regular key".into();
                return self.repaint_expanded();
            }
            combo.text()
        };
        let text = macro_script::set_header(&self.ui.macros.editor.text(), "hotkey", &value);
        self.ui.macros.editor.set_text(&text);
        self.end_edit(true);
        self.save_macro_editor();
        self.ui.macros.status = if value.is_empty() {
            "The hotkey is cleared".into()
        } else {
            format!("{}: {value}", crate::ui::i18n::tr("Hotkey"))
        };
        self.repaint_expanded();
    }
}
