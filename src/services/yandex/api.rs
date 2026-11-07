use md5::{Digest, Md5};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::Uri;
use windows::Storage::Streams::UnicodeEncoding;
use windows::Web::Http::Filters::HttpBaseProtocolFilter;
use windows::Web::Http::{HttpClient, HttpMethod, HttpRequestMessage, HttpStringContent};
use windows::core::HSTRING;

use super::{Group, Track};
use crate::services::http::pct;

pub const CLIENT_ID: &str = "23cabbbdc6cd418abb4b39c32c41195d";
pub const CLIENT_SECRET: &str = "53bc75238f0c4d08a118e51fe9203300";
pub const NETWORK: &str = "Could not connect to Yandex Music. Try again.";
pub const SIGN_IN: &str = "Sign in to Yandex Music again";
pub type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Default)]
pub struct Api {
    pub token: String,
    #[cfg(test)]
    pub base: Option<String>,
}

impl Api {
    fn endpoint(&self, path: &str, body: Option<&str>, mime: &str) -> Result<Value> {
        let base = "https://api.music.yandex.net";
        #[cfg(test)]
        let base = self.base.as_deref().unwrap_or(base);
        let url = format!("{base}{path}");
        let (status, text) = request(&url, Some(&self.token), body, mime)?;
        if status == 401 {
            return Err(SIGN_IN);
        }
        if status == 403 {
            return Err("Yandex Music requires an active subscription and an available region");
        }
        if status == 429 {
            return Err("Yandex Music is busy. Wait a moment and try again.");
        }
        if !(200..300).contains(&status) {
            return Err("Yandex Music could not complete the request. Try again.");
        }
        if text.trim().is_empty() && body.is_some() {
            return Ok(Value::Null);
        }
        let value: Value = serde_json::from_str(&text).map_err(|_| NETWORK)?;
        if value.get("error").is_some() {
            return Err("Yandex Music could not complete the request. Try again.");
        }
        Ok(value.get("result").cloned().unwrap_or(value))
    }

    pub fn call(&self, path: &str, body: Option<Value>) -> Result<Value> {
        let payload = body.map(|v| v.to_string());
        self.endpoint(path, payload.as_deref(), "application/json")
    }

