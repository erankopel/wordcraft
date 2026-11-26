//! A provider that answers from a list. It sends nothing anywhere: for tests, and for trying the
//! assistant without a key.

use std::collections::VecDeque;

use crate::{Disclosure, Error, Provider, Reply, Request, Stop, Usage};

/// Answers each request with the next canned result, and remembers what it was asked.
#[derive(Default)]
pub struct MockProvider {
    replies: VecDeque<Result<Reply, Error>>,
    seen: Vec<Request>,
}

impl MockProvider {
    pub fn new() -> MockProvider {
        MockProvider::default()
    }

    /// Queue a reply that ended normally with `text`.
    pub fn push_text(&mut self, text: impl Into<String>) -> &mut MockProvider {
        let reply = Reply { text: text.into(), stop: Stop::End, usage: Usage::default(), model: "mock".into(), fallback_used: false };
        self.replies.push_back(Ok(reply));
        self
    }

    /// Queue a result of any kind: a refusal, a cut-off reply, an error.
    pub fn push(&mut self, result: Result<Reply, Error>) -> &mut MockProvider {
        self.replies.push_back(result);
        self
    }

    /// The requests it was sent, oldest first.
    pub fn requests(&self) -> &[Request] {
        &self.seen
    }
}

impl Provider for MockProvider {
    fn disclosure(&self) -> Disclosure {
        Disclosure { recipient: "this computer".into(), model: "mock".into(), leaves_machine: false }
    }

    fn send(&mut self, request: &Request) -> Result<Reply, Error> {
        self.seen.push(request.clone());
        self.replies.pop_front().unwrap_or_else(|| Err(Error::Network("the mock has no reply left".into())))
    }
}
