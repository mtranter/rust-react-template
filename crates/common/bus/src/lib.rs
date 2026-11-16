//! Broadcasts between the instances of a load-balanced app (ADR 0039), so
//! each can tell the clients it holds (an SSE stream, a WebSocket) what
//! happened on another. Best effort: a message published while an instance
//! is disconnected is lost to it, and it is told only that it may have
//! missed some ([`Received::Gap`]). A consumer that must not lose anything
//! keeps the truth elsewhere, usually the database, and treats a gap as the
//! cue to recheck it.

mod hub;
mod redis;
#[cfg(feature = "test")]
pub mod testing;

pub use hub::{Follower, Hub, Received};
pub use redis::RedisBus;

use app_error::{AppError, AppResult};
use futures_util::stream::{BoxStream, StreamExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::marker::PhantomData;
use std::sync::Arc;

/// Carries bytes on named channels to every instance subscribed, whichever
/// published them. No durability, order or delivery is promised.
#[async_trait::async_trait]
pub trait Bus: Send + Sync {
    /// Sends at once, not once a unit of work commits.
    async fn publish(&self, channel: &str, payload: Vec<u8>) -> AppResult<()>;
    /// What this instance hears on `channel` from now on, reconnecting
    /// whenever its connection is lost, until the stream is dropped.
    fn subscribe(&self, channel: &str) -> BoxStream<'static, Hearing<Vec<u8>>>;
}

/// What a subscription hears: its messages, and when it can't, so that a
/// consumer can tell what it may have missed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hearing<T> {
    /// Connected: every message sent from now on is heard.
    Listening,
    Heard(T),
    /// Disconnected: messages sent until it listens again go unheard.
    Lost,
    /// A message that isn't a `T`, from a release that changed it
    /// incompatibly: only a [`Channel`] hears one.
    Unreadable,
}

/// The bus of a process that broadcasts nothing (a migration, a test that
/// opens no stream): it carries nothing and never listens.
pub struct NoBus;

#[async_trait::async_trait]
impl Bus for NoBus {
    async fn publish(&self, _channel: &str, _payload: Vec<u8>) -> AppResult<()> {
        Ok(())
    }

    fn subscribe(&self, _channel: &str) -> BoxStream<'static, Hearing<Vec<u8>>> {
        futures_util::stream::pending().boxed()
    }
}

/// One channel of a [`Bus`], carrying `T` as JSON. Instances on different
/// releases share it during a deploy, so `T` changes only compatibly, as an
/// event does; a message an instance can't read is heard as
/// [`Hearing::Unreadable`], so its followers learn they missed something.
pub struct Channel<T> {
    bus: Arc<dyn Bus>,
    name: &'static str,
    carries: PhantomData<fn() -> T>,
}

impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self {
        Self {
            bus: self.bus.clone(),
            name: self.name,
            carries: PhantomData,
        }
    }
}

impl<T: Serialize + DeserializeOwned + Send + 'static> Channel<T> {
    /// `name` is shared by every instance and release, so it is never reused
    /// for another `T`. Prefix it with the module that owns it.
    pub fn new(bus: Arc<dyn Bus>, name: &'static str) -> Self {
        Self {
            bus,
            name,
            carries: PhantomData,
        }
    }

    pub async fn publish(&self, message: &T) -> AppResult<()> {
        let payload = serde_json::to_vec(message).map_err(AppError::internal)?;
        self.bus.publish(self.name, payload).await
    }

    /// Each call is a subscription of its own: a process fanning one out to
    /// many followers uses a [`Hub`].
    pub fn subscribe(&self) -> BoxStream<'static, Hearing<T>> {
        let name = self.name;
        self.bus
            .subscribe(name)
            .filter_map(move |hearing| async move {
                match hearing {
                    Hearing::Listening => Some(Hearing::Listening),
                    Hearing::Lost => Some(Hearing::Lost),
                    Hearing::Unreadable => Some(Hearing::Unreadable),
                    Hearing::Heard(payload) => match serde_json::from_slice(&payload) {
                        Ok(message) => Some(Hearing::Heard(message)),
                        Err(err) => {
                            tracing::warn!(channel = name, error = %err, "an unreadable message");
                            Some(Hearing::Unreadable)
                        }
                    },
                }
            })
            .boxed()
    }
}
