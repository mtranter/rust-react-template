//! `fake-noticeboard --token <token> [--port <port>]`: serves the noticeboard on 127.0.0.1, port
//! 0 picking a free one, and prints its URL as its first line of output once it is listening.

use fake_noticeboard::Noticeboard;
use poem::listener::{Acceptor, Listener, TcpListener};
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut token = None;
    let mut port = "0".to_owned();
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        let value = argv
            .next()
            .ok_or_else(|| format!("`{flag}` needs a value"))?;
        match flag.as_str() {
            "--token" => token = Some(value),
            "--port" => port = value,
            other => return Err(format!("unexpected argument `{other}`").into()),
        }
    }
    let token = token.ok_or("missing --token")?;

    let acceptor = TcpListener::bind(format!("127.0.0.1:{port}"))
        .into_acceptor()
        .await?;
    let addr = acceptor
        .local_addr()
        .into_iter()
        .find_map(|addr| addr.as_socket_addr().copied())
        .ok_or("no local address")?;
    println!("http://{addr}");
    std::io::stdout().flush()?;
    poem::Server::new_with_acceptor(acceptor)
        .run(Noticeboard::new(token).endpoint())
        .await?;
    Ok(())
}
