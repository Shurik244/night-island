use std::path::PathBuf;

use crate::data::chats;
use crate::island::*;
use crate::services::ai::ChatTurn;
use crate::services::attach;

const MAX_TURNS: usize = 200;

impl App {
    pub(in crate::island) fn refresh_chat(&mut self) {
        self.ui.chat.keyed = ai::has_key(self.settings.ai.provider);
        self.reload_chats();
        if !self.ui.chat.loaded {
            self.ui.chat.loaded = true;
            if let Some(first) = self.ui.chat.list.first().map(|c| c.id) {
                self.open_saved_chat(first);
            }
        }
    }

    fn reload_chats(&mut self) {
        self.ui.chat.list = self.store.as_ref().map(Store::chat_list).unwrap_or_default();
        let most = self.ui.chat.list.len().saturating_sub(view::chat_rows());
        self.ui.chat.list_scroll = self.ui.chat.list_scroll.min(most);
    }

    pub(in crate::island) fn open_ai_settings(&mut self) {
        if let Some(group) = view::tab_settings_group(view::Tab::Chat) {
            self.ui.sw.assistant_page = view::AssistantPage::Model;
            self.open_settings_at(group);
        }
    }

    pub(in crate::island) fn click_chat(&mut self, x: i32, y: i32) {
        let pill = self.pill_rect(true);
        match view::hit_chat(pill, self.scale, x as f32, y as f32, &self.ui) {
            Some(view::ChatHit::Pet) => return self.toggle_pet_studio(),
            Some(view::ChatHit::PetAction(action)) => return self.pet_studio_action(action),
            Some(view::ChatHit::Setup) => return self.open_ai_settings(),
            Some(view::ChatHit::New) if !self.ui.chat.pending && !self.ui.chat.pet.pending => {
                self.ui.chat.pet.enabled = false;
                self.new_chat();
                self.sync_pet_studio();
            }
            Some(view::ChatHit::New) => {}
            Some(view::ChatHit::Chats) => {
                if self.ui.chat.pending || self.ui.chat.pet.pending {
                    return;
                }
                self.ui.chat.pet.enabled = false;
                self.sync_pet_studio();
                self.ui.chat.list_open = !self.ui.chat.list_open;
                if self.ui.chat.list_open {
                    self.end_edit(false);
                    self.reload_chats();
                }
            }
            Some(view::ChatHit::Attach) => self.want_chat_dialog = true,
            Some(view::ChatHit::Row(i)) => {
                if let Some(id) = self.ui.chat.list.get(self.ui.chat.list_scroll + i).map(|c| c.id) {
                    self.open_saved_chat(id);
                }
            }
            Some(view::ChatHit::RowDelete(i)) => {
                if let Some(id) = self.ui.chat.list.get(self.ui.chat.list_scroll + i).map(|c| c.id) {
                    self.delete_saved_chat(id);
                }
            }
            Some(view::ChatHit::ChipRemove(i)) => {
                if i < self.ui.chat.files.len() {
                    self.ui.chat.files.remove(i);
                }
            }
            Some(view::ChatHit::Send) => self.send_chat(),
            Some(view::ChatHit::Input) => {
                self.begin_edit();
                if let Some(layout) = self.active_layout() {
                    let (_, inner) = view::agy_input(pill, self.scale, self.ui.chat.input_h.get());
                    let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
                    let scroll = self.ui.chat.input.scroll;
                    self.ui.chat.input.click(&layout, x as f32 - inner.0, y as f32 - inner.1 + scroll, shift);
                }
            }
            Some(view::ChatHit::History) => {
                self.begin_edit();
                self.ui.chat.sel = view::chat_text_at(&self.ui, x as f32, y as f32).map(|at| (at, at));
                self.pointer.reading = self.ui.chat.sel.is_some();
            }
            None => {
                if self.ui.editing {
                    self.end_edit(false);
                }
            }
        }
        self.repaint_expanded();
    }

    pub(in crate::island) fn drag_read(&mut self, x: i32, y: i32) {
        if !self.pointer.reading || self.ui.tab != view::Tab::Chat {
            return;
        }
        let pill = self.pill_rect(true);
        let area = view::chat_view(pill, self.scale, self.ui.chat.input_h.get(), self.ui.chat.files.len());
        let step = 16.0 * self.scale;
        if (y as f32) < area.1 {
            self.ui.chat.hist = (self.ui.chat.hist + step).min(self.ui.chat.max.get());
        } else if (y as f32) > area.3 {
            self.ui.chat.hist = (self.ui.chat.hist - step).max(0.0);
        }
        if let Some(at) = view::chat_text_at(&self.ui, x as f32, y as f32)
            && let Some((anchor, _)) = self.ui.chat.sel
        {
            self.ui.chat.sel = Some((anchor, at));
            self.repaint_expanded();
        }
    }

    pub(in crate::island) fn copy_chat_selection(&mut self) -> bool {
        let Some(text) = self.ui.chat.selected_text() else { return false };
        clips::sys::write_text(self.hwnd, &text);
        true
    }

    fn new_chat(&mut self) {
        let chat = &mut self.ui.chat;
        chat.turns.clear();
        chat.error = None;
        chat.pending = false;
        chat.chat_id = None;
        chat.title.clear();
        chat.saved = 0;
        chat.files.clear();
        chat.list_open = false;
        chat.rebuild();
    }

    fn open_saved_chat(&mut self, id: i64) {
        let Some(store) = &self.store else { return };
        let turns = store.chat_turns(id);
        let chat = &mut self.ui.chat;
        chat.title = chat.list.iter().find(|c| c.id == id).map(|c| c.title.clone()).unwrap_or_default();
        chat.saved = turns.len();
        chat.turns = turns;
        chat.chat_id = Some(id);
        chat.error = None;
        chat.pending = false;
        chat.files.clear();
        chat.list_open = false;
        chat.rebuild();
    }

