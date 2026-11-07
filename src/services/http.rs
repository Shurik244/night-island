use std::time::{Duration, Instant};
use windows::Foundation::Uri;
use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream, UnicodeEncoding};
use windows::Web::Http::Headers::HttpCredentialsHeaderValue;
use windows::Web::Http::{
    HttpBufferContent, HttpClient, HttpCompletionOption, HttpMethod, HttpRequestMessage, HttpStringContent,
};
use windows::core::{HSTRING, Interface};
use windows_future::{AsyncStatus, IAsyncInfo};

const MAX_BODY_CHARS: usize = 600_000;

pub struct Response {
    pub status: u16,
    pub body: String,
}

pub fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn request(method: &str, url: &str, bearer: Option<&str>, form: Option<&str>) -> Option<Response> {
    request_as(method, url, bearer, form, None)
}

pub fn request_as(
    method: &str,
    url: &str,
    bearer: Option<&str>,
    form: Option<&str>,
    agent: Option<&str>,
) -> Option<Response> {
    let verb = match method {
        "GET" => HttpMethod::Get().ok()?,
        "POST" => HttpMethod::Post().ok()?,
        "PUT" => HttpMethod::Put().ok()?,
        "DELETE" => HttpMethod::Delete().ok()?,
        _ => return None,
    };
    let client = HttpClient::new().ok()?;
    let message = HttpRequestMessage::Create(&verb, &Uri::CreateUri(&HSTRING::from(url)).ok()?).ok()?;
    if let Some(agent) = agent {
        message.Headers().ok()?.UserAgent().ok()?.TryParseAdd(&HSTRING::from(agent)).ok()?;
    }
    if let Some(token) = bearer {
        let value =
            HttpCredentialsHeaderValue::CreateFromSchemeWithToken(&HSTRING::from("Bearer"), &HSTRING::from(token))
                .ok()?;
        message.Headers().ok()?.SetAuthorization(&value).ok()?;
    }
    if let Some(body) = form {
        let content = HttpStringContent::CreateFromStringWithEncodingAndMediaType(
            &HSTRING::from(body),
            UnicodeEncoding::Utf8,
            &HSTRING::from("application/x-www-form-urlencoded"),
        )
        .ok()?;
        message.SetContent(&content).ok()?;
    }
    let reply = client.SendRequestAsync(&message).ok()?.join().ok()?;
    let status = reply.StatusCode().ok()?.0 as u16;
    let body = reply.Content().ok()?.ReadAsStringAsync().ok()?.join().ok()?.to_string();
    Some(Response { status, body: body.chars().take(MAX_BODY_CHARS).collect() })
}

pub fn request_json(method: &str, url: &str, bearer: &str, json: &str) -> Option<Response> {
    let verb = match method {
        "POST" => HttpMethod::Post().ok()?,
        "PUT" => HttpMethod::Put().ok()?,
        _ => return None,
    };
    let client = HttpClient::new().ok()?;
    let message = HttpRequestMessage::Create(&verb, &Uri::CreateUri(&HSTRING::from(url)).ok()?).ok()?;
    let value =
        HttpCredentialsHeaderValue::CreateFromSchemeWithToken(&HSTRING::from("Bearer"), &HSTRING::from(bearer)).ok()?;
    message.Headers().ok()?.SetAuthorization(&value).ok()?;
    let content = HttpStringContent::CreateFromStringWithEncodingAndMediaType(
        &HSTRING::from(json),
        UnicodeEncoding::Utf8,
        &HSTRING::from("application/json"),
    )
    .ok()?;
    message.SetContent(&content).ok()?;
    let reply = client.SendRequestAsync(&message).ok()?.join().ok()?;
    let status = reply.StatusCode().ok()?.0 as u16;
    let body = reply.Content().ok()?.ReadAsStringAsync().ok()?.join().ok()?.to_string();
    Some(Response { status, body: body.chars().take(MAX_BODY_CHARS).collect() })
}

pub struct BytesReply {
    pub status: u16,
    pub body: Vec<u8>,
}

pub fn get_bytes(url: &str, max_bytes: usize, agent: Option<&str>) -> Option<BytesReply> {
    send_bytes("GET", url, &[], "", max_bytes, agent)
}

