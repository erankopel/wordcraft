use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::Duration;

use super::*;
use crate::{HOST, Scope};

const KEY: &str = "sk-ant-test-0123456789";

/// A one-shot server on this machine: it answers the first request with `status` and `body` and
/// hands back the request as it arrived, headers and body.
fn serve(status: &str, body: &str) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/messages", listener.local_addr().unwrap());
    let (status, body) = (status.to_owned(), body.to_owned());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut raw = Vec::new();
        let mut chunk = [0u8; 4096];
        let header_end = loop {
            let n = stream.read(&mut chunk).unwrap();
            raw.extend_from_slice(&chunk[..n]);
            if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
            if n == 0 {
                break raw.len();
            }
        };
        let head = String::from_utf8_lossy(&raw[..header_end]).to_lowercase();
        let length = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
        while raw.len() < header_end + length {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&chunk[..n]);
        }
        let answer =
            format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        stream.write_all(answer.as_bytes()).unwrap();
        String::from_utf8_lossy(&raw).into_owned()
    });
    (url, handle)
}

fn request() -> Request {
    Request { instruction: "Shorten".into(), context: "A long sentence.".into(), scope: Scope::Selection, history: Vec::new() }
}

const GOOD: &str = r#"{"type":"message","model":"claude-opus-5-5","content":[{"type":"text","text":"Short."}],"stop_reason":"end_turn","usage":{"input_tokens":9,"output_tokens":2}}"#;

#[test]
fn a_request_carries_the_key_the_version_the_beta_and_the_body() {
    let (url, server) = serve("200 OK", GOOD);
    let mut claude = Claude::with_endpoint(KEY, DEFAULT_MODEL, &url);
    let reply = claude.send(&request()).unwrap();
    assert_eq!((reply.text.as_str(), reply.usage.output_tokens), ("Short.", 2));
    let seen = server.join().unwrap();
    let lower = seen.to_lowercase();
    assert!(lower.starts_with("post /v1/messages"), "{seen}");
    assert!(seen.contains(KEY), "the key is sent");
    assert!(lower.contains(&format!("x-api-key: {}", KEY.to_lowercase())), "in the x-api-key header: {seen}");
    assert!(lower.contains(&format!("anthropic-version: {API_VERSION}")), "{seen}");
    assert!(lower.contains(&format!("anthropic-beta: {FALLBACK_BETA}")), "{seen}");
    assert!(lower.contains("content-type: application/json"), "{seen}");
    let body: serde_json::Value = serde_json::from_str(seen.split("\r\n\r\n").nth(1).unwrap_or_default()).unwrap();
    assert_eq!(body["model"], DEFAULT_MODEL);
    assert!(body["messages"][0]["content"].as_str().unwrap_or_default().contains("Shorten"));
    assert_eq!(seen.matches(KEY).count(), 1, "the key appears once, in its header, never in the body: {seen}");
}

#[test]
fn an_error_answer_is_an_http_error_and_nothing_prints_the_key() {
    let (url, server) = serve("401 Unauthorized", r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#);
    let mut claude = Claude::with_endpoint(KEY, DEFAULT_MODEL, &url);
    let error = claude.send(&request()).unwrap_err();
    server.join().unwrap();
    assert!(matches!(error, Error::Http { status: 401, .. }), "{error:?}");
    for text in [format!("{error}"), format!("{error:?}"), format!("{claude:?}")] {
        assert!(!text.contains(KEY) && !text.contains("0123456789"), "the key leaked into: {text}");
    }
}

/// An API endpoint never redirects, and a redirect must not carry the key to another host.
#[test]
fn a_redirect_is_reported_not_followed() {
    let (url, server) = serve("302 Found\r\nLocation: http://127.0.0.1:9/elsewhere", "{}");
    let mut claude = Claude::with_endpoint(KEY, DEFAULT_MODEL, &url);
    let error = claude.send(&request()).unwrap_err();
    server.join().unwrap();
    assert!(matches!(error, Error::Http { status: 302, .. }), "{error:?}");
}

#[test]
fn no_connection_is_a_network_error_without_the_key() {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    // The listener is gone: nothing answers on this port.
    let mut claude = Claude::with_endpoint(KEY, DEFAULT_MODEL, &format!("http://127.0.0.1:{port}/v1/messages"));
    let error = claude.send(&request()).unwrap_err();
    assert!(matches!(error, Error::Network(_)), "{error:?}");
    assert!(!format!("{error}").contains(KEY));
}

#[test]
fn a_request_that_cannot_be_built_is_not_sent() {
    // An empty instruction fails before any connection is made, so this needs no server.
    let mut claude = Claude::with_endpoint(KEY, DEFAULT_MODEL, "http://127.0.0.1:9/v1/messages");
    let empty = Request { instruction: " ".into(), ..request() };
    assert_eq!(claude.send(&empty), Err(Error::EmptyInstruction));
}

#[test]
fn a_missing_or_blank_key_is_reported_and_a_given_one_is_not_printed() {
    assert!(matches!(Claude::from_key(None), Err(Error::MissingKey)));
    assert!(matches!(Claude::from_key(Some("  \n")), Err(Error::MissingKey)));
    let claude = Claude::from_key(Some(" sk-secret-value \n")).unwrap();
    assert!(!format!("{claude:?}").contains("sk-secret-value"));
    let disclosure = claude.disclosure();
    assert_eq!((disclosure.recipient.as_str(), disclosure.model.as_str(), disclosure.leaves_machine), (HOST, DEFAULT_MODEL, true));
}
