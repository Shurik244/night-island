use crate::island::*;
use crate::services::ai::ChatTurn;
use crate::ui::pet_design::PetDesign;

impl App {
    pub(in crate::island) fn sync_pet_studio(&mut self) {
        self.ui.chat.pet.source = PetDesign::for_character(self.settings.character, &self.settings.custom_pet);
        self.ui.chat.pet.performance = self.settings.pet_performance;
        if self.ui.chat.pet.enabled {
            self.ui.chat.pet.preview_started.get_or_insert_with(Instant::now);
        }
        if self.state == State::Expanded
            && self.ui.tab == view::Tab::Chat
            && self.ui.chat.pet.preview_interval().is_some()
        {
            self.set_timer(Timer::UiAnim, 1);
        }
        if let Some(chat) = &mut self.pet.chat {
            chat.window.update_pet(&self.ui.chat.pet, &self.settings.custom_pet, self.settings.character);
        }
        self.repaint_expanded();
    }

    pub(in crate::island) fn toggle_pet_studio(&mut self) {
        if self.ui.chat.pending || self.ui.chat.pet.pending || self.pet.chat.as_ref().is_some_and(|chat| chat.pending) {
            return;
        }
        self.ui.chat.pet.enabled = !self.ui.chat.pet.enabled;
        self.ui.chat.list_open = false;
        self.ui.chat.sel = None;
        self.pointer.reading = false;
        if self.ui.chat.pet.draft.is_none() && self.settings.character == Character::Custom {
            self.ui.chat.pet.draft = Some(self.settings.custom_pet.clone());
            self.ui.chat.pet.applied = true;
        }
        self.sync_pet_studio();
    }

    pub(in crate::island) fn pet_studio_action(&mut self, action: view::PetStudioHit) {
        if self.ui.chat.pet.pending {
            return;
        }
        match action {
            view::PetStudioHit::Body => {
                let studio = &mut self.ui.chat.pet;
                let design = studio.draft.get_or_insert_with(|| studio.source.clone());
                design.body = design.body.next();
                studio.applied = false;
                studio.error = None;
            }
            view::PetStudioHit::Apply => {
                let Some(design) = self.ui.chat.pet.draft.clone() else { return };
                self.settings.custom_pet = design;
                self.settings.character = Character::Custom;
                self.ui.cfg = self.settings.clone();
                self.settings.save();
                self.apply_character();
                self.react(Reaction::Done, 1000);
                self.ui.chat.pet.applied = true;
                self.render_settings_win();
            }
            view::PetStudioHit::Photo => {
                self.want_chat_dialog = true;
            }
            view::PetStudioHit::TextOnly => {
                self.ui.chat.pet.text_only = !self.ui.chat.pet.text_only;
                self.ui.chat.pet.error = None;
            }
        }
        self.sync_pet_studio();
    }

    pub(in crate::island) fn send_pet_request(&mut self, prompt: String, from_companion: bool) {
        let studio = &mut self.ui.chat.pet;
        if studio.pending {
            return;
        }
        let attachments = if studio.text_only { Vec::new() } else { studio.files.clone() };
        if prompt.trim().is_empty() && attachments.is_empty() {
            return;
        }
        if !ai::has_key(self.settings.ai.provider) {
            studio.error = Some("Connect your AI provider in AI settings.".into());
        } else {
            studio.last_prompt = prompt.clone();
            studio.from_companion = from_companion;
            let turn = ChatTurn {
                assistant: false,
                text: if prompt.trim().is_empty() {
                    crate::ui::i18n::tr("Create a pet from this photo").into_owned()
                } else {
                    prompt
                },
                attachments,
            };
            let existing = PetDesign::for_character(self.settings.character, &self.settings.custom_pet);
            let current = studio.draft.as_ref().unwrap_or(&existing);
            if ai::ask_pet(self.hwnd, turn, current) {
                studio.pending = true;
                studio.error = None;
                if from_companion {
                    if let Some(chat) = &mut self.pet.chat {
                        chat.window.clear_input();
                    }
                } else {
                    self.ui.chat.input.set_text("");
                }
            } else {
                studio.error = Some("Another request is running, try again in a moment".into());
            }
        }
        self.sync_pet_studio();
    }

    pub(in crate::island) fn finish_pet_request(&mut self, answer: ai::Answer) {
        let studio = &mut self.ui.chat.pet;
        studio.pending = false;
        match answer.and_then(|reply| PetDesign::from_reply(&reply).map_err(String::from)) {
            Ok(design) => {
                studio.draft = Some(design);
                studio.error = None;
                studio.applied = false;
            }
            Err(error) => studio.error = Some(error),
        }
        if studio.error.is_some() {
            if studio.from_companion {
                if let Some(chat) = &mut self.pet.chat
                    && chat.window.input().is_empty()
                {
                    chat.window.set_input(&studio.last_prompt);
                }
            } else if self.ui.chat.input.is_empty() {
                self.ui.chat.input.set_text(&studio.last_prompt);
            }
        }
        self.sync_pet_studio();
    }
}
