//! The server's shutdown, as long-lived responses see it: a stream ends when
//! the server begins to drain, so a graceful shutdown isn't held open by a
//! client that never hangs up.

use futures_util::{Stream, StreamExt};
use std::sync::Arc;
use tokio::sync::watch;

/// Whether the server has begun to shut down. Clones share it; one a server
/// never drains (a test's) never begins.
#[derive(Clone)]
pub struct Draining(Arc<watch::Sender<bool>>);

impl Default for Draining {
    fn default() -> Self {
        Self(Arc::new(watch::Sender::new(false)))
    }
}

impl Draining {
    pub fn begin(&self) {
        self.0.send_replace(true);
    }

    /// Resolves once the server has begun to drain.
    pub async fn begun(&self) {
        let mut draining = self.0.subscribe();
        // The sender lives as long as `self`, so this only ends by draining.
        let _ = draining.wait_for(|begun| *begun).await;
    }

    /// `stream`, ending early if the server begins to drain.
    pub fn bounding<S: Stream + Send + 'static>(
        &self,
        stream: S,
    ) -> impl Stream<Item = S::Item> + Send + 'static {
        let draining = self.clone();
        stream.take_until(async move { draining.begun().await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    #[tokio::test]
    async fn a_stream_ends_when_the_server_begins_to_drain() {
        let draining = Draining::default();
        let mut bounded =
            Box::pin(draining.bounding(stream::iter([1, 2]).chain(stream::pending())));
        assert_eq!(bounded.next().await, Some(1));
        assert_eq!(bounded.next().await, Some(2));
        draining.begin();
        assert_eq!(bounded.next().await, None);
    }
}
