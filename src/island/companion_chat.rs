use crate::island::*;
use crate::services::ai::ChatTurn;
use crate::ui::mini_chat::MiniChatWindow;

pub(in crate::island) struct CompanionChat {
    pub(in crate::island) window: MiniChatWindow,
    turns: Vec<ChatTurn>,
    pub(in crate::island) pending: bool,
    error: Option<String>,
}

impl CompanionChat {
    pub(in crate::island) fn repaint(&mut self) {
        self.window.repaint();
    }

    pub(in crate::island) fn set_capture_hidden(&self, hidden: bool) {
        self.window.set_capture_hidden(hidden);
    }

    fn refresh(&mut self, provider: &str) {
        self.window.update(&self.turns, self.error.as_deref(), self.pending, provider);
    }
}

impl App {
    pub(in crate::island) fn open_companion_chat(&mut self) {
        let Some(desktop) = &self.pet.desktop else { return };
        let anchor = desktop.center();
        self.end_edit(true);
        if self.pet.chat.is_none() {
            match MiniChatWindow::new(self.hwnd, self.dpi_scale, &self.gpu) {
                Ok(window) => {
                    self.pet.chat = Some(CompanionChat { window, turns: Vec::new(), pending: false, error: None })
                }
                Err(error) => {
                    crate::log_warn!("could not open companion chat: {error}");
                    return;
                }
            }
        }
        if let Some(chat) = &mut self.pet.chat {
            chat.window.set_capture_hidden(self.settings.capture_hidden);
            chat.window.set_voice_enabled(self.settings.pet_voice_input);
            chat.refresh(&format!(
                "{} · {}",
                self.settings.ai.provider.name(),
                self.settings.ai.model(self.settings.ai.provider)
            ));
            chat.window.update_pet(&self.ui.chat.pet, &self.settings.custom_pet, self.settings.character);
            chat.window.show(anchor, self.settings.character);
        }
    }

    pub(in crate::island) fn on_mini_chat(&mut self, action: usize, source: isize) {
        use crate::ui::mini_chat;
        if self.pet.chat.as_ref().is_none_or(|chat| chat.window.hwnd().0 as isize != source) {
            return;
        }
        match action {
            mini_chat::VOICE => return self.pet_voice_input(),
            mini_chat::VOICE_CANCEL => {
                self.cancel_voice_input();
                return;
            }
            mini_chat::PET => return self.toggle_pet_studio(),
            mini_chat::PET_APPLY => return self.pet_studio_action(view::PetStudioHit::Apply),
            mini_chat::PET_PHOTO => return self.pet_studio_action(view::PetStudioHit::Photo),
            mini_chat::PET_TEXT => return self.pet_studio_action(view::PetStudioHit::TextOnly),
            mini_chat::PET_BODY => return self.pet_studio_action(view::PetStudioHit::Body),
            mini_chat::SEND if self.ui.chat.pet.enabled => {
                let prompt = self.pet.chat.as_ref().map(|chat| chat.window.input()).unwrap_or_default();
                return self.send_pet_request(prompt, true);
            }
            _ => {}
        }
        let Some(chat) = &mut self.pet.chat else { return };
        if action == crate::ui::mini_chat::SETUP {
            return self.open_ai_settings();
        }
        if action != crate::ui::mini_chat::SEND || chat.pending {
            return;
        }
        let text = chat.window.input().trim().to_string();
        if text.is_empty() {
            return;
        }
        if !ai::has_key(self.settings.ai.provider) {
            chat.error = Some(crate::ui::i18n::tr("Connect your AI provider in AI settings.").into_owned());
            chat.refresh(&format!(
                "{} · {}",
                self.settings.ai.provider.name(),
                self.settings.ai.model(self.settings.ai.provider)
            ));
            return;
        }
        chat.turns.push(ChatTurn::text(false, text));
        if ai::ask_chat(self.hwnd, chat.turns.clone()) {
            chat.pending = true;
            chat.error = None;
            chat.window.clear_input();
        } else {
            chat.turns.pop();
            chat.error = Some(crate::ui::i18n::tr("Another request is running, try again in a moment").into_owned());
        }
        chat.refresh(&format!(
            "{} · {}",
            self.settings.ai.provider.name(),
            self.settings.ai.model(self.settings.ai.provider)
        ));
    }

    pub(in crate::island) fn finish_companion_chat(&mut self, answer: std::result::Result<String, String>) {
        let Some(chat) = &mut self.pet.chat else { return };
        chat.pending = false;
        let notification = answer.as_ref().ok().cloned();
        match answer {
            Ok(text) => {
                chat.turns.push(ChatTurn::text(true, text));
                chat.error = None;
            }
            Err(error) => {
                if let Some(turn) = chat.turns.pop_if(|turn| !turn.assistant)
                    && chat.window.input().is_empty()
                {
                    chat.window.set_input(&turn.text);
                }
                chat.error = Some(error);
            }
        }
        let overflow = chat.turns.len().saturating_sub(100);
        chat.turns.drain(..overflow);
        chat.refresh(&format!(
            "{} · {}",
            self.settings.ai.provider.name(),
            self.settings.ai.model(self.settings.ai.provider)
        ));
        if let Some(text) = notification {
            self.notify_for(Notice::Ai, '\u{E8F2}', &text, 8000);
        }
    }
}
