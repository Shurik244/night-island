use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::json;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::data::settings::{AiProvider, AiSettings};
use crate::services::assistant::TtsProvider;
use crate::services::attach::{Attachment, Kind as FileKind};
use crate::services::{http, integrations};

pub const WM_AI: u32 = WM_APP + 21;

const USER_AGENT: &str = "NightIsland/0.1";
const MAX_PROMPT_CHARS: usize = 4000;
const MAX_ANSWER_CHARS: usize = 24_000;
const MAX_OUTPUT_TOKENS: u32 = 600;
const SYSTEM_PROMPT: &str =
    "Answer briefly and plainly, in the language of the question, in at most a few sentences. No markdown.";
const CHAT_SYSTEM_PROMPT: &str = "You are a helpful assistant in a small chat panel on the user's desktop. Reply in \
    the language of the user's message. Keep answers short and clear; use plain text, short paragraphs and, when it \
    helps, simple lists or code blocks.";
const CHAT_OUTPUT_TOKENS: u32 = 1200;
const MAX_MEDIA_BYTES: usize = 12 * 1024 * 1024;
const CHAT_TURNS: usize = 24;

pub const WM_CHAT: u32 = WM_APP + 24;

pub const WM_COMMAND: u32 = WM_APP + 30;
const COMMAND_OUTPUT_TOKENS: u32 = 3000;
static REPLY_TOKENS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(COMMAND_OUTPUT_TOKENS);

pub fn set_reply_tokens(tokens: u32) {
    REPLY_TOKENS.store(tokens, std::sync::atomic::Ordering::Relaxed);
}

pub const WM_SORT: u32 = WM_APP + 31;
pub const SORT_REQUESTS_PER_DAY: u32 = 5;
const SORT_OUTPUT_TOKENS: u32 = 500;

pub type Answer = Result<String, String>;

static CONFIG: OnceLock<Mutex<AiSettings>> = OnceLock::new();
static RESULT: Mutex<Option<Answer>> = Mutex::new(None);
static CHAT_RESULT: Mutex<Option<Answer>> = Mutex::new(None);
static COMMAND_RESULT: Mutex<Option<(u64, Answer)>> = Mutex::new(None);
static SORT_RESULT: Mutex<Option<Answer>> = Mutex::new(None);
static BUSY: AtomicBool = AtomicBool::new(false);

pub fn busy() -> bool {
    BUSY.load(Ordering::SeqCst)
}

pub const WM_AI_CHECKED: u32 = WM_APP + 110;
static CHECK_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub type ConnectionCheck = (u64, String, Result<(), String>);
static CHECK_RESULT: Mutex<Option<ConnectionCheck>> = Mutex::new(None);

pub fn connection_key(settings: &AiSettings) -> String {
    format!(
        "{}|{}|{}",
        settings.provider.id(),
        settings.model(settings.provider),
        if settings.provider == AiProvider::OpenAi { settings.openai_base.as_str() } else { "" }
    )
}

pub fn check_connection(hwnd: HWND, settings: AiSettings) -> u64 {
    let serial = CHECK_SERIAL.fetch_add(1, Ordering::AcqRel) + 1;
    let signature = connection_key(&settings);
    let key = integrations::load_secret(&secret_name(settings.provider));
    let raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        let result = match key {
            Some(key) => build(
                &settings,
                &key,
                &[ChatTurn::text(false, "Reply with OK.")],
                "This is a connection test. Reply with OK.",
                512,
                true,
            )
            .ok_or_else(|| "The model or server address is invalid".to_string())
            .and_then(|call| {
                let headers: Vec<(&str, &str)> = call.headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
                let reply = http::post_json(&call.url, &headers, &call.body, Some(USER_AGENT))
                    .ok_or("No connection to the service or the request timed out")?;
                parse_reply(settings.provider, reply.status, &reply.body)
                    .map(|_| ())
                    .map_err(|why| why.replace(&key, "[hidden]"))
            }),
            None => Err("No key is saved for this service".into()),
        };
        let mut slot = CHECK_RESULT.lock().unwrap_or_else(|e| e.into_inner());
        if CHECK_SERIAL.load(Ordering::Acquire) != serial {
            return;
        }
        *slot = Some((serial, signature, result));
        drop(slot);
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_AI_CHECKED, WPARAM(0), LPARAM(0));
        }
    });
    serial
}

pub fn take_connection_check() -> Option<ConnectionCheck> {
    CHECK_RESULT.lock().ok()?.take()
}

pub fn configure(settings: &AiSettings) {
    let cell = CONFIG.get_or_init(|| Mutex::new(AiSettings::default()));
    if let Ok(mut g) = cell.lock() {
        *g = settings.clone();
    }
}

fn config() -> AiSettings {
    CONFIG.get().and_then(|c| c.lock().ok().map(|g| g.clone())).unwrap_or_default()
}

pub fn active_provider() -> AiProvider {
    config().provider
}

fn secret_name(provider: AiProvider) -> String {
    format!("ai_{}", provider.id())
}

pub fn valid_key(key: &str) -> bool {
    let key = key.trim();
    (8..=300).contains(&key.len()) && key.chars().all(|c| c.is_ascii_graphic())
}

pub fn has_key(provider: AiProvider) -> bool {
    integrations::dir().is_some_and(|d| d.join(format!("{}.bin", secret_name(provider))).exists())
}

pub fn save_key(provider: AiProvider, key: &str) -> bool {
    if !valid_key(key) {
        return false;
    }
    integrations::save_secret(&secret_name(provider), key.trim());
    has_key(provider)
}

pub fn remove_key(provider: AiProvider) {
    integrations::delete_secret(&secret_name(provider));
}

const VOICE_SECRET: &str = "ai_voice";

pub fn has_voice_key() -> bool {
    integrations::dir().is_some_and(|d| d.join(format!("{VOICE_SECRET}.bin")).exists())
}

pub fn save_voice_key(key: &str) -> bool {
    if !valid_key(key) {
        return false;
    }
    integrations::save_secret(VOICE_SECRET, key.trim());
    has_voice_key()
}

fn tts_secret(provider: TtsProvider) -> &'static str {
    match provider {
        TtsProvider::OpenAi => "ai_tts",
        TtsProvider::Eleven => "ai_tts_eleven",
        TtsProvider::Fish => "ai_tts_fish",
    }
}

pub fn has_tts_key(provider: TtsProvider) -> bool {
    integrations::dir().is_some_and(|d| d.join(format!("{}.bin", tts_secret(provider))).exists())
}

