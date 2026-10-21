//! A `Gh` answering from canned replies, for the tests of any module.

use std::path::Path;
use std::sync::Mutex;

use futures_util::future::BoxFuture;

use super::{Gh, GhError, strings};

type Reply = (Vec<String>, Result<String, GhError>);

pub(crate) struct FakeGh {
    replies: Mutex<Vec<Reply>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl FakeGh {
    pub(crate) fn new() -> Self {
        Self {
            replies: Mutex::default(),
            calls: Mutex::default(),
        }
    }

    /// Answers calls whose arguments start with `prefix` (the first match wins).
    pub(crate) fn on(self, prefix: &[&str], reply: Result<&str, GhError>) -> Self {
        self.replies
            .lock()
            .unwrap()
            .push((strings(prefix), reply.map(str::to_string)));
        self
    }

    /// Every call made, in order.
    pub(crate) fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl Gh for FakeGh {
    fn run(
        &self,
        _dir: Option<&Path>,
        args: Vec<String>,
        _stdin: Option<String>,
    ) -> BoxFuture<'_, Result<String, GhError>> {
        let reply = self
            .replies
            .lock()
            .unwrap()
            .iter()
            .find(|(prefix, _)| args.starts_with(prefix))
            .map(|(_, r)| r.clone());
        let unsupported = format!("fake gh: unsupported: {}", args.join(" "));
        self.calls.lock().unwrap().push(args);
        Box::pin(async move { reply.unwrap_or_else(|| Err(GhError::failed(unsupported))) })
    }
}