    pub fn account(&self) -> Result<(String, u64)> {
        let v = self.call("/account/status", None)?;
        let uid = numeric_id(&v["account"]["uid"]).and_then(|s| s.parse().ok()).ok_or(SIGN_IN)?;
        let name = v
            .pointer("/account/displayName")
            .or_else(|| v.pointer("/account/login"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(80).collect())
            .ok_or(SIGN_IN)?;
        Ok((name, uid))
    }

    pub fn groups(&self) -> Result<Vec<Group>> {
        let v = self.call("/rotor/wave/settings?seeds=user%3Aonyourwave", None)?;
        Ok(parse_groups(&v))
    }

    pub fn station(&self, query: &str, artist: bool) -> Result<(String, String)> {
        let (kind, list, label) = if artist { ("artist", "artists", "name") } else { ("track", "tracks", "title") };
        let result = self.call(&format!("/search?text={}&type={kind}&nocorrect=False&page=0", pct(query)), None)?;
        let item = if result["best"]["type"] == kind {
            &result["best"]["result"]
        } else {
            result[list]["results"]
                .as_array()
                .and_then(|items| items.first())
                .ok_or("No matching Yandex Music result")?
        };
        let id = match &item["id"] {
            Value::String(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => s.clone(),
            Value::Number(n) if n.is_u64() => n.to_string(),
            _ => return Err("No matching Yandex Music result"),
        };
        let name = item[label].as_str().ok_or("No matching Yandex Music result")?;
        Ok((format!("{kind}:{id}"), name.chars().take(160).collect()))
    }

    pub fn search_tracks(&self, query: &str) -> Result<Vec<Track>> {
        let result = self.call(&format!("/search?text={}&type=track&nocorrect=False&page=0", pct(query)), None)?;
        let mut seen = std::collections::HashSet::new();
        let best = (result["best"]["type"] == "track").then(|| &result["best"]["result"]);
        Ok(best
            .into_iter()
            .chain(result["tracks"]["results"].as_array().into_iter().flatten())
            .filter_map(|item| parse_track(item, ""))
            .filter(|track| seen.insert(super::track_key(&track.id).to_owned()))
            .take(20)
            .collect())
    }

    pub fn library_ids(&self, uid: u64, disliked: bool) -> Result<Vec<String>> {
        let (list, revision) =
            if disliked { ("dislikes", "if_modified_since_revision") } else { ("likes", "if-modified-since-revision") };
        let value = self.call(&format!("/users/{uid}/{list}/tracks?{revision}=0"), None)?;
        let tracks = value.pointer("/library/tracks").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
        let mut seen = std::collections::HashSet::new();
        Ok(tracks
            .iter()
            .filter_map(|t| {
                let id = numeric_id(&t["id"])?;
                if !seen.insert(id.clone()) {
                    return None;
                }
                Some(with_album(id, &t["albumId"]))
            })
            .collect())
    }

    pub fn library_tracks(&self, ids: &[String]) -> Result<Vec<Track>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let body = format!("track-ids={}", pct(&ids.join(",")));
        let value = self.endpoint("/tracks", Some(&body), "application/x-www-form-urlencoded")?;
        let mut tracks = value.as_array().ok_or(NETWORK)?.iter().filter_map(|t| parse_track(t, "")).collect::<Vec<_>>();
        tracks.sort_by_key(|track| ids.iter().position(|id| super::track_key(id) == super::track_key(&track.id)));
        Ok(tracks)
    }

    pub fn rate(&self, uid: u64, track: &Track, kind: &str) -> Result<()> {
        let (list, action) = match kind {
            "like" => ("likes", "add-multiple"),
            "unlike" => ("likes", "remove"),
            "dislike" => ("dislikes", "add-multiple"),
            "undislike" => ("dislikes", "remove"),
            _ => return Err("Unknown My Wave feedback"),
        };
        let path = format!("/users/{uid}/{list}/tracks/{action}");
        let form = format!("track-ids={}", pct(&track.id));
        self.endpoint(&path, Some(&form), "application/x-www-form-urlencoded")?;
        if kind != "undislike" {
            self.feedback(&track.session, &track.batch, kind, Some(&track.id), 0.0);
        }
        Ok(())
    }

    pub fn stream(&self, id: &str) -> Result<String> {
        let v = self.call(&format!("/tracks/{}/download-info", pct(id)), None)?;
        let best = v
            .as_array()
            .into_iter()
            .flatten()
            .filter(|i| i["codec"] == "mp3" && i["preview"].as_bool() != Some(true))
            .max_by_key(|i| i["bitrateInKbps"].as_u64().unwrap_or(0))
            .and_then(|i| i["downloadInfoUrl"].as_str())
            .ok_or("This track is unavailable with your subscription or in your region")?;
        let best = if best.starts_with("http://") { best.replacen("http://", "https://", 1) } else { best.into() };
        if !trusted_url(&best) {
            return Err(NETWORK);
        }
        let (status, xml) = request(&best, None, None, "")?;
        if status != 200 {
            return Err(NETWORK);
        }
        direct_url(&xml)
    }

    pub fn feedback(&self, session: &str, batch: &str, kind: &str, track: Option<&str>, seconds: f64) {
        if session.is_empty() {
            return;
        }
        let mut event =
            json!({"type":kind, "timestamp":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true)});
        if let Some(track) = track {
            event["trackId"] = json!(track);
            if kind == "skip" || kind == "trackFinished" {
                event["totalPlayedSeconds"] = json!(seconds);
            }
        }
        let mut body = json!({"event":event, "batchId":batch});
        if kind == "radioStarted" {
            body["from"] = json!("desktop_win-home-play");
        }
        let tx = FEEDBACK.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::sync_channel::<Feedback>(64);
            std::thread::spawn(move || {
                unsafe {
                    let _ = windows::Win32::System::WinRT::RoInitialize(
                        windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
                    );
                }
                while let Ok((api, path, body)) = rx.recv() {
                    let _ = api.call(&path, Some(body));
                }
            });
            tx
        });
        let _ = tx.try_send((self.clone(), format!("/rotor/session/{}/feedback", pct(session)), body));
    }
}

type Feedback = (Api, String, Value);
static FEEDBACK: std::sync::OnceLock<std::sync::mpsc::SyncSender<Feedback>> = std::sync::OnceLock::new();

pub fn oauth(path: &str, fields: &[(&str, &str)]) -> Result<Value> {
    let body = fields.iter().map(|(k, v)| format!("{}={}", pct(k), pct(v))).collect::<Vec<_>>().join("&");
    let (status, text) =
        request(&format!("https://oauth.yandex.ru/{path}"), None, Some(&body), "application/x-www-form-urlencoded")?;
    if status >= 500 {
        return Err(NETWORK);
    }
    serde_json::from_str(&text).map_err(|_| NETWORK)
}

