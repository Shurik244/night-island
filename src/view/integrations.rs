use crate::view::*;

#[derive(Default)]
pub struct IntegrationsUi {
    pub id: TextEdit,
    pub secret: TextEdit,
    pub spotify_id: TextEdit,
    pub ai_key: TextEdit,
    pub ai_model: TextEdit,
    pub ai_base: TextEdit,
    pub ai_key_saved: bool,
    pub voice_key: TextEdit,
    pub voice_model: TextEdit,
    pub voice_base: TextEdit,
    pub voice_key_saved: bool,
    pub tts_key: TextEdit,
    pub tts_base: TextEdit,
    pub tts_model: TextEdit,
    pub tts_voice: TextEdit,
    pub tts_key_saved: bool,
    pub tts_key_shared: bool,
    pub voice_key_shared: bool,
    pub wake: TextEdit,
    pub group_name: TextEdit,
    pub search: TextEdit,
}

pub fn discord_fields_ok(ui: &UiState) -> bool {
    crate::services::discord::credentials_look_valid(&ui.int.id.text(), &ui.int.secret.text())
}