pub fn save_tts_key(provider: TtsProvider, key: &str) -> bool {
    if !valid_key(key) {
        return false;
    }
    integrations::save_secret(tts_secret(provider), key.trim());
    has_tts_key(provider)
}

pub fn remove_tts_key(provider: TtsProvider) {
    integrations::delete_secret(tts_secret(provider));
}

pub fn tts_credentials(base: &str, provider: TtsProvider) -> Option<(String, String)> {
    let base = clean_base(base)?;
    if let Some(key) = integrations::load_secret(tts_secret(provider)) {
        return Some((base, key));
    }
    if provider.needs_own_key() {
        return None;
    }
    let (shared_base, key) = speech_credentials()?;
    (shared_base == base).then_some((base, key))
}

pub fn tts_key_is_shared(base: &str, provider: TtsProvider) -> bool {
    !provider.needs_own_key()
        && !has_tts_key(provider)
        && clean_base(base).is_some_and(|b| speech_credentials().is_some_and(|(shared, _)| shared == b))
}

pub fn remove_voice_key() {
    integrations::delete_secret(VOICE_SECRET);
}

fn voice_secret(settings: &AiSettings) -> Option<String> {
    if !settings.voice_separate {
        return has_key(settings.provider).then(|| secret_name(settings.provider));
    }
    if has_voice_key() {
        return Some(VOICE_SECRET.to_string());
    }
    (voice_shares_chat_key(settings) && has_key(AiProvider::OpenAi)).then(|| secret_name(AiProvider::OpenAi))
}

pub fn voice_shares_chat_key(settings: &AiSettings) -> bool {
    settings.voice_separate
        && matches!((clean_base(&settings.voice_base), clean_base(&settings.openai_base)), (Some(a), Some(b)) if a == b)
}

pub fn speech_credentials() -> Option<(String, String)> {
    let settings = config();
    if voice_provider(&settings) != AiProvider::OpenAi {
        return None;
    }
    let base = clean_base(if settings.voice_separate { &settings.voice_base } else { &settings.openai_base })?;
    let key = integrations::load_secret(&voice_secret(&settings)?)?;
    Some((base, key))
}

pub fn voice_ready(settings: &AiSettings) -> bool {
    voice_secret(settings).is_some()
}

pub fn valid_model(model: &str, provider: AiProvider) -> bool {
    let ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    !model.is_empty()
        && model.len() <= 80
        && model.chars().all(|c| ok(c) || (provider != AiProvider::Gemini && matches!(c, ':' | '/')))
}

pub fn clean_base(base: &str) -> Option<String> {
    let base = base.trim().trim_end_matches('/');
    let rest = base.strip_prefix("https://").or_else(|| {
        let local = base.strip_prefix("http://")?;
        let host = local.split(['/', ':']).next()?;
        matches!(host, "localhost" | "127.0.0.1" | "[::1]").then_some(local)
    })?;
    let valid = !rest.is_empty() && base.len() <= 200 && base.chars().all(|c| c.is_ascii_graphic());
    valid.then(|| base.to_string())
}

