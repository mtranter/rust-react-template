//! A Valkey of a test's own, in Docker, as production's ElastiCache
//! Serverless for Valkey (ADR 0039): reached as a user of its own, as a
//! deployment is, and over TLS only if asked.

use crate::RedisBus;
use ::redis::aio::MultiplexedConnection;
use ::redis::{Client, RedisResult, TlsCertificates};
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use std::time::Duration;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

const USER: &str = "app";
const PASSWORD: &str = "a-test-password";

/// Runs while it is held.
pub struct Valkey {
    container: ContainerAsync<GenericImage>,
    /// The PEM of the CA its certificate is signed by, if it speaks only TLS.
    tls_ca: Option<String>,
}

impl Valkey {
    /// Lets the app's user in, as [`Self::start_unadmitted`] doesn't.
    pub async fn start() -> Self {
        let valkey = Self::start_unadmitted().await;
        valkey.admit().await;
        valkey
    }

    /// Refuses the app's user until [`Self::admit`].
    pub async fn start_unadmitted() -> Self {
        Self::reachable(async || {
            let container = GenericImage::new("valkey/valkey", "8-alpine")
                .with_exposed_port(6379.tcp())
                .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
                .start()
                .await
                .expect("start Valkey (is Docker running?)");
            Self {
                container,
                tls_ca: None,
            }
        })
        .await
    }

    /// As [`Self::start`], speaking only TLS, as ElastiCache Serverless for
    /// Valkey does, with a certificate from a CA made for the test.
    pub async fn start_tls() -> Self {
        let valkey = Self::reachable(async || Self::start_tls_once().await).await;
        valkey.admit().await;
        valkey
    }

    /// Starts a Valkey with `start` until one answers. Docker Desktop now and
    /// then publishes a running container's port without ever forwarding it,
    /// or never publishes it, most often with many containers starting at
    /// once; a fresh container gets a fresh port.
    async fn reachable(start: impl AsyncFn() -> Self) -> Self {
        for _ in 0..3 {
            let valkey = start().await;
            for _ in 0..20 {
                if valkey.try_admin().await.is_ok() {
                    return valkey;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            eprintln!("Valkey refused every connection; starting another");
        }
        panic!("reach Valkey: three containers in a row refused every connection");
    }

    async fn start_tls_once() -> Self {
        let certificates = TestCertificates::new();
        let container = GenericImage::new("valkey/valkey", "8-alpine")
            .with_exposed_port(6379.tcp())
            .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
            .with_copy_to("/tls/ca.crt", certificates.ca.clone().into_bytes())
            .with_copy_to("/tls/valkey.crt", certificates.server.into_bytes())
            .with_copy_to("/tls/valkey.key", certificates.server_key.into_bytes())
            .with_cmd([
                "valkey-server",
                "--port",
                "0",
                "--tls-port",
                "6379",
                "--tls-cert-file",
                "/tls/valkey.crt",
                "--tls-key-file",
                "/tls/valkey.key",
                "--tls-ca-cert-file",
                "/tls/ca.crt",
                "--tls-auth-clients",
                "no",
            ])
            .start()
            .await
            .expect("start Valkey over TLS (is Docker running?)");
        Self {
            container,
            tls_ca: Some(certificates.ca),
        }
    }

    /// A bus of an instance of the app, reaching this Valkey as its user.
    pub async fn bus(&self) -> RedisBus {
        RedisBus::new(self.url().await, self.tls_ca.clone())
    }

    /// Where the app reaches it, as its user.
    pub async fn url(&self) -> String {
        format!(
            "{}://{USER}:{PASSWORD}@127.0.0.1:{}",
            self.scheme(),
            self.port().await
        )
    }

    /// The CA (PEM) its certificate is checked against, if it speaks TLS.
    pub fn tls_ca(&self) -> Option<String> {
        self.tls_ca.clone()
    }

    /// Lets the app's user in.
    pub async fn admit(&self) {
        let _: () = ::redis::cmd("ACL")
            .arg(&["SETUSER", USER, "on"])
            .arg(format!(">{PASSWORD}"))
            .arg(&["~*", "&*", "+@all"])
            .query_async(&mut self.admin().await)
            .await
            .expect("add the app's user");
    }

    /// Lets the app's user publish, or refuses it, while its subscriptions
    /// stay up.
    pub async fn allow_publishing(&self, allowed: bool) {
        let _: () = ::redis::cmd("ACL")
            .arg(&[
                "SETUSER",
                USER,
                if allowed { "+publish" } else { "-publish" },
            ])
            .query_async(&mut self.admin().await)
            .await
            .expect("change what the app's user may do");
    }

    /// Ends every subscriber's connection, returning how many there were.
    pub async fn end_subscribers(&self) -> usize {
        ::redis::cmd("CLIENT")
            .arg(&["KILL", "TYPE", "pubsub"])
            .query_async(&mut self.admin().await)
            .await
            .expect("end Valkey's subscribers")
    }

    /// Publishes `payload` on `channel` as is, bypassing any `Channel`.
    pub async fn publish_raw(&self, channel: &str, payload: &[u8]) {
        let _: () = ::redis::cmd("PUBLISH")
            .arg(channel)
            .arg(payload)
            .query_async(&mut self.admin().await)
            .await
            .expect("publish to Valkey");
    }

    async fn port(&self) -> u16 {
        self.container
            .get_host_port_ipv4(6379)
            .await
            .expect("Valkey's port")
    }

    fn scheme(&self) -> &'static str {
        if self.tls_ca.is_some() {
            "rediss"
        } else {
            "redis"
        }
    }

    async fn admin(&self) -> MultiplexedConnection {
        self.try_admin().await.expect("reach Valkey")
    }

    /// As the default user, whom a fresh Valkey admits with no password.
    async fn try_admin(&self) -> RedisResult<MultiplexedConnection> {
        let port = self
            .container
            .get_host_port_ipv4(6379)
            .await
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        let url = format!("{}://127.0.0.1:{port}", self.scheme());
        crate::redis::install_crypto_provider();
        let client = match &self.tls_ca {
            None => Client::open(url),
            Some(ca) => Client::build_with_tls(
                url,
                TlsCertificates {
                    client_tls: None,
                    root_cert: Some(ca.clone().into_bytes()),
                },
            ),
        };
        client?.get_multiplexed_async_connection().await
    }
}

/// A CA, and a certificate it signed for a server at `127.0.0.1`, as PEM.
struct TestCertificates {
    ca: String,
    server: String,
    server_key: String,
}

impl TestCertificates {
    fn new() -> Self {
        let ca_key = KeyPair::generate().expect("a CA key");
        let mut ca = CertificateParams::new(Vec::<String>::new()).expect("a CA");
        ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_certificate = ca.self_signed(&ca_key).expect("a CA certificate");
        let issuer = Issuer::new(ca, ca_key);
        let server_key = KeyPair::generate().expect("a server key");
        let server = CertificateParams::new(["127.0.0.1".to_string()])
            .expect("a server")
            .signed_by(&server_key, &issuer)
            .expect("a server certificate");
        Self {
            ca: ca_certificate.pem(),
            server: server.pem(),
            server_key: server_key.serialize_pem(),
        }
    }
}
