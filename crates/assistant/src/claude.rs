//! The Claude provider: the Messages API over HTTPS with the person's own key.
//!
//! The key is the one the person typed in or set in [`KEY_ENV`] and lives only in this struct's
//! memory: it is sent in one header, never printed (its `Debug` and every error leave it out), never
//! put in a document, and never saved by this provider (the pane keeps it in a file only when the person
//! asks, see `key_store`). The endpoint is always HTTPS and redirects are not followed, so the key cannot
//! be sent in the clear or to a host the endpoint did not name.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::agent::{Step, ToolRequest, build_tool_body, parse_step};
use crate::{
    API_VERSION, DEFAULT_MODEL, Disclosure, ENDPOINT, Error, FALLBACK_BETA, KEY_ENV, MAX_RESPONSE_BYTES, Provider, Reply, Request, build_body,
    clean_key, parse_reply,
};

/// How long one request may take, in all. A reply is read whole, and a long one takes minutes.
const TIMEOUT: Duration = Duration::from_secs(600);
/// How long finding the host and connecting may take, so a request that cannot start fails fast.
const SETUP_TIMEOUT: Duration = Duration::from_secs(20);
/// How many times a request is sent again after a busy or failing service or a connection that could not
/// be made, as the official SDKs do.
const RETRIES: u32 = 2;
/// The longest wait before sending again.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(30);
/// How often a wait before sending again looks whether the request was let go.
const CANCEL_CHECK: Duration = Duration::from_millis(100);

/// A key that never prints.
struct Secret(String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

/// Sends requests to the Messages API.
pub struct Claude {
    key: Secret,
    model: String,
    endpoint: String,
    agent: ureq::Agent,
    /// Set when the person lets the request go: nothing is sent again after that.
    cancel: Option<Arc<AtomicBool>>,
}

impl fmt::Debug for Claude {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Claude").field("key", &self.key).field("model", &self.model).field("endpoint", &self.endpoint).finish_non_exhaustive()
    }
}

impl Claude {
    /// The provider for the key in [`KEY_ENV`], or [`Error::MissingKey`].
    pub fn from_env() -> Result<Claude, Error> {
        Claude::from_key(std::env::var(KEY_ENV).ok().as_deref())
    }

    /// The provider for a key already read. A missing or blank key is [`Error::MissingKey`].
    pub fn from_key(key: Option<&str>) -> Result<Claude, Error> {
        let key = key.map(clean_key).filter(|k| !k.is_empty()).ok_or(Error::MissingKey)?;
        Ok(Claude::build(&key, DEFAULT_MODEL, ENDPOINT, true))
    }

    /// Whether [`KEY_ENV`] holds a key, for the pane to say so. The key itself is not read out.
    pub fn key_present() -> bool {
        std::env::var(KEY_ENV).is_ok_and(|k| !k.trim().is_empty())
    }

    fn build(key: &str, model: &str, endpoint: &str, https_only: bool) -> Claude {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .https_only(https_only)
            .timeout_resolve(Some(SETUP_TIMEOUT))
            .timeout_connect(Some(SETUP_TIMEOUT))
            .timeout_global(Some(TIMEOUT))
            .build();
        Claude {
            key: Secret(key.to_owned()),
            model: model.to_owned(),
            endpoint: endpoint.to_owned(),
            agent: ureq::Agent::new_with_config(config),
            cancel: None,
        }
    }

    /// A provider that sends to `endpoint` over plain HTTP, for the tests, which point it at a server
    /// on this machine. It does not exist outside them: the app only ever talks to [`ENDPOINT`], and
    /// [`Claude::from_key`] refuses anything but HTTPS.
    #[cfg(test)]
    pub(crate) fn with_endpoint(key: &str, model: &str, endpoint: &str) -> Claude {
        Claude::build(key, model, endpoint, false)
    }
}

/// The host of an endpoint URL.
fn host_of(endpoint: &str) -> String {
    let rest = endpoint.split_once("://").map_or(endpoint, |(_, rest)| rest);
    rest.split('/').next().unwrap_or_default().to_owned()
}

impl Provider for Claude {
    fn disclosure(&self) -> Disclosure {
        Disclosure { recipient: host_of(&self.endpoint), model: self.model.clone(), leaves_machine: true, fallback_possible: true }
    }

    fn send(&mut self, request: &Request) -> Result<Reply, Error> {
        let body = build_body(request, &self.model)?;
        let (status, text) = self.post(&body)?;
        parse_reply(status, &text)
    }