#[derive(Debug, PartialEq)]
pub struct Call {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatTurn {
    pub assistant: bool,
    pub text: String,
    pub attachments: Vec<Attachment>,
}

impl ChatTurn {
    pub fn text(assistant: bool, text: impl Into<String>) -> Self {
        Self { assistant, text: text.into(), attachments: Vec::new() }
    }
}

enum Part {
    Text(String),
    Media { name: String, mime: &'static str, pdf: bool, b64: String },
}

fn media_allowed(turns: &[ChatTurn]) -> Vec<bool> {
    let mut left = MAX_MEDIA_BYTES;
    let mut allowed = vec![false; turns.len()];
    for (i, turn) in turns.iter().enumerate().rev() {
        let size: usize = turn.attachments.iter().filter(|a| a.kind != FileKind::Text).map(|a| a.data.len()).sum();
        if size <= left {
            left -= size;
            allowed[i] = true;
        }
    }
    allowed
}

fn turn_parts(turn: &ChatTurn, media_ok: bool) -> Vec<Part> {
    use base64::Engine as _;
    let mut parts = Vec::new();
    for a in &turn.attachments {
        parts.push(match a.kind {
            FileKind::Text if a.kept() => Part::Text(format!("File \"{}\":\n```\n{}\n```", a.name, a.text())),
            _ if !a.kept() => Part::Text(format!("[The file \"{}\" is no longer available]", a.name)),
            _ if !media_ok => Part::Text(format!("[The file \"{}\" was left out to keep the request small]", a.name)),
            kind => Part::Media {
                name: a.name.clone(),
                mime: a.mime,
                pdf: kind == FileKind::Pdf,
                b64: base64::engine::general_purpose::STANDARD.encode(a.data.as_slice()),
            },
        });
    }
    let text: String = turn.text.chars().take(MAX_PROMPT_CHARS).collect();
    if !text.is_empty() || parts.is_empty() {
        parts.push(Part::Text(text));
    }
    parts
}

fn content_json(provider: AiProvider, parts: Vec<Part>) -> serde_json::Value {
    if let [Part::Text(t)] = parts.as_slice() {
        return json!(t);
    }
    parts
        .into_iter()
        .map(|part| match (part, provider) {
            (Part::Text(t), _) => json!({"type": "text", "text": t}),
            (Part::Media { mime, pdf, b64, .. }, AiProvider::Claude) => json!({
                "type": if pdf { "document" } else { "image" },
                "source": {"type": "base64", "media_type": mime, "data": b64},
            }),
            (Part::Media { name, mime, pdf: true, b64 }, _) => {
                json!({"type": "file", "file": {"filename": name, "file_data": format!("data:{mime};base64,{b64}")}})
            }
            (Part::Media { mime, b64, .. }, _) => {
                json!({"type": "image_url", "image_url": {"url": format!("data:{mime};base64,{b64}")}})
            }
        })
        .collect::<Vec<_>>()
        .into()
}

fn gemini_parts(parts: Vec<Part>) -> Vec<serde_json::Value> {
    parts
        .into_iter()
        .map(|part| match part {
            Part::Text(t) => json!({"text": t}),
            Part::Media { mime, b64, .. } => json!({"inlineData": {"mimeType": mime, "data": b64}}),
        })
        .collect()
}

pub fn build_call(settings: &AiSettings, key: &str, prompt: &str) -> Option<Call> {
    build(settings, key, &[ChatTurn::text(false, prompt)], SYSTEM_PROMPT, MAX_OUTPUT_TOKENS, false)
}

pub fn build_chat_call(settings: &AiSettings, key: &str, turns: &[ChatTurn]) -> Option<Call> {
    let recent = &turns[turns.len().saturating_sub(CHAT_TURNS)..];
    let first_user = recent.iter().position(|t| !t.assistant)?;
    build(settings, key, &recent[first_user..], CHAT_SYSTEM_PROMPT, CHAT_OUTPUT_TOKENS, false)
}

fn build(
    settings: &AiSettings,
    key: &str,
    turns: &[ChatTurn],
    system: &str,
    max_tokens: u32,
    quick: bool,
) -> Option<Call> {
    let provider = settings.provider;
    let model = settings.model(provider);
    if !valid_model(model, provider) {
        return None;
    }
    let allowed = media_allowed(turns);
    let messages: Vec<(bool, Vec<Part>)> =
        turns.iter().zip(allowed).map(|(t, ok)| (t.assistant, turn_parts(t, ok))).collect();
    Some(match provider {
        AiProvider::Claude => Call {
            url: "https://api.anthropic.com/v1/messages".into(),
            headers: vec![("x-api-key", key.into()), ("anthropic-version", "2023-06-01".into())],
            body: json!({
                "model": model,
                "max_tokens": max_tokens,
                "system": system,
                "messages": messages
                    .into_iter()
                    .map(|(a, parts)| json!({
                        "role": if a { "assistant" } else { "user" },
                        "content": content_json(provider, parts),
                    }))
                    .collect::<Vec<_>>(),
            })
            .to_string(),
        },
        AiProvider::OpenAi => {
            let mut all = vec![json!({"role": "system", "content": system})];
            all.extend(messages.into_iter().map(|(a, parts)| {
                json!({"role": if a { "assistant" } else { "user" }, "content": content_json(provider, parts)})
            }));
            let base = clean_base(&settings.openai_base)?;
            let token_limit = if base == "https://api.openai.com" || base.starts_with("https://api.openai.com/") {
                "max_completion_tokens"
            } else {
                "max_tokens"
            };
            let mut body = json!({"model": model, "messages": all});
            body[token_limit] = json!(max_tokens);
            if quick && settings.openai_base.to_ascii_lowercase().contains("deepseek.com") {
                body["thinking"] = json!({"type": "disabled"});
            }
            Call {
                url: format!("{base}/chat/completions"),
                headers: vec![("Authorization", format!("Bearer {key}"))],
                body: body.to_string(),
            }
        }
        AiProvider::Gemini => Call {
            url: format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"),
            headers: vec![("x-goog-api-key", key.into())],
            body: json!({
                "systemInstruction": {"parts": [{"text": system}]},
                "contents": messages
                    .into_iter()
                    .map(|(a, parts)| json!({"role": if a { "model" } else { "user" }, "parts": gemini_parts(parts)}))
                    .collect::<Vec<_>>(),
                "generationConfig": {"maxOutputTokens": max_tokens},
            })
            .to_string(),
        },
    })
}

fn tidy(text: &str) -> String {
    let clean: String = text.chars().filter(|c| *c == '\n' || !c.is_control()).collect();
    clean.trim().chars().take(MAX_ANSWER_CHARS).collect()
}

fn error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let err = value.get("error")?;
    let text = err.get("message").and_then(|m| m.as_str()).or_else(|| err.as_str())?;
    Some(tidy(text).chars().take(140).collect())
}

pub fn parse_reply(provider: AiProvider, status: u16, body: &str) -> Answer {
    if status == 401 || status == 403 {
        return Err("The key was rejected".into());
    }
    if status == 429 {
        return Err("Too many requests, try again later".into());
    }
    if !(200..300).contains(&status) {
        let why = error_message(body);
        if why
            .as_deref()
            .is_some_and(|w| w.to_lowercase().contains("api key") && w.to_lowercase().contains("not valid"))
        {
            return Err("The key was rejected".into());
        }
        return Err(match why {
            Some(why) if !why.is_empty() => format!("Error {status}: {why}"),
            _ => format!("Error {status}"),
        });
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The reply was not understood")?;
    let text = match provider {
        AiProvider::Claude => value["content"]
            .as_array()
            .map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("")),
        AiProvider::OpenAi => value["choices"][0]["message"]["content"].as_str().map(str::to_string),
        AiProvider::Gemini => value["candidates"][0]["content"]["parts"]
            .as_array()
            .map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("")),
    };
    match text.map(|t| tidy(&t)) {
        Some(t) if !t.is_empty() => Ok(t),
        _ => {
            let finish = value["choices"][0]["finish_reason"]
                .as_str()
                .or(value["candidates"][0]["finishReason"].as_str())
                .or(value["stop_reason"].as_str())
                .unwrap_or("");
            let thought = value["choices"][0]["message"]["reasoning_content"].as_str().is_some_and(|r| !r.is_empty());
            crate::log_warn!("AI answer was empty (finish_reason={finish}, reasoning_only={thought})");
            if matches!(finish, "length" | "MAX_TOKENS" | "max_tokens") {
                Err(CUT_OFF.into())
            } else if thought {
                Err("The model spent its whole limit thinking and gave no answer".into())
            } else {
                Err("The answer was empty".into())
            }
        }
    }
}

pub fn take_result() -> Option<Answer> {
    RESULT.lock().ok()?.take()
}

pub fn ask(hwnd: HWND, prompt: &str) -> bool {
    if BUSY.swap(true, Ordering::SeqCst) {
        return false;
    }
    let (raw, prompt, settings) = (hwnd.0 as isize, prompt.to_string(), config());
    std::thread::spawn(move || {
        let answer = run(&settings, &prompt);
        if let Ok(mut r) = RESULT.lock() {
            *r = Some(answer);
        }
        BUSY.store(false, Ordering::SeqCst);
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_AI, WPARAM(0), LPARAM(0));
        }
    });
    true
}

pub const WM_TRANSCRIBED: u32 = WM_APP + 26;

static VOICE_RESULT: Mutex<Option<Answer>> = Mutex::new(None);
const MULTIPART_BOUNDARY: &str = "----NightIslandVoice7f3a91c2";

pub fn can_transcribe(settings: &AiSettings) -> bool {
    settings.voice_separate || settings.provider != AiProvider::Claude
}

fn voice_provider(settings: &AiSettings) -> AiProvider {
    if settings.voice_separate { AiProvider::OpenAi } else { settings.provider }
}

#[derive(Debug, PartialEq)]
pub struct VoiceCall {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub content_type: String,
    pub body: Vec<u8>,
}

fn whisper_model(base: &str) -> &'static str {
    if base.contains("groq.com") { "whisper-large-v3-turbo" } else { "whisper-1" }
}

