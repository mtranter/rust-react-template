use fake_noticeboard::{Noticeboard, Pin};
use reqwest::StatusCode;
use serde_json::json;

const TOKEN: &str = "nb_test_0123456789";

#[tokio::test]
async fn it_keeps_what_is_pinned_and_answers_only_its_own_token() {
    let board = Noticeboard::new(TOKEN);
    let url = board.serve();
    let http = reqwest::Client::new();

    let refused = http.get(format!("{url}/health")).send().await.unwrap();
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    let healthy = http
        .get(format!("{url}/health"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(healthy.status(), StatusCode::OK);

    for title in ["First", "Second"] {
        let put = http
            .put(format!("{url}/pins/a"))
            .bearer_auth(TOKEN)
            .json(&json!({ "title": title, "body": "b" }))
            .send()
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::OK);
    }

    let pin: Pin = http
        .get(format!("{url}/pins/a"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(pin.title, "Second");
    assert_eq!(board.pins().len(), 1);
    assert_eq!(board.puts(), 2);
}