pub fn send_bytes(
    method: &str,
    url: &str,
    body: &[u8],
    content_type: &str,
    max_bytes: usize,
    agent: Option<&str>,
) -> Option<BytesReply> {
    let verb = match method {
        "GET" => HttpMethod::Get().ok()?,
        "POST" => HttpMethod::Post().ok()?,
        "PUT" => HttpMethod::Put().ok()?,
        _ => return None,
    };
    let client = HttpClient::new().ok()?;
    let message = HttpRequestMessage::Create(&verb, &Uri::CreateUri(&HSTRING::from(url)).ok()?).ok()?;
    if let Some(agent) = agent {
        message.Headers().ok()?.UserAgent().ok()?.TryParseAdd(&HSTRING::from(agent)).ok()?;
    }
    if !body.is_empty() {
        let stream = InMemoryRandomAccessStream::new().ok()?;
        let writer = DataWriter::CreateDataWriter(&stream).ok()?;
        writer.WriteBytes(body).ok()?;
        let content = HttpBufferContent::CreateFromBuffer(&writer.DetachBuffer().ok()?).ok()?;
        content
            .Headers()
            .ok()?
            .TryAppendWithoutValidation(&HSTRING::from("Content-Type"), &HSTRING::from(content_type))
            .ok()?;
        message.SetContent(&content).ok()?;
    }
    let reply = client.SendRequestAsync(&message).ok()?.join().ok()?;
    let status = reply.StatusCode().ok()?.0 as u16;
    let content = reply.Content().ok()?;
    if let Ok(len) = content.Headers().and_then(|h| h.ContentLength())
        && let Ok(len) = len.Value()
        && len > max_bytes as u64
    {
        return None;
    }
    let buffer = content.ReadAsBufferAsync().ok()?.join().ok()?;
    let len = buffer.Length().ok()? as usize;
    if len > max_bytes {
        return None;
    }
    let mut bytes = vec![0u8; len];
    if len > 0 {
        DataReader::FromBuffer(&buffer).ok()?.ReadBytes(&mut bytes).ok()?;
    }
    Some(BytesReply { status, body: bytes })
}

pub fn post_json(url: &str, headers: &[(&str, &str)], body: &str, agent: Option<&str>) -> Option<Response> {
    let client = HttpClient::new().ok()?;
    let message = json_request(url, headers, body, agent)?;
    bounded_response(&client, &message, Duration::from_secs(60))
}

fn json_request(url: &str, headers: &[(&str, &str)], body: &str, agent: Option<&str>) -> Option<HttpRequestMessage> {
    let message =
        HttpRequestMessage::Create(&HttpMethod::Post().ok()?, &Uri::CreateUri(&HSTRING::from(url)).ok()?).ok()?;
    let sent = message.Headers().ok()?;
    if let Some(agent) = agent {
        sent.UserAgent().ok()?.TryParseAdd(&HSTRING::from(agent)).ok()?;
    }
    for (name, value) in headers {
        if !sent.TryAppendWithoutValidation(&HSTRING::from(*name), &HSTRING::from(*value)).ok()? {
            return None;
        }
    }
    let content = HttpStringContent::CreateFromStringWithEncodingAndMediaType(
        &HSTRING::from(body),
        UnicodeEncoding::Utf8,
        &HSTRING::from("application/json"),
    )
    .ok()?;
    message.SetContent(&content).ok()?;
    Some(message)
}

pub fn post_json_stream(
    url: &str,
    headers: &[(&str, &str)],
    body: &str,
    running: impl Fn() -> bool,
    mut consume: impl FnMut(&[u8]) -> Option<()>,
) -> Option<u16> {
    use windows::Storage::Streams::InputStreamOptions;
    let client = HttpClient::new().ok()?;
    let message = json_request(url, headers, body, Some("NightIsland/0.1"))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let operation = client.SendRequestWithOptionAsync(&message, HttpCompletionOption::ResponseHeadersRead).ok()?;
    wait_response_while(&operation.cast().ok()?, deadline, &running)?;
    let reply = operation.GetResults().ok()?;
    let status = reply.StatusCode().ok()?.0 as u16;
    if status != 200 {
        return Some(status);
    }
    let operation = reply.Content().ok()?.ReadAsInputStreamAsync().ok()?;
    wait_response_while(&operation.cast().ok()?, deadline, &running)?;
    let stream = operation.GetResults().ok()?;
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.SetInputStreamOptions(InputStreamOptions::Partial).ok()?;
    while running() {
        let operation = reader.LoadAsync(8192).ok()?;
        wait_response_while(&operation.cast().ok()?, deadline, &running)?;
        let count = operation.GetResults().ok()? as usize;
        if count == 0 {
            break;
        }
        let mut bytes = vec![0; count];
        reader.ReadBytes(&mut bytes).ok()?;
        consume(&bytes)?;
    }
    Some(status)
}

