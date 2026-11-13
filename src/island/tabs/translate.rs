use crate::island::*;
use crate::services::translate as service;

const AUTO_DELAY_MS: u32 = 800;
const PASTE_DELAY_MS: u32 = 60;

impl App {
    pub(in crate::island) fn enter_translate(&mut self) {
        self.ui.translate.key_saved = self.settings.translate.provider.needs_key() && self.translate_has_key();
        let seq = clips::sys::sequence_number();
        if seq == self.ui.translate.clip_seq {
            return;
        }
        self.ui.translate.clip_seq = seq;
        if let Some(text) = clips::sys::read_text(self.hwnd) {
            let text: String = text.chars().take(service::MAX_CHARS).collect();
            if !text.trim().is_empty() && text != self.ui.translate.input.text() {
                self.ui.translate.input.set_text(&text);
                self.ui.translate.result.clear();
                self.ui.translate.detected.clear();
                self.ui.translate.last_source.clear();
                self.ui.translate.status = "Pasted from the clipboard, press Translate".into();
            }
        }
    }

    fn translate_has_key(&self) -> bool {
        service::has_key(self.settings.translate.provider)
    }

    pub(in crate::island) fn schedule_translate(&mut self) {
        let pasted = std::mem::take(&mut self.ui.translate.pasted);
        if !self.settings.translate.auto {
            return;
        }
        if self.ui.translate.input.text().trim().is_empty() {
            self.kill_timer(Timer::Translate);
            self.ui.translate.result.clear();
            self.ui.translate.detected.clear();
            self.ui.translate.status.clear();
            return;
        }
        self.set_timer(Timer::Translate, if pasted { PASTE_DELAY_MS } else { AUTO_DELAY_MS });
    }

    pub(in crate::island) fn run_translate(&mut self, force: bool) {
        self.kill_timer(Timer::Translate);
        if self.state != State::Expanded || self.ui.tab != view::Tab::Translate {
            return;
        }
        let text = self.ui.translate.input.text();
        if text.trim().is_empty() {
            self.ui.translate.status = "Nothing to translate".into();
            return self.repaint_expanded();
        }
        if !force && text == self.ui.translate.last_source && !self.ui.translate.result.is_empty() {
            return;
        }
        let provider = self.settings.translate.provider;
        if provider.needs_key() && !service::has_key(provider) {
            self.ui.translate.status = "Add the API key first".into();
            return self.repaint_expanded();
        }
        if text.chars().count() > service::MAX_CHARS {
            self.ui.translate.status = "The text is too long".into();
            return self.repaint_expanded();
        }
        self.ui.translate.serial += 1;
        self.ui.translate.busy = true;
        self.ui.translate.status = "Translating…".into();
        self.ui.translate.last_source = text.clone();
        service::start(self.hwnd, self.ui.translate.serial, provider, text, self.settings.translate.target.clone());
        self.repaint_expanded();
    }

    pub(in crate::island) fn on_translate(&mut self) {
        let Some((serial, reply)) = service::take_result() else { return };
        if serial != self.ui.translate.serial {
            return;
        }
        self.ui.translate.busy = false;
        match reply {
            Ok(outcome) => {
                self.ui.translate.result = outcome.text;
                self.ui.translate.detected = outcome.detected;
                self.ui.translate.res_scroll = 0.0;
                self.ui.translate.used = Some(outcome.provider);
                self.ui.translate.status = if outcome.provider == self.settings.translate.provider {
                    String::new()
                } else {
                    format!("{} {}", crate::ui::i18n::tr("Translated by"), outcome.provider.name())
                };
            }
            Err(why) => {
                self.ui.translate.last_source.clear();
                self.ui.translate.status = why;
            }
        }
        if self.state == State::Expanded && self.ui.tab == view::Tab::Translate {
            self.repaint_expanded();
        }
    }

    fn save_translate_settings(&mut self) {
        self.ui.cfg = self.settings.clone();
        self.settings.save();
    }

    pub(in crate::island) fn click_translate(&mut self, x: i32, y: i32) {
        let pill = self.pill_rect(true);
        let (fx, fy) = (x as f32, y as f32);
        match view::hit_translate(pill, self.scale, fx, fy, &self.ui.cfg) {
            Some(view::TranslateHit::Provider) => {
                self.end_edit(true);
                self.settings.translate.provider = self.settings.translate.provider.step(true);
                self.save_translate_settings();
                self.ui.translate.key_saved = self.settings.translate.provider.needs_key() && self.translate_has_key();
                self.ui.translate.status.clear();
                self.retranslate_if_text();
            }
            Some(view::TranslateHit::Lang(i)) => {
                if let Some((code, _)) = service::LANGUAGES.get(i) {
                    self.settings.translate.target = (*code).to_string();
                    self.save_translate_settings();
                    self.retranslate_if_text();
                }
            }
            Some(view::TranslateHit::Action(i)) => self.run_translate_action(i),
            Some(view::TranslateHit::Result) => {}
            None => {
                let frame = view::translate_field(pill, self.scale).0;
                if fx >= frame.0 && fx < frame.2 && fy >= frame.1 && fy < frame.3 {
                    self.click_in_field(fx, fy);
                } else {
                    self.end_edit(true);
                }
            }
        }
    }

    fn retranslate_if_text(&mut self) {
        if self.ui.translate.input.text().trim().is_empty() {
            return self.repaint_expanded();
        }
        self.run_translate(true);
    }

    fn run_translate_action(&mut self, index: usize) {
        match index {
            0 => return self.run_translate(true),
            1 => match clips::sys::read_text(self.hwnd) {
                Some(text) if !text.trim().is_empty() => {
                    let text: String = text.chars().take(service::MAX_CHARS).collect();
                    self.ui.translate.input.set_text(&text);
                    self.ui.translate.clip_seq = clips::sys::sequence_number();
                    self.ui.translate.result.clear();
                    self.ui.translate.detected.clear();
                    self.ui.translate.status.clear();
                    if self.settings.translate.auto {
                        return self.run_translate(true);
                    }
                    self.ui.translate.status = "Pasted from the clipboard, press Translate".into();
                }
                _ => self.ui.translate.status = "The clipboard has no text".into(),
            },
            2 => {
                if self.ui.translate.result.is_empty() {
                    self.ui.translate.status = "Nothing to copy".into();
                } else {
                    clips::sys::write_text(self.hwnd, &self.ui.translate.result);
                    self.ui.translate.status = "Copied".into();
                }
            }
            3 => {
                self.settings.translate.auto = !self.settings.translate.auto;
                self.save_translate_settings();
            }
            _ => {
                let provider = self.settings.translate.provider;
                let key = clips::sys::read_text(self.hwnd).unwrap_or_default();
                if service::save_key(provider, &key) {
                    self.ui.translate.key_saved = true;
                    self.ui.translate.status = "The key is saved".into();
                } else {
                    self.ui.translate.status = "The clipboard does not hold a valid key".into();
                }
            }
        }
        self.repaint_expanded();
    }
}
