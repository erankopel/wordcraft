//! The Claude provider: the Messages API over HTTPS with the person's own key.
//!
//! The key comes from [`KEY_ENV`] and lives only in this struct's memory: it is sent in one header,
//! never printed (its `Debug` and every error leave it out), never saved, never put in a document.
//! Redirects are not followed, so the key cannot be sent to a host the endpoint did not name.

use std::fmt;
use std::time::Duration;

use crate::{
    API_VERSION, DEFAULT_MODEL, Disclosure, ENDPOINT, Error, FALLBACK_BETA, KEY_ENV, MAX_RESPONSE_BYTES, Provider, Reply, Request, build_body,
    parse_reply,
};

/// How long one request may take, in all. A reply is read whole, and a long one takes minutes.
const TIMEOUT: Duration = Duration::from_secs(600);

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
        let key = key.map(str::trim).filter(|k| !k.is_empty()).ok_or(Error::MissingKey)?;
        Ok(Claude::with_endpoint(key, DEFAULT_MODEL, ENDPOINT))
    }

    /// Whether [`KEY_ENV`] holds a key, for the pane to say so. The key itself is not read out.
    pub fn key_present() -> bool {
        std::env::var(KEY_ENV).is_ok_and(|k| !k.trim().is_empty())
    }

    /// A provider that sends to `endpoint`. Private on purpose: the app only ever talks to
    /// [`ENDPOINT`], and the tests point it at a server on this machine. A plain `http` endpoint
    /// is allowed here, so it must never be reachable from outside the crate.
    pub(crate) fn with_endpoint(key: &str, model: &str, endpoint: &str) -> Claude {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .https_only(endpoint.starts_with("https://"))
            .timeout_global(Some(TIMEOUT))
            .build();
        Claude { key: Secret(key.to_owned()), model: model.to_owned(), endpoint: endpoint.to_owned(), agent: ureq::Agent::new_with_config(config) }
    }
}

/// The host of an endpoint URL.
fn host_of(endpoint: &str) -> String {
    let rest = endpoint.split_once("://").map_or(endpoint, |(_, rest)| rest);
    rest.split('/').next().unwrap_or_default().to_owned()
}

impl Provider for Claude {
    fn disclosure(&self) -> Disclosure {
        Disclosure { recipient: host_of(&self.endpoint), model: self.model.clone(), leaves_machine: true }
    }

    fn send(&mut self, request: &Request) -> Result<Reply, Error> {
        let body = build_body(request, &self.model)?;
        let response = self
            .agent
            .post(self.endpoint.as_str())
            .header("x-api-key", self.key.0.as_str())
            .header("anthropic-version", API_VERSION)
            .header("anthropic-beta", FALLBACK_BETA)
            .content_type("application/json")
            .send(body.as_str())
            .map_err(|e| Error::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let mut answer = response.into_body();
        let text = answer.with_config().limit(MAX_RESPONSE_BYTES).read_to_string().map_err(|e| Error::Network(e.to_string()))?;
        parse_reply(status, &text)
    }
}

#[cfg(test)]
mod tests;
