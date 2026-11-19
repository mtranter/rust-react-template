//! `fake-oidc --client-id <id> --subject <sub> --name <name> --email <email> [--port <port>]`:
//! serves the provider on 127.0.0.1, port 0 picking a free one, and prints its
//! issuer as its first line of output once it is listening.

use app_fake_oidc::{Provider, Subject};
use poem::{
    Server,
    listener::{Acceptor, Listener, TcpListener},
};
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: HashMap<String, String> = HashMap::new();
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        let name = flag
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument `{flag}`"))?;
        let value = argv
            .next()
            .ok_or_else(|| format!("`{flag}` needs a value"))?;
        args.insert(name.to_string(), value);
    }
    let mut arg = |name: &str| args.remove(name).ok_or_else(|| format!("missing --{name}"));
    let client_id = arg("client-id")?;
    let subject = Subject {
        sub: arg("subject")?,
        name: arg("name")?,
        email: arg("email")?,
    };
    let port = arg("port").unwrap_or_else(|_| "0".to_string());

    let acceptor = TcpListener::bind(format!("127.0.0.1:{port}"))
        .into_acceptor()
        .await?;
    let addr = acceptor
        .local_addr()
        .into_iter()
        .find_map(|addr| addr.as_socket_addr().copied())
        .ok_or("no local address")?;
    let provider = Arc::new(Provider::new(format!("http://{addr}"), client_id, subject));
    println!("{}", provider.issuer());
    std::io::stdout().flush()?;
    Server::new_with_acceptor(acceptor)
        .run(provider.endpoint())
        .await?;
    Ok(())
}