    fn delete_saved_chat(&mut self, id: i64) {
        if let Some(store) = &self.store {
            store.chat_delete(id);
        }
        if self.ui.chat.chat_id == Some(id) {
            self.new_chat();
            self.ui.chat.list_open = true;
        }
        self.reload_chats();
    }

    fn save_chat(&mut self) {
        let Some(store) = &self.store else { return };
        let chat = &mut self.ui.chat;
        if chat.saved >= chat.turns.len() {
            return;
        }
        let id = match chat.chat_id {
            Some(id) => id,
            None => {
                let title = chat
                    .turns
                    .iter()
                    .find(|t| !t.assistant)
                    .map_or_else(|| "Chat".to_string(), |t| chats::title_of(&t.text, &t.attachments));
                let Some(id) = store.chat_create(&title) else { return };
                chat.chat_id = Some(id);
                chat.title = title;
                id
            }
        };
        for turn in &chat.turns[chat.saved..] {
            store.chat_add(id, turn);
        }
        chat.saved = chat.turns.len();
        self.reload_chats();
    }

    pub(in crate::island) fn attach_files(&mut self, paths: Vec<PathBuf>) {
        if self.ui.chat.pet.enabled {
            if self.ui.chat.pet.pending {
                return;
            }
            if let Some(path) = paths.first() {
                match attach::load(path) {
                    Ok(file) if file.kind == attach::Kind::Image => {
                        self.ui.chat.pet.reference = crate::platform::apps::thumbnail(&path.to_string_lossy());
                        self.ui.chat.pet.files = vec![file];
                        self.ui.chat.pet.text_only = false;
                        self.ui.chat.pet.error = None;
                    }
                    Ok(_) => self.ui.chat.pet.error = Some("Choose a PNG, JPEG or WebP photo for your pet".into()),
                    Err(error) => self.ui.chat.pet.error = Some(error.into()),
                }
            }
            return self.sync_pet_studio();
        }
        for path in paths {
            if self.ui.chat.files.len() >= attach::MAX_FILES {
                self.notify(Notice::Error, '\u{E783}', "Up to 4 files go with one message");
                break;
            }
            match attach::load(&path) {
                Ok(file) => self.ui.chat.files.push(file),
                Err(why) => self.notify(Notice::Error, '\u{E783}', why),
            }
        }
        self.repaint_expanded();
    }

    pub(in crate::island) fn finish_chat_dialog(&mut self, picked: Option<String>) {
        self.popup_open = false;
        self.pointer.tracking = false;
        if let Some(path) = picked {
            self.attach_files(vec![PathBuf::from(path)]);
        }
        self.repaint_expanded();
        if !self.cursor_inside() {
            self.set_timer(Timer::Collapse, self.settings.close_delay_ms.max(1));
        }
    }

    pub(in crate::island) fn send_chat(&mut self) {
        if self.ui.chat.pet.enabled {
            return self.send_pet_request(self.ui.chat.input.text().trim().to_string(), false);
        }
        if self.ui.chat.pending {
            return;
        }
        let text = self.ui.chat.input.text().trim().to_string();
        if text.is_empty() && self.ui.chat.files.is_empty() {
            return;
        }
        self.ui.chat.keyed = ai::has_key(self.settings.ai.provider);
        if !self.ui.chat.keyed {
            return self.repaint_expanded();
        }
        self.ui.chat.error = None;
        let attachments = std::mem::take(&mut self.ui.chat.files);
        self.ui.chat.turns.push(ChatTurn { assistant: false, text, attachments });
        if ai::ask_chat(self.hwnd, self.ui.chat.turns.clone()) {
            self.ui.chat.input.set_text("");
            self.ui.chat.pending = true;
        } else {
            if let Some(turn) = self.ui.chat.turns.pop() {
                self.ui.chat.files = turn.attachments;
            }
            self.ui.chat.error = Some("Another request is running, try again in a moment".into());
        }
        self.ui.chat.rebuild();
        self.repaint_expanded();
    }

    pub(in crate::island) fn on_chat(&mut self) {
        let Some(answer) = ai::take_chat_result() else { return };
        if self.ui.chat.pet.pending {
            return self.finish_pet_request(answer);
        }
        if self.pet.chat.as_ref().is_some_and(|chat| chat.pending) {
            return self.finish_companion_chat(answer);
        }
        if !self.ui.chat.pending {
            return;
        }
        self.ui.chat.pending = false;
        match answer {
            Ok(text) => {
                self.ui.chat.turns.push(ChatTurn::text(true, text.clone()));
                self.save_chat();
                let chat = &mut self.ui.chat;
                let overflow = chat.turns.len().saturating_sub(MAX_TURNS);
                chat.turns.drain(..overflow);
                chat.saved = chat.saved.saturating_sub(overflow);
                if self.state != State::Expanded || self.ui.tab != view::Tab::Chat {
                    self.notify_for(Notice::Ai, '\u{E8F2}', &text, 7000);
                }
            }
            Err(why) => {
                if let Some(last) = self.ui.chat.turns.pop_if(|t| !t.assistant) {
                    if self.ui.chat.input.is_empty() {
                        self.ui.chat.input.set_text(&last.text);
                    }
                    if self.ui.chat.files.is_empty() {
                        self.ui.chat.files = last.attachments;
                    }
                }
                self.ui.chat.error = Some(why);
            }
        }
        self.ui.chat.rebuild();
        if self.state == State::Expanded && self.ui.tab == view::Tab::Chat {
            self.repaint_expanded();
        }
    }
}