fn openai_voice_call(base: &str, model: &str, key: &str, wav: &[u8]) -> VoiceCall {
    let mut body: Vec<u8> = Vec::with_capacity(wav.len() + 512);
    let field = |name: &str, value: &str| {
        format!("--{MULTIPART_BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
    };
    body.extend_from_slice(field("model", model).as_bytes());
    body.extend_from_slice(field("response_format", "json").as_bytes());
    body.extend_from_slice(
        format!(
            "--{MULTIPART_BOUNDARY}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\n\
             Content-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{MULTIPART_BOUNDARY}--\r\n").as_bytes());
    VoiceCall {
        url: format!("{base}/audio/transcriptions"),
        headers: vec![("Authorization", format!("Bearer {key}"))],
        content_type: format!("multipart/form-data; boundary={MULTIPART_BOUNDARY}"),
        body,
    }
}

pub fn build_voice_call(settings: &AiSettings, key: &str, wav: &[u8]) -> Option<VoiceCall> {
    if settings.voice_separate {
        let base = clean_base(&settings.voice_base)?;
        return valid_model(&settings.voice_model, AiProvider::OpenAi)
            .then(|| openai_voice_call(&base, &settings.voice_model, key, wav));
    }
    let provider = settings.provider;
    match provider {
        AiProvider::Claude => None,
        AiProvider::OpenAi => {
            let base = clean_base(&settings.openai_base)?;
            Some(openai_voice_call(&base, whisper_model(&base), key, wav))
        }
        AiProvider::Gemini => {
            use base64::Engine as _;
            let model = settings.model(provider);
            if !valid_model(model, provider) {
                return None;
            }
            let data = base64::engine::general_purpose::STANDARD.encode(wav);
            Some(VoiceCall {
                url: format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"),
                headers: vec![("x-goog-api-key", key.into())],
                content_type: "application/json".into(),
                body: json!({
                    "contents": [{"role": "user", "parts": [
                        {"text": "Transcribe this speech exactly as spoken, in its own language. Reply with the transcript only, no comments."},
                        {"inlineData": {"mimeType": "audio/wav", "data": data}},
                    ]}],
                    "generationConfig": {"maxOutputTokens": 2000},
                })
                .to_string()
                .into_bytes(),
            })
        }
    }
}

pub fn parse_transcript(provider: AiProvider, status: u16, body: &str) -> Answer {
    if provider == AiProvider::OpenAi && matches!(status, 404 | 405 | 501) {
        return Err("Speech endpoint unavailable; check the separate speech service".into());
    }
    if provider == AiProvider::OpenAi && (200..300).contains(&status) {
        let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "The reply was not understood")?;
        return match value["text"].as_str().map(tidy) {
            Some(t) if !t.is_empty() => Ok(t),
            _ => Err("No speech was recognized".into()),
        };
    }
    parse_reply(provider, status, body)
}

pub fn take_transcript() -> Option<Answer> {
    VOICE_RESULT.lock().ok()?.take()
}

pub fn transcribe(hwnd: HWND, wav: Vec<u8>) -> bool {
    if BUSY.swap(true, Ordering::SeqCst) {
        return false;
    }
    let (raw, settings) = (hwnd.0 as isize, config());
    std::thread::spawn(move || {
        let answer = transcribe_now(&settings, &wav);
        if let Ok(mut r) = VOICE_RESULT.lock() {
            *r = Some(answer);
        }
        BUSY.store(false, Ordering::SeqCst);
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_TRANSCRIBED, WPARAM(0), LPARAM(0));
        }
    });
    true
}

fn transcribe_now(settings: &AiSettings, wav: &[u8]) -> Answer {
    let provider = voice_provider(settings);
    if !can_transcribe(settings) {
        return Err("This service cannot turn speech into text; choose Gemini or an OpenAI-compatible one".into());
    }
    let Some(key) = voice_secret(settings).and_then(|secret| integrations::load_secret(&secret)) else {
        return Err("No key is saved for this service".into());
    };
    let Some(call) = build_voice_call(settings, &key, wav) else {
        return Err("The model or the server address in the settings is not valid".into());
    };
    let headers: Vec<(&str, &str)> = call.headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    match http::post_bytes(&call.url, &headers, &call.body, &call.content_type, Some(USER_AGENT)) {
        Some(reply) => {
            crate::log_debug!("Speech request answered with status {}", reply.status);
            parse_transcript(provider, reply.status, &reply.body)
        }
        None => {
            crate::log_warn!("Speech request could not be made");
            Err("No connection to the service or the request timed out".into())
        }
    }
}

pub fn take_sort_result() -> Option<Answer> {
    SORT_RESULT.lock().ok()?.take()
}

pub fn ask_sort(hwnd: HWND, names: &[String], groups: &[String]) -> bool {
    if BUSY.swap(true, Ordering::SeqCst) {
        return false;
    }
    let (raw, settings) = (hwnd.0 as isize, config());
    let system = crate::tools::appgroups::system_prompt(groups);
    let turn = ChatTurn::text(false, names.join("\n"));
    std::thread::spawn(move || {
        let answer = run_with(&settings, |key| build(&settings, key, &[turn], &system, SORT_OUTPUT_TOKENS, true));
        if let Ok(mut r) = SORT_RESULT.lock() {
            *r = Some(answer);
        }
        BUSY.store(false, Ordering::SeqCst);
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_SORT, WPARAM(0), LPARAM(0));
        }
    });
    true
}

pub const CUT_OFF: &str = "The model ran out of its limit before it began to answer";
const GROWN_TOKENS_CAP: u32 = 16_000;

fn run_growing(settings: &AiSettings, turns: &[ChatTurn], system: &str, tokens: u32) -> Answer {
    let first = run_with(settings, |key| build(settings, key, turns, system, tokens, true));
    if first.as_ref().err().is_some_and(|why| why == CUT_OFF) {
        let more = (tokens.saturating_mul(4)).min(GROWN_TOKENS_CAP.max(tokens));
        crate::log_info!("AI: the limit of {tokens} tokens ran out, asking again with {more}");
        return run_with(settings, |key| build(settings, key, turns, system, more, true));
    }
    first
}

pub fn complete(system: &str, user: &str, max_tokens: u32) -> Answer {
    let settings = config();
    let turn = ChatTurn::text(false, user);
    run_growing(&settings, &[turn], system, max_tokens)
}

pub fn take_command_result() -> Option<(u64, Answer)> {
    COMMAND_RESULT.lock().ok()?.take()
}

