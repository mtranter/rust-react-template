//! The bus over Redis or Valkey pub/sub (ADR 0039): ElastiCache Serverless
//! for Valkey in production, a Docker Compose Valkey in development.
//!
//! Cluster-wide `PUBLISH`/`SUBSCRIBE`, not sharded `SPUBLISH`/`SSUBSCRIBE`,
//! so a subscriber on any node of a cluster hears every message.

use crate::{Bus, Hearing};
use ::redis::aio::{ConnectionManager, ConnectionManagerConfig, PubSubSink, PubSubStream};
use ::redis::{Client, RedisError, RedisResult, TlsCertificates, Value};
use app_error::{AppError, AppResult};
use futures_util::stream::{self, BoxStream, StreamExt};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval, sleep, timeout};

const RECONNECT_AFTER: Duration = Duration::from_secs(1);
/// Longer than this to connect, or to answer, and Redis is unreachable.
const ANSWERED_WITHIN: Duration = Duration::from_secs(5);
/// How often a subscription checks its connection is alive, since one cut
/// without a word (a NAT or a failover dropping it) would never end.
const PING_EVERY: Duration = Duration::from_secs(15);

/// Connects only when it first publishes or subscribes, so a server whose
/// Redis is unreachable starts anyway: what it publishes fails, to be
/// retried, and its subscriptions keep reconnecting. `rediss://` connects
/// over TLS, as Valkey Serverless requires; the URL carries any credentials.
/// Publishing shares one connection; each subscription holds its own.
pub struct RedisBus {
    redis: Redis,
    publishing: OnceLock<ConnectionManager>,
}

/// Where the bus connects, and the CA (PEM) it trusts in place of the
/// public roots, if any.
#[derive(Clone)]
struct Redis {
    url: String,
    ca: Option<String>,
}

impl Redis {
    fn client(&self) -> RedisResult<Client> {
        match &self.ca {
            None => Client::open(self.url.as_str()),
            Some(ca) => Client::build_with_tls(
                self.url.as_str(),
                TlsCertificates {
                    client_tls: None,
                    root_cert: Some(ca.clone().into_bytes()),
                },
            ),
        }
    }
}

impl RedisBus {
    /// `ca` (PEM) replaces the public roots a `rediss://` certificate is
    /// checked against; `None` trusts them, as ElastiCache's need.
    pub fn new(url: String, ca: Option<String>) -> Self {
        Self {
            redis: Redis { url, ca },
            publishing: OnceLock::new(),
        }
    }

    fn publishing(&self) -> RedisResult<ConnectionManager> {
        if let Some(publishing) = self.publishing.get() {
            return Ok(publishing.clone());
        }
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(ANSWERED_WITHIN))
            .set_response_timeout(Some(ANSWERED_WITHIN))
            .set_number_of_retries(1);
        let connection = ConnectionManager::new_lazy_with_config(self.redis.client()?, config)?;
        Ok(self.publishing.get_or_init(|| connection).clone())
    }
}

#[async_trait::async_trait]
impl Bus for RedisBus {
    async fn publish(&self, channel: &str, payload: Vec<u8>) -> AppResult<()> {
        let mut publishing = self.publishing().map_err(AppError::internal)?;
        let _: Value = ::redis::cmd("PUBLISH")
            .arg(channel)
            .arg(payload)
            .query_async(&mut publishing)
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }

    fn subscribe(&self, channel: &str) -> BoxStream<'static, Hearing<Vec<u8>>> {
        let (hearing, heard) = mpsc::channel(64);
        tokio::spawn(listen(self.redis.clone(), channel.to_owned(), hearing));
        Box::pin(stream::unfold(heard, |mut heard| async move {
            heard.recv().await.map(|hearing| (hearing, heard))
        }))
    }
}

type Hearings = mpsc::Sender<Hearing<Vec<u8>>>;

/// Listens until nothing hears it any longer, reconnecting whenever the
/// connection is lost or can't be made.
async fn listen(redis: Redis, channel: String, hearing: Hearings) {
    loop {
        match subscribed(&redis, &channel).await {
            Ok((sink, messages)) => {
                if hearing.send(Hearing::Listening).await.is_err() {
                    return;
                }
                match relay(sink, messages, &hearing).await {
                    Relayed::Unheard => return,
                    Relayed::Lost(err) => {
                        tracing::warn!(channel, error = %err, "lost a bus subscription");
                    }
                }
                if hearing.send(Hearing::Lost).await.is_err() {
                    return;
                }
            }
            Err(err) => tracing::warn!(channel, error = %err, "can't subscribe to the bus"),
        }
        tokio::select! {
            () = sleep(RECONNECT_AFTER) => {}
            () = hearing.closed() => return,
        }
    }
}

async fn subscribed(redis: &Redis, channel: &str) -> RedisResult<(PubSubSink, PubSubStream)> {
    let subscribing = async {
        let mut pubsub = redis.client()?.get_async_pubsub().await?;
        pubsub.subscribe(channel).await?;
        Ok(pubsub.split())
    };
    timeout(ANSWERED_WITHIN, subscribing)
        .await
        .unwrap_or_else(|_| Err(timed_out()))
}

enum Relayed {
    /// Nothing hears the subscription any longer.
    Unheard,
    Lost(RedisError),
}

async fn relay(mut sink: PubSubSink, mut messages: PubSubStream, hearing: &Hearings) -> Relayed {
    let mut pinging = interval(PING_EVERY);
    pinging.set_missed_tick_behavior(MissedTickBehavior::Delay);
    pinging.tick().await;
    loop {
        tokio::select! {
            message = messages.next() => {
                let Some(message) = message else {
                    return Relayed::Lost(closed());
                };
                let payload = message.get_payload_bytes().to_vec();
                if hearing.send(Hearing::Heard(payload)).await.is_err() {
                    return Relayed::Unheard;
                }
            }
            _ = pinging.tick() => {
                let ponged = timeout(ANSWERED_WITHIN, sink.ping::<Value>())
                    .await
                    .unwrap_or_else(|_| Err(timed_out()));
                if let Err(err) = ponged {
                    return Relayed::Lost(err);
                }
            }
            () = hearing.closed() => return Relayed::Unheard,
        }
    }
}

fn timed_out() -> RedisError {
    RedisError::from(std::io::Error::from(std::io::ErrorKind::TimedOut))
}

fn closed() -> RedisError {
    RedisError::from(std::io::Error::from(std::io::ErrorKind::ConnectionAborted))
}