pub fn request(url: &str, token: Option<&str>, body: Option<&str>, mime: &str) -> Result<(u16, String)> {
    let run = || -> windows::core::Result<(u16, String)> {
        let filter = HttpBaseProtocolFilter::new()?;
        filter.SetAllowAutoRedirect(false)?;
        let client = HttpClient::Create(&filter)?;
        let verb = if body.is_some() { HttpMethod::Post()? } else { HttpMethod::Get()? };
        let req = HttpRequestMessage::Create(&verb, &Uri::CreateUri(&HSTRING::from(url))?)?;
        let headers = req.Headers()?;
        headers.TryAppendWithoutValidation(&HSTRING::from("Accept-Language"), &HSTRING::from("ru"))?;
        if let Some(token) = token {
            headers.TryAppendWithoutValidation(
                &HSTRING::from("Authorization"),
                &HSTRING::from(format!("OAuth {token}")),
            )?;
            headers.TryAppendWithoutValidation(
                &HSTRING::from("X-Yandex-Music-Client"),
                &HSTRING::from("YandexMusicAndroid/24023621"),
            )?;
        }
        if let Some(body) = body {
            req.SetContent(&HttpStringContent::CreateFromStringWithEncodingAndMediaType(
                &HSTRING::from(body),
                UnicodeEncoding::Utf8,
                &HSTRING::from(mime),
            )?)?;
        }
        let op = client.SendRequestAsync(&req)?;
        let at = Instant::now();
        while op.Status()?.0 == 0 {
            if at.elapsed() > Duration::from_secs(15) {
                op.Cancel()?;
                return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let response = op.GetResults()?;
        let content = response.Content()?;
        if content.Headers()?.ContentLength().is_ok_and(|n| n.Value().unwrap_or(0) > 2_000_000) {
            return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL));
        }
        let op = content.ReadAsStringAsync()?;
        while op.Status()?.0 == 0 {
            if at.elapsed() > Duration::from_secs(20) {
                op.Cancel()?;
                return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let text = op.GetResults()?.to_string();
        if text.len() > 2_000_000 {
            return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL));
        }
        Ok((response.StatusCode()?.0 as u16, text))
    };
    run().map_err(|_| NETWORK)
}

pub fn trusted_url(url: &str) -> bool {
    let Ok(uri) = Uri::CreateUri(&HSTRING::from(url)) else { return false };
    let host = uri.Host().unwrap_or_default().to_string().to_ascii_lowercase();
    uri.SchemeName().is_ok_and(|s| s == "https")
        && (host.ends_with(".yandex.net") || host.ends_with(".yandex.ru"))
        && uri.UserName().is_ok_and(|s| s.is_empty())
        && uri.Password().is_ok_and(|s| s.is_empty())
        && uri.Port().is_ok_and(|p| p == 443)
}

fn direct_url(xml: &str) -> Result<String> {
    let doc = XmlDocument::new().map_err(|_| NETWORK)?;
    doc.LoadXml(&HSTRING::from(xml)).map_err(|_| NETWORK)?;
    let field = |name: &str| -> Result<String> {
        doc.SelectSingleNode(&HSTRING::from(format!("/download-info/{name}")))
            .and_then(|n| n.InnerText())
            .map(|s| s.to_string())
            .map_err(|_| NETWORK)
    };
    let (host, path, ts, salt) = (field("host")?, field("path")?, field("ts")?, field("s")?);
    if !host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        || !path.starts_with('/')
        || !ts.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(NETWORK);
    }
    let signature = Md5::digest(format!("XGRlBW9FXlekgbPrRHuSiA{}{salt}", &path[1..]));
    let url = format!("https://{host}/get-mp3/{signature:x}/{ts}{path}");
    if trusted_url(&url) { Ok(url) } else { Err(NETWORK) }
}

fn numeric_id(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) if number.is_u64() => Some(number.to_string()),
        Value::String(id) if !id.is_empty() && id.len() <= 20 && id.bytes().all(|byte| byte.is_ascii_digit()) => {
            Some(id.clone())
        }
        _ => None,
    }
}

fn with_album(id: String, album: &Value) -> String {
    match numeric_id(album) {
        Some(album) => format!("{id}:{album}"),
        None => id,
    }
}

