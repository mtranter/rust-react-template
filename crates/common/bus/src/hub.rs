//! One subscription per instance, fanned out to every client it holds.

use crate::{Channel, Hearing};
use futures_util::stream::{BoxStream, StreamExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

/// Messages a slow follower may fall behind by before it is told of a gap.
const BACKLOG: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Received<T> {
    Message(T),
    /// Messages may have gone unheard: the subscription was disconnected,
    /// a message couldn't be read, the follower fell behind, or it began
    /// following before the subscription first listened.
    Gap,
}

/// Subscribes to its channel once something first follows it, so a process
/// that only publishes (a worker) holds no subscription.
pub struct Hub<T> {
    channel: Channel<T>,
    listening_within: Duration,
    started: OnceLock<Started<T>>,
}

impl<T> Hub<T>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    /// Following waits up to 5 s for the subscription to listen.
    pub fn new(channel: Channel<T>) -> Self {
        Self {
            channel,
            listening_within: Duration::from_secs(5),
            started: OnceLock::new(),
        }
    }

    /// How long following waits for the subscription, which starts with the
    /// first follower, before going ahead to be told of a gap once it
    /// listens. Zero never waits: for a follower that can begin by fetching
    /// the truth, so that what it would have heard meanwhile doesn't matter.
    pub fn with_listening_within(self, listening_within: Duration) -> Self {
        Self {
            listening_within,
            ..self
        }
    }

    /// The channel it follows, to publish on.
    pub fn channel(&self) -> &Channel<T> {
        &self.channel
    }

    /// Everything heard from when this returns.
    pub async fn follow(&self) -> Follower<T> {
        let started = self
            .started
            .get_or_init(|| Started::start(self.channel.subscribe()));
        let received = started.received.subscribe();
        let mut listening = started.listening.clone();
        if tokio::time::timeout(
            self.listening_within,
            listening.wait_for(|listening| *listening),
        )
        .await
        .is_err()
        {
            tracing::warn!(
                channel = self.channel.name,
                "following before the subscription could listen"
            );
            started.unheard.store(true, Ordering::SeqCst);
            // The subscription may have connected since it last looked.
            if *listening.borrow() && started.unheard.swap(false, Ordering::SeqCst) {
                let _ = started.received.send(Received::Gap);
            }
        }
        Follower(received)
    }
}

pub struct Follower<T>(broadcast::Receiver<Received<T>>);