pub fn post_json_for_bytes(
    url: &str,
    headers: &[(&str, &str)],
    body: &str,
    max_bytes: usize,
    agent: Option<&str>,
) -> Option<BytesReply> {
    let client = HttpClient::new().ok()?;
    let message = json_request(url, headers, body, agent)?;
    let reply = client.SendRequestAsync(&message).ok()?.join().ok()?;
    let status = reply.StatusCode().ok()?.0 as u16;
    let buffer = reply.Content().ok()?.ReadAsBufferAsync().ok()?.join().ok()?;
    let len = buffer.Length().ok()? as usize;
    if len > max_bytes {
        return None;
    }
    let mut bytes = vec![0u8; len];
    if len > 0 {
        DataReader::FromBuffer(&buffer).ok()?.ReadBytes(&mut bytes).ok()?;
    }
    Some(BytesReply { status, body: bytes })
}

pub fn post_bytes(
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    content_type: &str,
    agent: Option<&str>,
) -> Option<Response> {
    let client = HttpClient::new().ok()?;
    let message =
        HttpRequestMessage::Create(&HttpMethod::Post().ok()?, &Uri::CreateUri(&HSTRING::from(url)).ok()?).ok()?;
    let sent = message.Headers().ok()?;
    if let Some(agent) = agent {
        sent.UserAgent().ok()?.TryParseAdd(&HSTRING::from(agent)).ok()?;
    }
    for (name, value) in headers {
        if !sent.TryAppendWithoutValidation(&HSTRING::from(*name), &HSTRING::from(*value)).ok()? {
            return None;
        }
    }
    let stream = InMemoryRandomAccessStream::new().ok()?;
    let writer = DataWriter::CreateDataWriter(&stream).ok()?;
    writer.WriteBytes(body).ok()?;
    let content = HttpBufferContent::CreateFromBuffer(&writer.DetachBuffer().ok()?).ok()?;
    content
        .Headers()
        .ok()?
        .TryAppendWithoutValidation(&HSTRING::from("Content-Type"), &HSTRING::from(content_type))
        .ok()?;
    message.SetContent(&content).ok()?;
    bounded_response(&client, &message, Duration::from_secs(60))
}

fn wait_response(operation: &IAsyncInfo, deadline: Instant) -> Option<()> {
    wait_response_while(operation, deadline, &|| true)
}

fn wait_response_while(operation: &IAsyncInfo, deadline: Instant, running: &impl Fn() -> bool) -> Option<()> {
    while operation.Status().ok()? == AsyncStatus::Started {
        if Instant::now() >= deadline || !running() {
            let _ = operation.Cancel();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    (operation.Status().ok()? == AsyncStatus::Completed).then_some(())
}

fn bounded_response(client: &HttpClient, message: &HttpRequestMessage, timeout: Duration) -> Option<Response> {
    let deadline = Instant::now() + timeout;
    let operation = client.SendRequestWithOptionAsync(message, HttpCompletionOption::ResponseHeadersRead).ok()?;
    wait_response(&operation.cast().ok()?, deadline)?;
    let reply = operation.GetResults().ok()?;
    let status = reply.StatusCode().ok()?.0 as u16;
    let content = reply.Content().ok()?;
    if content.Headers().ok()?.ContentLength().is_ok_and(|size| size.Value().unwrap_or(0) > 4 * MAX_BODY_CHARS as u64) {
        return None;
    }
    let operation = content.ReadAsStringAsync().ok()?;
    wait_response(&operation.cast().ok()?, deadline)?;
    let body = operation.GetResults().ok()?.to_string();
    if body.len() > 4 * MAX_BODY_CHARS {
        return None;
    }
    Some(Response { status, body: body.chars().take(MAX_BODY_CHARS).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stalled_request_is_cancelled_and_a_completed_one_is_available() {
        use windows_future::IAsyncOperation;
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let pending = IAsyncOperation::<bool>::spawn(move || Ok(rx.recv_timeout(Duration::from_secs(2)).is_ok()));
        let info: IAsyncInfo = pending.cast().unwrap();
        let start = Instant::now();
        assert!(wait_response(&info, start + Duration::from_millis(40)).is_none());
        assert!(start.elapsed() < Duration::from_secs(1));
        drop(tx);
        let done = IAsyncOperation::<bool>::ready(Ok(true));
        assert!(wait_response(&done.cast().unwrap(), Instant::now() + Duration::from_secs(1)).is_some());
        assert!(done.GetResults().unwrap());
    }

    #[test]
    fn percent_encoding_keeps_unreserved_only() {
        assert_eq!(pct("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(pct("Az09-_.~"), "Az09-_.~");
    }
}
