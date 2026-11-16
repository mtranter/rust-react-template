//! The bus end to end, over a real Valkey: two `RedisBus`es stand for two
//! instances of the app.

use app_bus::testing::Valkey;
use app_bus::{Channel, Follower, Hub, Received};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

const CHANNEL: &str = "tests_greetings";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Greeting {
    from: String,
}

fn greeting(from: &str) -> Greeting {
    Greeting {
        from: from.to_string(),
    }
}

async fn instance(valkey: &Valkey) -> Hub<Greeting> {
    Hub::new(Channel::new(Arc::new(valkey.bus().await), CHANNEL))
}

async fn next(follower: &mut Follower<Greeting>) -> Received<Greeting> {
    tokio::time::timeout(Duration::from_secs(10), follower.next())
        .await
        .expect("received within 10 s")
        .expect("the hub still running")
}

#[tokio::test]
async fn every_instance_hears_what_any_instance_publishes() {
    let valkey = Valkey::start().await;
    let (one, other) = (instance(&valkey).await, instance(&valkey).await);
    let mut on_one = one.follow().await;
    let mut on_other = other.follow().await;

    one.channel().publish(&greeting("one")).await.unwrap();

    assert_eq!(next(&mut on_one).await, Received::Message(greeting("one")));
    assert_eq!(
        next(&mut on_other).await,
        Received::Message(greeting("one"))
    );
}

#[tokio::test]
async fn it_works_over_tls_with_a_private_ca() {
    let valkey = Valkey::start_tls().await;
    let (one, other) = (instance(&valkey).await, instance(&valkey).await);
    let mut on_other = other.follow().await;

    one.channel().publish(&greeting("one")).await.unwrap();

    assert_eq!(
        next(&mut on_other).await,
        Received::Message(greeting("one"))
    );
}

#[tokio::test]
async fn followers_are_told_of_a_gap_once_a_lost_subscription_reconnects() {
    let valkey = Valkey::start().await;
    let hub = instance(&valkey).await;
    let mut follower = hub.follow().await;

    assert_eq!(valkey.end_subscribers().await, 1);

    assert_eq!(next(&mut follower).await, Received::Gap);
    hub.channel().publish(&greeting("after")).await.unwrap();
    assert_eq!(
        next(&mut follower).await,
        Received::Message(greeting("after"))
    );
}

#[tokio::test]
async fn followers_are_told_of_a_gap_where_a_message_could_not_be_read() {
    let valkey = Valkey::start().await;
    let hub = instance(&valkey).await;
    let mut follower = hub.follow().await;

    valkey.publish_raw(CHANNEL, b"{\"to\":\"nobody\"}").await;
    hub.channel().publish(&greeting("after")).await.unwrap();

    assert_eq!(next(&mut follower).await, Received::Gap);
    assert_eq!(
        next(&mut follower).await,
        Received::Message(greeting("after"))
    );
}

#[tokio::test]
async fn publishing_is_refused_while_the_bus_refuses_it() {
    let valkey = Valkey::start().await;
    let hub = instance(&valkey).await;

    valkey.allow_publishing(false).await;

    assert!(hub.channel().publish(&greeting("one")).await.is_err());
}