impl<T: Clone> Follower<T> {
    /// `None` once the hub has stopped.
    pub async fn next(&mut self) -> Option<Received<T>> {
        match self.0.recv().await {
            Ok(received) => Some(received),
            Err(broadcast::error::RecvError::Lagged(_)) => Some(Received::Gap),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}

struct Started<T> {
    received: broadcast::Sender<Received<T>>,
    listening: watch::Receiver<bool>,
    /// A follower stopped waiting for the subscription, so it is to be told
    /// of a gap once the subscription listens.
    unheard: Arc<AtomicBool>,
    relay: JoinHandle<()>,
}

impl<T: Clone + Send + 'static> Started<T> {
    fn start(hearing: BoxStream<'static, Hearing<T>>) -> Self {
        let (received, _) = broadcast::channel(BACKLOG);
        let (listening, listening_rx) = watch::channel(false);
        let unheard = Arc::new(AtomicBool::new(false));
        let relay = tokio::spawn(relay(hearing, received.clone(), listening, unheard.clone()));
        Self {
            received,
            listening: listening_rx,
            unheard,
            relay,
        }
    }
}

impl<T> Drop for Started<T> {
    fn drop(&mut self) {
        self.relay.abort();
    }
}

/// Messages sent while disconnected are lost, so every follower is told of
/// a gap once the subscription listens again, as is every follower that
/// stopped waiting for it to listen first.
async fn relay<T: Clone>(
    mut hearing: BoxStream<'static, Hearing<T>>,
    received: broadcast::Sender<Received<T>>,
    listening: watch::Sender<bool>,
    unheard: Arc<AtomicBool>,
) {
    let mut reconnecting = false;
    while let Some(heard) = hearing.next().await {
        match heard {
            Hearing::Listening => {
                listening.send_replace(true);
                if unheard.swap(false, Ordering::SeqCst) || reconnecting {
                    let _ = received.send(Received::Gap);
                }
            }
            // No follower is no one to tell.
            Hearing::Heard(message) => {
                let _ = received.send(Received::Message(message));
            }
            Hearing::Unreadable => {
                let _ = received.send(Received::Gap);
            }
            Hearing::Lost => {
                listening.send_replace(false);
                reconnecting = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bus;
    use app_error::AppResult;
    use std::sync::Mutex;
    use tokio::sync::mpsc;

    /// A bus whose one subscription the test drives by hand.
    struct ScriptedBus(Mutex<Option<mpsc::UnboundedReceiver<Hearing<Vec<u8>>>>>);

    #[async_trait::async_trait]
    impl Bus for ScriptedBus {
        async fn publish(&self, _channel: &str, _payload: Vec<u8>) -> AppResult<()> {
            Ok(())
        }

        fn subscribe(&self, _channel: &str) -> BoxStream<'static, Hearing<Vec<u8>>> {
            let heard = self.0.lock().unwrap().take().expect("one subscription");
            futures_util::stream::unfold(heard, |mut heard| async move {
                heard.recv().await.map(|hearing| (hearing, heard))
            })
            .boxed()
        }
    }

    fn hub() -> (Hub<u32>, mpsc::UnboundedSender<Hearing<Vec<u8>>>) {
        let (hear, heard) = mpsc::unbounded_channel();
        let bus = Arc::new(ScriptedBus(Mutex::new(Some(heard))));
        (Hub::new(Channel::new(bus, "test")), hear)
    }

    fn message(n: u32) -> Hearing<Vec<u8>> {
        Hearing::Heard(serde_json::to_vec(&n).unwrap())
    }

    #[tokio::test]
    async fn every_follower_receives_each_message() {
        let (hub, hear) = hub();
        hear.send(Hearing::Listening).unwrap();
        let mut first = hub.follow().await;
        let mut second = hub.follow().await;
        hear.send(message(7)).unwrap();
        assert_eq!(first.next().await, Some(Received::Message(7)));
        assert_eq!(second.next().await, Some(Received::Message(7)));
    }

    #[tokio::test]
    async fn followers_are_told_of_a_gap_once_the_subscription_reconnects() {
        let (hub, hear) = hub();
        hear.send(Hearing::Listening).unwrap();
        let mut follower = hub.follow().await;
        hear.send(Hearing::Lost).unwrap();
        hear.send(Hearing::Listening).unwrap();
        hear.send(message(1)).unwrap();
        assert_eq!(follower.next().await, Some(Received::Gap));
        assert_eq!(follower.next().await, Some(Received::Message(1)));
    }

    #[tokio::test]
    async fn followers_are_told_of_a_gap_where_a_message_could_not_be_read() {
        let (hub, hear) = hub();
        hear.send(Hearing::Listening).unwrap();
        let mut follower = hub.follow().await;
        hear.send(Hearing::Heard(b"not a number".to_vec())).unwrap();
        hear.send(message(2)).unwrap();
        assert_eq!(follower.next().await, Some(Received::Gap));
        assert_eq!(follower.next().await, Some(Received::Message(2)));
    }

    #[tokio::test(start_paused = true)]
    async fn a_follower_that_need_not_wait_follows_at_once() {
        let (hub, hear) = hub();
        let hub = hub.with_listening_within(Duration::ZERO);
        let following = tokio::time::Instant::now();
        let mut follower = hub.follow().await;
        assert_eq!(following.elapsed(), Duration::ZERO);
        hear.send(Hearing::Listening).unwrap();
        assert_eq!(follower.next().await, Some(Received::Gap));
    }

    #[tokio::test(start_paused = true)]
    async fn a_follower_that_stopped_waiting_is_told_of_a_gap_once_it_listens() {
        let (hub, hear) = hub();
        let mut follower = hub.follow().await;
        hear.send(Hearing::Listening).unwrap();
        assert_eq!(follower.next().await, Some(Received::Gap));
    }

    #[tokio::test(start_paused = true)]
    async fn a_follower_that_falls_behind_is_told_of_a_gap() {
        let (hub, hear) = hub();
        hear.send(Hearing::Listening).unwrap();
        let mut follower = hub.follow().await;
        for n in 0..=BACKLOG as u32 {
            hear.send(message(n)).unwrap();
        }
        // Paused time advances only once every task is idle: the relay has
        // passed every message on.
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(follower.next().await, Some(Received::Gap));
    }
}