pub fn ask_agent(hwnd: HWND, turns: Vec<ChatTurn>, system: String, serial: u64) -> bool {
    let (raw, settings) = (hwnd.0 as isize, config());
    std::thread::spawn(move || {
        let answer = run_growing(&settings, &turns, &system, REPLY_TOKENS.load(std::sync::atomic::Ordering::Relaxed));
        if let Ok(mut r) = COMMAND_RESULT.lock() {
            *r = Some((serial, answer));
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_COMMAND, WPARAM(0), LPARAM(0));
        }
    });
    true
}

pub fn take_chat_result() -> Option<Answer> {
    CHAT_RESULT.lock().ok()?.take()
}

pub fn ask_chat(hwnd: HWND, turns: Vec<ChatTurn>) -> bool {
    if BUSY.swap(true, Ordering::SeqCst) {
        return false;
    }
    let (raw, settings) = (hwnd.0 as isize, config());
    std::thread::spawn(move || {
        let answer = run_with(&settings, |key| build_chat_call(&settings, key, &turns));
        if let Ok(mut r) = CHAT_RESULT.lock() {
            *r = Some(answer);
        }
        BUSY.store(false, Ordering::SeqCst);
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_CHAT, WPARAM(0), LPARAM(0));
        }
    });
    true
}

pub fn build_pet_call(
    settings: &AiSettings,
    key: &str,
    turn: &ChatTurn,
    current: &crate::ui::pet_design::PetDesign,
) -> Option<Call> {
    let context =
        format!("{}\nCurrent design: {}", crate::ui::pet_design::DESIGN_PROMPT, serde_json::to_string(current).ok()?);
    build(settings, key, std::slice::from_ref(turn), &context, 1000, false)
}

pub fn ask_pet(hwnd: HWND, turn: ChatTurn, current: &crate::ui::pet_design::PetDesign) -> bool {
    if BUSY.swap(true, Ordering::SeqCst) {
        return false;
    }
    let (raw, settings, current) = (hwnd.0 as isize, config(), current.clone());
    std::thread::spawn(move || {
        let mut answer = run_with(&settings, |key| build_pet_call(&settings, key, &turn, &current));
        if let Err(error) = &answer {
            let lower = error.to_lowercase();
            if !turn.attachments.is_empty()
                && (lower.contains("image") || lower.contains("vision") || lower.contains("multimodal"))
                && (lower.contains("support") || lower.contains("invalid") || lower.contains("accept"))
            {
                answer = Err("This model did not accept the photo. Select Text only and describe your pet, or choose a vision model.".into());
            }
        }
        if let Ok(mut result) = CHAT_RESULT.lock() {
            *result = Some(answer);
        }
        BUSY.store(false, Ordering::SeqCst);
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_CHAT, WPARAM(0), LPARAM(0));
        }
    });
    true
}

fn run(settings: &AiSettings, prompt: &str) -> Answer {
    run_with(settings, |key| build_call(settings, key, prompt))
}