pub fn parse_tracks(value: &Value) -> Vec<Track> {
    let batch = value["batchId"].as_str().unwrap_or_default();
    value["sequence"]
        .as_array()
        .into_iter()
        .flatten()
        .take(100)
        .filter_map(|entry| parse_track(&entry["track"], batch))
        .collect()
}

fn parse_track(t: &Value, batch: &str) -> Option<Track> {
    if t["available"].as_bool() == Some(false) {
        return None;
    }
    let id = with_album(numeric_id(&t["id"])?, &t["albums"][0]["id"]);
    let title = t["title"].as_str()?.chars().take(200).collect();
    let artist = t["artists"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["name"].as_str())
        .take(8)
        .collect::<Vec<_>>()
        .join(", ")
        .chars()
        .take(200)
        .collect();
    let cover = t["coverUri"]
        .as_str()
        .map(|c| format!("https://{}", c.replace("%%", "300x300")))
        .filter(|c| trusted_url(c))
        .unwrap_or_default();
    Some(Track {
        id,
        title,
        artist,
        cover,
        batch: batch.into(),
        session: String::new(),
        palette: super::palette::Palette::from_track(t),
    })
}

pub fn parse_groups(v: &Value) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut context = Group { key: "context".into(), name: "Wave activity".into(), items: Vec::new() };
    for block in v["blocks"].as_array().into_iter().flatten().filter(|b| b["type"] == "contexts") {
        for item in block["items"].as_array().into_iter().flatten() {
            let item = item.get("station").unwrap_or(item);
            if let (Some(kind), Some(tag), Some(name)) =
                (item["id"]["type"].as_str(), item["id"]["tag"].as_str(), item["name"].as_str())
            {
                context.items.push((name.chars().take(50).collect(), format!("{kind}:{tag}")));
            }
        }
    }
    if !context.items.is_empty() {
        context.items.truncate(30);
        groups.push(context);
    }
    for (key, name) in [("diversity", "Variety"), ("moodEnergy", "Mood"), ("language", "Language")] {
        let items = v["settingRestrictions"][key]["possibleValues"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|i| i["unspecified"].as_bool() != Some(true))
            .filter_map(|i| {
                Some((
                    i["name"].as_str()?.chars().take(50).collect(),
                    i["serializedSeed"].as_str()?.chars().take(150).collect(),
                ))
            })
            .take(30)
            .collect::<Vec<_>>();
        if !items.is_empty() {
            groups.push(Group { key: key.into(), name: name.into(), items });
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Reads the configured Yandex account and plays a catalog track muted without changing the library"]
    fn configured_account_can_read_wave_filters_and_search_a_station() {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let api = Api {
            token: crate::services::integrations::load_secret("yandex-music").expect("Yandex Music sign-in required"),
            base: None,
        };
        let (name, uid) = api.account().unwrap();
        assert!(!name.is_empty());
        let liked = api.library_ids(uid, false).unwrap();
        if !liked.is_empty() {
            let tracks = api.library_tracks(&liked[..liked.len().min(4)]).unwrap();
            assert!(tracks.iter().all(|track| !track.title.is_empty()));
        }
        assert!(!api.groups().unwrap().is_empty());
        let (seed, label) = api.station("Daft Punk", true).unwrap();
        assert!(seed.starts_with("artist:") && label.to_lowercase().contains("daft punk"));
        let (tx, _rx) = std::sync::mpsc::sync_channel(16);
        let mut worker = super::super::Worker::new(0, tx);
        worker.api = api;
        worker.state.config.volume = 0;
        let result = worker
            .voice(super::super::VoiceAction::Track { title: "Around the World".into(), artist: "Daft Punk".into() })
            .unwrap();
        assert!(result.contains("Around the World"));
        let player = worker.player.as_ref().unwrap();
        let deadline = Instant::now() + Duration::from_secs(12);
        while !player.state().2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(player.state().2, "exact catalog track must start in the local player");
        assert_eq!(worker.state.media.title, "Around the World");
        assert_eq!(worker.state.media.artist, "Daft Punk");
        worker.stop();
    }
    #[test]
    #[ignore = "Uses the configured Yandex catalog and starts the requested track muted"]
    fn spoken_jesus_starts_the_requested_recording() {
        use super::super::Worker;
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let (tx, _rx) = std::sync::mpsc::sync_channel(16);
        let mut worker = Worker::new(0, tx);
        worker.api.token = crate::services::integrations::load_secret("yandex-music").expect("Yandex Music sign-in");
        worker.state.config.volume = 0;
        for phrase in [
            "Включи в моей волне трек Jesus от группы курящих нет",
            "Найди в моей волне трек «Джезус» от группы «Курящих нет» и включи его.",
        ] {
            let action = crate::tools::voicecmd::local_yandex(phrase).expect("direct request");
            worker.voice(action).unwrap_or_else(|error| panic!("{phrase}: {error}"));
            let player = worker.player.as_ref().unwrap();
            let deadline = Instant::now() + Duration::from_secs(12);
            while !player.state().2 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            assert!(player.state().2, "requested track must start");
            assert!(worker.state.media.title.eq_ignore_ascii_case("Jesus"));
            assert_eq!(worker.state.media.artist.to_lowercase(), "курящих нет");
        }
        worker.stop();
    }

    fn server(responses: Vec<(u16, &'static str)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let deadline = Instant::now() + Duration::from_secs(10);
                let (mut socket, _) = loop {
                    if let Ok(socket) = listener.accept() {
                        break socket;
                    }
                    assert!(Instant::now() < deadline, "missing HTTP request");
                    std::thread::sleep(Duration::from_millis(10));
                };
                socket.set_nonblocking(false).unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let size = socket.read(&mut buffer).unwrap();
                    if size == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..size]);
                    if let Some(end) = request.windows(4).position(|s| s == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                        let length: usize = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .map_or(0, |s| s.trim().parse().unwrap());
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(request).unwrap());
                write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            requests
        });
        (base, handle)
    }

    #[test]
    #[ignore = "Needs native WinRT HTTP"]
    fn starting_my_wave_leaves_the_station_and_uses_saved_filters() {
        use super::super::Worker;
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let (base, server) = server(vec![(200, r#"{"result":{"radioSessionId":"fixture","sequence":[]}}"#)]);
        let (tx, _rx) = std::sync::mpsc::sync_channel(16);
        let mut worker = Worker::new(0, tx);
        worker.api = Api { token: "fixture-token".into(), base: Some(base) };
        worker.station = Some("artist:8".into());
        worker.state.config.selection.insert("language".into(), "settingLanguage:russian".into());
        let action = crate::tools::voicecmd::local_yandex("Включи мою волну.").unwrap();
        assert!(worker.voice(action).is_err());
        assert_eq!(worker.station.as_deref(), Some("artist:8"));
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("POST /rotor/session/new "));
        let body: Value = serde_json::from_str(requests[0].split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["seeds"], json!(["user:onyourwave", "settingLanguage:russian"]));
    }

    #[test]
    #[ignore = "Needs WinRT (Uri, HttpClient), which wine does not provide"]
    fn rotor_protocol_keeps_oauth_on_api_and_serializes_personal_seeds() {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let (base, server) = server(vec![
            (200, r#"{"result":{"radioSessionId":"session-1","batchId":"batch-1","sequence":[]}}"#),
            (401, r#"{"error":{"name":"session-expired"}}"#),
        ]);
        let api = Api { token: "fixture-token".into(), base: Some(base) };
        let body = json!({"seeds":["user:onyourwave"],"includeTracksInResponse":true});
        assert_eq!(api.call("/rotor/session/new", Some(body.clone())).unwrap()["radioSessionId"], "session-1");
        assert_eq!(api.account().unwrap_err(), SIGN_IN);
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("POST /rotor/session/new HTTP/1.1"));
        assert!(requests[0].to_ascii_lowercase().contains("authorization: oauth fixture-token"));
        assert_eq!(serde_json::from_str::<Value>(requests[0].split("\r\n\r\n").nth(1).unwrap()).unwrap(), body);
    }

    #[test]
    #[ignore = "Needs native WinRT HTTP"]
    fn library_protocol_preserves_track_order_and_writes_ratings_as_forms() {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let (base, server) = server(vec![
            (200, r#"{"result":{"library":{"tracks":[{"id":"2","albumId":20},{"id":1},{"id":"2"}]}}}"#),
            (200, r#"{"result":[{"id":1,"title":"First"},{"id":2,"title":"Second","albums":[{"id":20}]}]}"#),
            (200, r#"{"result":{"revision":2}}"#),
            (200, r#"{"result":{"revision":3}}"#),
        ]);
        let api = Api { token: "fixture-token".into(), base: Some(base) };
        let ids = api.library_ids(8, false).unwrap();
        assert_eq!(ids, ["2:20", "1"]);
        let tracks = api.library_tracks(&ids).unwrap();
        assert_eq!(tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["2:20", "1"]);
        api.rate(8, &tracks[0], "like").unwrap();
        api.rate(8, &tracks[0], "dislike").unwrap();
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("GET /users/8/likes/tracks?if-modified-since-revision=0 "));
        assert!(requests[1].starts_with("POST /tracks "));
        assert!(requests[1].ends_with("track-ids=2%3A20%2C1"));
        for (request, path) in requests[2..].iter().zip(["likes", "dislikes"]) {
            assert!(request.starts_with(&format!("POST /users/8/{path}/tracks/add-multiple ")));
            assert!(request.to_ascii_lowercase().contains("application/x-www-form-urlencoded"));
            assert!(request.ends_with("track-ids=2%3A20"));
        }
    }

    #[test]
    #[ignore = "Needs WinRT (Uri, HttpClient), which wine does not provide"]
    fn api_does_not_follow_redirects_with_a_token() {
        use std::io::{Read, Write};
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let url = base.clone();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut bytes = [0; 4096];
            let _ = s.read(&mut bytes).unwrap();
            write!(
                s,
                "HTTP/1.1 302 Found\r\nLocation: {url}/redirected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });
        let (status, _) = request(&base, Some("fixture-token"), None, "").unwrap();
        assert_eq!(status, 302);
        server.join().unwrap();
    }

    #[test]
    #[ignore = "Needs WinRT (Uri, HttpClient), which wine does not provide"]
    fn playback_url_signature_rejects_remote_hosts() {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let xml =
            "<download-info><host>storage.yandex.net</host><path>/a/b</path><ts>1234</ts><s>abc</s></download-info>";
        let expected =
            format!("https://storage.yandex.net/get-mp3/{:x}/1234/a/b", Md5::digest(b"XGRlBW9FXlekgbPrRHuSiAa/babc"));
        assert_eq!(direct_url(xml).unwrap(), expected);
        assert!(direct_url(&xml.replace("storage.yandex.net", "attacker.invalid")).is_err());
    }
    #[test]
    #[ignore = "Requests a temporary Yandex device code without signing into an account"]
    fn live_device_flow_returns_code_and_pending() {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let code = oauth("device/code", &[("client_id", CLIENT_ID), ("device_name", "Night Island connection test")])
            .expect("device-code endpoint");
        assert!(code["user_code"].as_str().is_some_and(|s| !s.is_empty()), "device code missing");
        let result = oauth(
            "token",
            &[
                ("grant_type", "device_code"),
                ("code", code["device_code"].as_str().unwrap()),
                ("client_id", CLIENT_ID),
                ("client_secret", CLIENT_SECRET),
            ],
        )
        .expect("token endpoint");
        assert_eq!(result["error"].as_str(), Some("authorization_pending"));
    }
    #[test]
    #[ignore = "Needs WinRT (Uri, HttpClient), which wine does not provide"]
    fn media_urls_reject_untrusted_hosts_and_credentials() {
        assert!(trusted_url("https://avatars.yandex.net/get-music-content/1/300x300"));
        for s in [
            "http://x.yandex.net/a",
            "https://yandex.net.evil.com/a",
            "https://x.yandex.net@evil.com/a",
            "file:///C:/a",
            "https://user:pass@x.yandex.net/a",
            "https://x.yandex.net:8080/a",
        ] {
            assert!(!trusted_url(s), "{s}");
        }
    }
    #[test]
    fn rotor_tracks_preserve_batch_and_skip_unavailable() {
        let tracks = parse_tracks(
            &json!({"batchId":"batch-1", "sequence":[{"track":{"id":42,"title":"Song","available":true,"artists":[{"name":"Artist"}]}},{"track":{"id":"3","title":"Unavailable","available":false}}]}),
        );
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].batch, "batch-1");
        assert_eq!(tracks[0].id, "42");
    }
    #[test]
    fn settings_use_server_seeds_and_omit_unspecified() {
        let groups = parse_groups(
            &json!({"settingRestrictions":{"language":{"possibleValues":[{"name":"Any","unspecified":true,"serializedSeed":"x"},{"name":"Instrumental","serializedSeed":"settingLanguage:without-words"}]}}}),
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 1);
        assert_eq!(groups[0].items[0].1, "settingLanguage:without-words");
    }
}