    fn send_tools(&mut self, request: &ToolRequest) -> Result<Step, Error> {
        let body = build_tool_body(request, &self.model)?;
        let (status, text) = self.post(&body)?;
        parse_step(status, &text)
    }

    fn set_cancel(&mut self, flag: Arc<AtomicBool>) {
        self.cancel = Some(flag);
    }
}

impl Claude {
    /// POST `body` to the endpoint with the key, the version and the fallback beta; the status and
    /// the whole answer. A busy or failing service, and a connection that could not be made, are tried
    /// again up to [`RETRIES`] times, waiting as the service asks (sending again is safe: nothing runs
    /// until a whole answer has come); a spent usage limit is not. Once the request is let go
    /// ([`Provider::set_cancel`]) nothing is sent again: the wait ends and the answer is
    /// [`Error::Interrupted`].
    fn post(&self, body: &str) -> Result<(u16, String), Error> {
        let mut attempt = 0;
        loop {
            if self.cancelled() {
                return Err(Error::Interrupted);
            }
            let sent = self
                .agent
                .post(self.endpoint.as_str())
                .header("x-api-key", self.key.0.as_str())
                .header("anthropic-version", API_VERSION)
                .header("anthropic-beta", FALLBACK_BETA)
                .content_type("application/json")
                .send(body);
            let response = match sent {
                Ok(response) => response,
                Err(e) if attempt < RETRIES && never_sent(&e) => {
                    attempt += 1;
                    self.wait(backoff(attempt, None));
                    continue;
                }
                Err(e) => return Err(transport(e)),
            };
            let status = response.status().as_u16();
            let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
            let retry_after = header("retry-after").and_then(|v| v.trim().parse::<u64>().ok()).map(Duration::from_secs);
            let should_retry = header("x-should-retry");
            let mut answer = response.into_body();
            // The answer has begun: a failure now means it ended without one.
            let text = answer.with_config().limit(MAX_RESPONSE_BYTES).read_to_string().map_err(|e| match e {
                ureq::Error::Timeout(_) => Error::NoAnswerInTime,
                _ => Error::Interrupted,
            })?;
            if attempt < RETRIES && retryable(status, &text, should_retry.as_deref()) {
                attempt += 1;
                self.wait(backoff(attempt, retry_after));
                continue;
            }
            return Ok((status, text));
        }
    }

    /// Was the request let go?
    fn cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|flag| flag.load(Ordering::SeqCst))
    }

    /// Wait `how_long` before sending again, or less once the request is let go.
    fn wait(&self, how_long: Duration) {
        let until = Instant::now() + how_long;
        while !self.cancelled() {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            std::thread::sleep(left.min(CANCEL_CHECK));
        }
    }
}

/// A transport failure that happened before anything was sent: trying again is safe.
fn never_sent(e: &ureq::Error) -> bool {
    matches!(e, ureq::Error::ConnectionFailed | ureq::Error::HostNotFound | ureq::Error::Timeout(ureq::Timeout::Connect | ureq::Timeout::Resolve))
}

/// A transport failure, in the words the pane uses: a time limit after the request went out is not
/// "could not be reached".
fn transport(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Timeout(ureq::Timeout::Global | ureq::Timeout::PerCall | ureq::Timeout::PerRead) => Error::NoAnswerInTime,
        e => Error::Network(e.to_string()),
    }
}

/// Should an answer with `status` be asked for again? The service's `x-should-retry` decides when it is
/// there; else a timeout, a conflict, a busy service (429, except a spent usage limit) or a failing one
/// (5xx, 529 included).
fn retryable(status: u16, body: &str, should_retry: Option<&str>) -> bool {
    // A success is the answer, whatever a header says.
    if (200..300).contains(&status) {
        return false;
    }
    match should_retry {
        Some("true") => return true,
        Some("false") => return false,
        _ => {}
    }
    match status {
        408 | 409 | 500..=599 => true,
        429 => !body.contains("enforced_spend_limit_reached"),
        _ => false,
    }
}

/// How long to wait before attempt `attempt` again: what the service asked, else 1 s, then 2 s.
fn backoff(attempt: u32, retry_after: Option<Duration>) -> Duration {
    retry_after.unwrap_or_else(|| Duration::from_millis(500 << attempt.min(6))).min(MAX_RETRY_WAIT)
}

#[cfg(test)]
mod tests;