fn run_with(settings: &AiSettings, make_call: impl FnOnce(&str) -> Option<Call>) -> Answer {
    let provider = settings.provider;
    let Some(key) = integrations::load_secret(&secret_name(provider)) else {
        return Err("No key is saved for this service".into());
    };
    let Some(call) = make_call(&key) else {
        return Err("The model or the server address in the settings is not valid".into());
    };
    let headers: Vec<(&str, &str)> = call.headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    match http::post_json(&call.url, &headers, &call.body, Some(USER_AGENT)) {
        Some(reply) => {
            crate::log_debug!("AI request answered with status {}", reply.status);
            parse_reply(provider, reply.status, &reply.body)
        }
        None => {
            crate::log_warn!("AI request could not be made");
            Err("No connection to the service or the request timed out".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Sends a small connection check using the configured AI account without changing settings"]
    fn configured_ai_connection_check_returns_an_answer() {
        let settings = crate::data::settings::Settings::load().ai;
        assert!(has_key(settings.provider), "Set up an AI service first");
        let serial = check_connection(HWND::default(), settings.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(75);
        loop {
            if let Some((returned, key, result)) = take_connection_check() {
                assert_eq!(returned, serial);
                assert_eq!(key, connection_key(&settings));
                result.expect("The configured model did not pass the connection check");
                return;
            }
            assert!(std::time::Instant::now() < deadline, "Connection check did not finish");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
    fn settings(provider: AiProvider) -> AiSettings {
        AiSettings { provider, ..AiSettings::default() }
    }

    #[test]
    fn an_empty_answer_that_hit_the_limit_is_told_apart_from_a_plain_empty_one() {
        let hit = r#"{"choices":[{"finish_reason":"length","message":{"content":""}}]}"#;
        assert_eq!(parse_reply(AiProvider::OpenAi, 200, hit), Err(CUT_OFF.to_string()));
        let gemini = r#"{"candidates":[{"finishReason":"MAX_TOKENS","content":{"parts":[]}}]}"#;
        assert_eq!(parse_reply(AiProvider::Gemini, 200, gemini), Err(CUT_OFF.to_string()));
        let claude = r#"{"stop_reason":"max_tokens","content":[]}"#;
        assert_eq!(parse_reply(AiProvider::Claude, 200, claude), Err(CUT_OFF.to_string()));
        let plain = r#"{"choices":[{"finish_reason":"stop","message":{"content":""}}]}"#;
        assert_eq!(parse_reply(AiProvider::OpenAi, 200, plain), Err("The answer was empty".to_string()));
        let fine = r#"{"choices":[{"finish_reason":"length","message":{"content":"Да"}}]}"#;
        assert_eq!(parse_reply(AiProvider::OpenAi, 200, fine), Ok("Да".to_string()));
    }

    #[test]
    #[ignore = "Sends a safe question to the configured AI service using the saved key"]
    fn configured_provider_answers_a_voice_question() {
        let settings = crate::data::settings::Settings::load().ai;
        configure(&settings);
        let started = std::time::Instant::now();
        assert!(ask_agent(
            HWND::default(),
            vec![ChatTurn::text(false, "Сколько будет два плюс два? Ответь кратко.")],
            crate::tools::voicecmd::system_prompt(&[], false, "", ""),
            1,
        ));
        let answer = loop {
            if let Some((serial, answer)) = take_command_result() {
                assert_eq!(serial, 1);
                break answer.expect("configured service must answer");
            }
            assert!(started.elapsed().as_secs() < 70, "voice command did not complete");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        while busy() && started.elapsed().as_secs() < 70 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!busy(), "the AI must release its busy state");
        assert!(
            matches!(crate::tools::voicecmd::parse(&answer), crate::tools::voicecmd::Command::Answer(text) if !text.is_empty())
        );
        println!(
            "Configured AI answered a safe voice question in {:.2}s and released its busy state",
            started.elapsed().as_secs_f32()
        );
    }

    #[test]
    #[ignore = "Transmits the explicitly selected recording to the configured speech service"]
    fn configured_speech_service_transcribes_a_recording() {
        let path = std::env::var_os("NI_VOICE_TEST_RECORDING").expect("Select a test WAV with NI_VOICE_TEST_RECORDING");
        let wav = std::fs::read(path).expect("test recording must load");
        assert!(wav.starts_with(b"RIFF") && wav.get(8..12) == Some(b"WAVE"));
        let settings = crate::data::settings::Settings::load().ai;
        let started = std::time::Instant::now();
        let transcript = transcribe_now(&settings, &wav).expect("configured speech service must transcribe");
        assert!(!transcript.trim().is_empty());
        println!(
            "Configured speech service recognized {} characters in {:.2}s",
            transcript.chars().count(),
            started.elapsed().as_secs_f32()
        );
    }

    #[test]
    fn unavailable_speech_endpoint_explains_what_to_configure() {
        for status in [404, 405, 501] {
            assert_eq!(
                parse_transcript(AiProvider::OpenAi, status, ""),
                Err("Speech endpoint unavailable; check the separate speech service".into())
            );
        }
        assert_eq!(
            parse_transcript(AiProvider::OpenAi, 200, r#"{"text":"Island, hello"}"#),
            Ok("Island, hello".into())
        );
        assert_ne!(parse_transcript(AiProvider::OpenAi, 401, ""), parse_transcript(AiProvider::OpenAi, 404, ""));
    }

    #[test]
    fn claude_call_has_the_key_in_a_header_only() {
        let call = build_call(&settings(AiProvider::Claude), "sk-secret-key", "Hi?").unwrap();
        assert_eq!(call.url, "https://api.anthropic.com/v1/messages");
        assert!(call.headers.contains(&("x-api-key", "sk-secret-key".to_string())));
        assert!(call.headers.contains(&("anthropic-version", "2023-06-01".to_string())));
        assert!(!call.body.contains("sk-secret-key") && !call.url.contains("sk-secret-key"));
        let body: serde_json::Value = serde_json::from_str(&call.body).unwrap();
        assert_eq!(body["messages"][0]["content"], "Hi?");
        assert_eq!(body["max_tokens"], MAX_OUTPUT_TOKENS);
    }

    #[test]
    fn openai_and_gemini_calls() {
        let mut s = settings(AiProvider::OpenAi);
        s.openai_base = "https://openrouter.ai/api/v1/".into();
        let call = build_call(&s, "k1234567", "Hi?").unwrap();
        assert_eq!(call.url, "https://openrouter.ai/api/v1/chat/completions");
        assert!(call.headers.contains(&("Authorization", "Bearer k1234567".to_string())));
        let body: serde_json::Value = serde_json::from_str(&call.body).unwrap();
        assert_eq!(body["max_tokens"], MAX_OUTPUT_TOKENS);
        assert!(body.get("max_completion_tokens").is_none());
        s.openai_base = "https://api.openai.com/v1".into();
        s.openai_model = "gpt-6-luna".into();
        let call = build_call(&s, "k1234567", "Hi?").unwrap();
        let body: serde_json::Value = serde_json::from_str(&call.body).unwrap();
        assert_eq!(body["max_completion_tokens"], MAX_OUTPUT_TOKENS);
        assert!(body.get("max_tokens").is_none());
        let g = build_call(&settings(AiProvider::Gemini), "k1234567", "Hi?").unwrap();
        assert!(g.url.starts_with("https://generativelanguage.googleapis.com/v1beta/models/"));
        assert!(g.url.ends_with(":generateContent") && !g.url.contains("k1234567"));
        assert!(g.headers.contains(&("x-goog-api-key", "k1234567".to_string())));
    }

    #[test]
    fn unsafe_models_and_addresses_are_refused() {
        let mut s = settings(AiProvider::Gemini);
        s.gemini_model = "../evil?x=1".into();
        assert!(build_call(&s, "k1234567", "x").is_none());
        s.gemini_model = "gemini/x".into();
        assert!(build_call(&s, "k1234567", "x").is_none(), "no slashes in a URL path");
        let mut o = settings(AiProvider::OpenAi);
        o.openai_model = "openai/gpt-4o".into();
        assert!(valid_model(&o.openai_model, AiProvider::OpenAi), "OpenRouter style names are fine");
        o.openai_base = "http://example.com/v1".into();
        assert!(build_call(&o, "k1234567", "x").is_none(), "plain HTTP only for this computer");
        o.openai_base = "http://localhost:1234/v1".into();
        assert!(build_call(&o, "k1234567", "x").is_some());
        assert_eq!(clean_base("https://a.b/v1/"), Some("https://a.b/v1".into()));
        assert_eq!(clean_base("ftp://a.b"), None);
        assert_eq!(clean_base("https://a b"), None);
        assert_eq!(clean_base("http://localhost.evil.com/v1"), None);
    }

    #[test]
    fn a_chat_call_carries_the_conversation_for_every_provider() {
        let turn = |assistant: bool, text: &str| ChatTurn::text(assistant, text);
        let turns = [turn(true, "stray"), turn(false, "Hi"), turn(true, "Hello"), turn(false, "And now?")];
        let claude = build_chat_call(&settings(AiProvider::Claude), "k1234567", &turns).unwrap();
        let body: serde_json::Value = serde_json::from_str(&claude.body).unwrap();
        let roles: Vec<&str> =
            body["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["user", "assistant", "user"], "starts with the user's message");
        assert_eq!(body["messages"][2]["content"], "And now?");
        let openai = build_chat_call(&settings(AiProvider::OpenAi), "k1234567", &turns).unwrap();
        let body: serde_json::Value = serde_json::from_str(&openai.body).unwrap();
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"].as_array().unwrap().len(), 4);
        let gemini = build_chat_call(&settings(AiProvider::Gemini), "k1234567", &turns).unwrap();
        let body: serde_json::Value = serde_json::from_str(&gemini.body).unwrap();
        assert_eq!(body["contents"][1]["role"], "model");
        assert!(build_chat_call(&settings(AiProvider::Claude), "k1234567", &[turn(true, "only")]).is_none());
        let many: Vec<ChatTurn> = (0..100).map(|i| turn(i % 2 == 1, "x")).collect();
        let call = build_chat_call(&settings(AiProvider::Claude), "k1234567", &many).unwrap();
        let body: serde_json::Value = serde_json::from_str(&call.body).unwrap();
        assert!(body["messages"].as_array().unwrap().len() <= CHAT_TURNS);
    }

    #[test]
    fn a_voice_call_is_built_per_provider() {
        let wav = vec![0, b'\n', b'\r', b'\n'];
        assert!(build_voice_call(&settings(AiProvider::Claude), "k1234567", &wav).is_none());
        let mut s = settings(AiProvider::OpenAi);
        s.openai_base = "https://api.groq.com/openai/v1".into();
        let call = build_voice_call(&s, "k1234567", &wav).unwrap();
        assert_eq!(call.url, "https://api.groq.com/openai/v1/audio/transcriptions");
        let boundary = call.content_type.strip_prefix("multipart/form-data; boundary=").unwrap();
        let fields = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-large-v3-turbo\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\njson\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\n\
             Content-Type: audio/wav\r\n\r\n"
        );
        let ending = format!("\r\n--{boundary}--\r\n");
        assert_eq!(call.body, [fields.as_bytes(), &wav, ending.as_bytes()].concat());
        s.openai_base = "https://api.openai.com/v1".into();
        assert!(String::from_utf8_lossy(&build_voice_call(&s, "k1234567", &wav).unwrap().body).contains("whisper-1"));
        let g = build_voice_call(&settings(AiProvider::Gemini), "k1234567", &wav).unwrap();
        let body: serde_json::Value = serde_json::from_slice(&g.body).unwrap();
        assert_eq!(body["contents"][0]["parts"][1]["inlineData"]["mimeType"], "audio/wav");
        assert_eq!(body["contents"][0]["parts"][1]["inlineData"]["data"], "AAoNCg==");
    }

    fn file_turn(text: &str, files: Vec<Attachment>) -> ChatTurn {
        ChatTurn { assistant: false, text: text.into(), attachments: files }
    }

    fn picture(size: usize) -> Attachment {
        Attachment {
            name: "p.png".into(),
            kind: FileKind::Image,
            mime: "image/png",
            data: std::sync::Arc::new(vec![7; size]),
        }
    }

    #[test]
    #[ignore = "Sends two requests to the configured AI provider using an explicitly supplied test image"]
    fn configured_provider_creates_pets_from_text_and_reference() {
        let path = std::env::var_os("NI_PET_TEST_REFERENCE")
            .expect("NI_PET_TEST_REFERENCE must name a non-private test image");
        let image = crate::services::attach::load(std::path::Path::new(&path)).expect("test image must load");
        let settings = crate::data::settings::Settings::load().ai;
        assert!(has_key(settings.provider), "No configured API key");
        let current = crate::ui::pet_design::PetDesign::default();
        for attachments in [vec![], vec![image]] {
            let with_image = !attachments.is_empty();
            let turn = ChatTurn { assistant: false, text: "Создай коричневого медведя с синим шарфом. Если есть изображение, используй медведя справа как образец.".into(), attachments };
            let answer = run_with(&settings, |key| build_pet_call(&settings, key, &turn, &current))
                .expect("pet generation request failed");
            let pet = crate::ui::pet_design::PetDesign::from_reply(&answer).expect("provider returned an invalid pet");
            assert_eq!(pet.body, crate::ui::pet_design::PetBody::Bear);
            assert_eq!(pet.accessory, crate::ui::pet_design::PetAccessory::Scarf);
            println!("Configured provider returned a valid pet; reference image: {with_image}");
        }
    }

    #[test]
    fn pet_requests_keep_the_design_context_and_photo_for_every_provider() {
        let design = crate::ui::pet_design::PetDesign { name: "Mochi".into(), ..Default::default() };
        for provider in [AiProvider::Claude, AiProvider::OpenAi, AiProvider::Gemini] {
            for with_photo in [false, true] {
                let turn = file_turn("Make the scarf blue", if with_photo { vec![picture(3)] } else { vec![] });
                let call = build_pet_call(&settings(provider), "test-key", &turn, &design).unwrap();
                let body: serde_json::Value = serde_json::from_str(&call.body).unwrap();
                let (system, photo) = match provider {
                    AiProvider::Claude => {
                        (body["system"].as_str().unwrap(), !body["messages"][0]["content"][0]["source"].is_null())
                    }
                    AiProvider::OpenAi => (
                        body["messages"][0]["content"].as_str().unwrap(),
                        !body["messages"][1]["content"][0]["image_url"].is_null(),
                    ),
                    AiProvider::Gemini => (
                        body["systemInstruction"]["parts"][0]["text"].as_str().unwrap(),
                        !body["contents"][0]["parts"][0]["inlineData"].is_null(),
                    ),
                };
                assert!(system.contains("Mochi") && system.contains("Return ONLY one JSON object"));
                assert_eq!(photo, with_photo, "{provider:?}");
                assert!(call.body.contains("Make the scarf blue"));
                assert!(!call.body.contains("test-key"));
            }
        }
    }

    #[test]
    fn attachments_go_out_in_each_services_own_form() {
        let note = Attachment::restored("n.txt", FileKind::Text, "secret plan");
        let pdf = Attachment {
            name: "d.pdf".into(),
            kind: FileKind::Pdf,
            mime: "application/pdf",
            data: std::sync::Arc::new(vec![1, 2, 3]),
        };
        let turns = [file_turn("What is this?", vec![picture(3), pdf, note])];
        let claude = build_chat_call(&settings(AiProvider::Claude), "k1234567", &turns).unwrap();
        let body: serde_json::Value = serde_json::from_str(&claude.body).unwrap();
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert_eq!(content[1]["type"], "document");
        assert!(content[2]["text"].as_str().unwrap().contains("secret plan"));
        assert_eq!(content[3]["text"], "What is this?");
        let openai = build_chat_call(&settings(AiProvider::OpenAi), "k1234567", &turns).unwrap();
        let body: serde_json::Value = serde_json::from_str(&openai.body).unwrap();
        let content = body["messages"][1]["content"].as_array().unwrap();
        assert!(content[0]["image_url"]["url"].as_str().unwrap().starts_with("data:image/png;base64,"));
        assert_eq!(content[1]["type"], "file");
        let gemini = build_chat_call(&settings(AiProvider::Gemini), "k1234567", &turns).unwrap();
        let body: serde_json::Value = serde_json::from_str(&gemini.body).unwrap();
        assert_eq!(body["contents"][0]["parts"][0]["inlineData"]["mimeType"], "image/png");
        assert_eq!(body["contents"][0]["parts"][3]["text"], "What is this?");
    }

    #[test]
    fn old_pictures_are_dropped_when_the_request_would_get_too_big() {
        let big = MAX_MEDIA_BYTES / 2 + 1;
        let turns = [file_turn("old", vec![picture(big)]), file_turn("new", vec![picture(big)])];
        assert_eq!(media_allowed(&turns), [false, true], "the newest picture wins");
        let call = build_chat_call(&settings(AiProvider::Claude), "k1234567", &turns).unwrap();
        let body: serde_json::Value = serde_json::from_str(&call.body).unwrap();
        assert!(body["messages"][0]["content"][0]["text"].as_str().unwrap().contains("left out"));
        assert_eq!(body["messages"][1]["content"][0]["type"], "image");
        let gone = Attachment::restored("x.png", FileKind::Image, "");
        let call = build_chat_call(&settings(AiProvider::Claude), "k1234567", &[file_turn("hi", vec![gone])]).unwrap();
        assert!(call.body.contains("no longer available"));
    }

    #[test]
    fn a_separate_speech_service_has_its_own_address_and_model() {
        let wav = vec![9u8, 8, 7];
        let mut s = settings(AiProvider::Claude);
        assert!(!can_transcribe(&s), "Claude cannot do speech");
        s.voice_separate = true;
        s.voice_base = "https://api.groq.com/openai/v1/".into();
        s.voice_model = "whisper-large-v3".into();
        assert!(can_transcribe(&s), "a separate service makes dictation possible with any chat service");
        let call = build_voice_call(&s, "k1234567", &wav).unwrap();
        assert_eq!(call.url, "https://api.groq.com/openai/v1/audio/transcriptions");
        let text = String::from_utf8_lossy(&call.body);
        assert!(text.contains("whisper-large-v3") && !text.contains("k1234567"));
        assert!(call.headers.contains(&("Authorization", "Bearer k1234567".to_string())));
        s.voice_base = "http://example.com/v1".into();
        assert!(build_voice_call(&s, "k1234567", &wav).is_none(), "plain HTTP only for this computer");
        s.voice_base = "https://a.b/v1".into();
        s.voice_model = "bad model?".into();
        assert!(build_voice_call(&s, "k1234567", &wav).is_none());
        assert_eq!(voice_provider(&s), AiProvider::OpenAi);
    }

    #[test]
    fn the_speech_service_at_the_chat_address_shares_its_key() {
        let mut s = settings(AiProvider::OpenAi);
        s.openai_base = "https://api.groq.com/openai/v1".into();
        s.voice_base = "https://api.groq.com/openai/v1/".into();
        assert!(!voice_shares_chat_key(&s), "only a separate speech service shares");
        s.voice_separate = true;
        assert!(voice_shares_chat_key(&s));
        s.voice_base = "https://api.openai.com/v1".into();
        assert!(!voice_shares_chat_key(&s), "another address needs its own key");
    }

    #[test]
    fn a_transcript_is_read_back() {
        assert_eq!(
            parse_transcript(AiProvider::OpenAi, 200, r#"{"text":"  hello there \n"}"#),
            Ok("hello there".into())
        );
        assert!(parse_transcript(AiProvider::OpenAi, 200, r#"{"text":"  "}"#).is_err());
        assert_eq!(parse_transcript(AiProvider::OpenAi, 401, "{}"), Err("The key was rejected".into()));
        let gemini = r#"{"candidates":[{"content":{"parts":[{"text":"Привет"}]}}]}"#;
        assert_eq!(parse_transcript(AiProvider::Gemini, 200, gemini), Ok("Привет".into()));
    }

    #[test]
    fn long_questions_are_cut() {
        let call = build_call(&settings(AiProvider::Claude), "k1234567", &"я".repeat(50_000)).unwrap();
        let body: serde_json::Value = serde_json::from_str(&call.body).unwrap();
        assert_eq!(body["messages"][0]["content"].as_str().unwrap().chars().count(), MAX_PROMPT_CHARS);
    }

    #[test]
    fn replies_are_read_per_provider() {
        let claude = r#"{"content":[{"type":"text","text":"Hello "},{"type":"text","text":"there"}]}"#;
        assert_eq!(parse_reply(AiProvider::Claude, 200, claude), Ok("Hello there".into()));
        let openai = r#"{"choices":[{"message":{"role":"assistant","content":"  Hi\u0007!  "}}]}"#;
        assert_eq!(parse_reply(AiProvider::OpenAi, 200, openai), Ok("Hi!".into()));
        let gemini = r#"{"candidates":[{"content":{"parts":[{"text":"Hey"}]}}]}"#;
        assert_eq!(parse_reply(AiProvider::Gemini, 200, gemini), Ok("Hey".into()));
        assert!(parse_reply(AiProvider::Gemini, 200, r#"{"candidates":[]}"#).is_err());
        assert!(parse_reply(AiProvider::Claude, 200, "not json").is_err());
    }

    #[test]
    fn failures_have_short_reasons_without_secrets() {
        assert_eq!(parse_reply(AiProvider::Claude, 401, "{}"), Err("The key was rejected".into()));
        assert_eq!(parse_reply(AiProvider::OpenAi, 429, ""), Err("Too many requests, try again later".into()));
        let body = r#"{"error":{"message":"model not found"}}"#;
        assert_eq!(parse_reply(AiProvider::OpenAi, 404, body), Err("Error 404: model not found".into()));
        assert_eq!(parse_reply(AiProvider::Gemini, 500, "<html>"), Err("Error 500".into()));
        let gemini_bad_key = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key."}}"#;
        assert_eq!(parse_reply(AiProvider::Gemini, 400, gemini_bad_key), Err("The key was rejected".into()));
        let long = format!(r#"{{"error":{{"message":"{}"}}}}"#, "x".repeat(1000));
        assert!(parse_reply(AiProvider::OpenAi, 400, &long).unwrap_err().chars().count() < 170);
    }

    #[test]
    #[ignore = "needs the network"]
    fn the_real_services_refuse_a_made_up_key() {
        for provider in [AiProvider::Claude, AiProvider::OpenAi, AiProvider::Gemini] {
            let call = build_call(&settings(provider), "not-a-real-key-12345", "Hi").unwrap();
            let headers: Vec<(&str, &str)> = call.headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let reply = http::post_json(&call.url, &headers, &call.body, Some(USER_AGENT)).expect("a connection");
            println!("{provider:?}: status {}", reply.status);
            let answer = parse_reply(provider, reply.status, &reply.body);
            assert!(answer.is_err(), "{provider:?} accepted a made-up key");
            assert!(matches!(reply.status, 400 | 401 | 403), "{provider:?} answered {}", reply.status);
        }
    }

    #[test]
    fn keys_must_look_like_keys() {
        assert!(valid_key("sk-ant-api03-abcdefgh"));
        assert!(!valid_key("short"));
        assert!(!valid_key("has a space in it"));
        assert!(!valid_key(&"k".repeat(400)));
        assert!(!valid_key("línea-con-acentos-1234"));
    }
}
