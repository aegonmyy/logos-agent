//! Real-node evidence for the Messaging backend: a round-trip against a running
//! nwaku node (not the in-memory stand-in). Ignored by default because it needs
//! an external node; run with:
//!   docker compose -f <compose> up -d   # nwaku on 127.0.0.1:8645
//!   cargo test -p logos_agent --test waku_live -- --ignored --nocapture

use std::time::Duration;

use logos_agent::messaging::{Messaging, WakuMessaging};

#[tokio::test]
#[ignore = "requires a running nwaku node on 127.0.0.1:8645"]
async fn waku_round_trip() {
    let waku = WakuMessaging::new("http://127.0.0.1:8645");

    // create_group subscribes the node to the derived content topic.
    let topic = waku
        .create_group(&["alice".to_owned(), "bob".to_owned()])
        .await
        .expect("create_group (subscribe) should succeed against nwaku");
    println!("topic: {topic}");

    // Give the subscription a moment to take effect.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let message = b"hello over waku";
    let id = match waku.send(&topic, message).await {
        Ok(id) => id,
        Err(error) => {
            // A standalone nwaku node has no relay mesh and reports
            // `NoPeersToPublish`, although v0.38 stores the message locally.
            // Keep this test useful for the REST adapter without disguising
            // failures unrelated to that single-node condition.
            let text = format!("{error:#}");
            assert!(
                text.contains("NoPeersToPublish"),
                "send (publish) should succeed against nwaku: {error:#}"
            );
            "local-store-fallback".to_owned()
        }
    };
    println!("published message id: {id}");
    assert!(!id.is_empty());

    // Let the message settle into the node's relay buffer.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let received = waku
        .poll(&topic)
        .await
        .expect("poll (read) should succeed against nwaku");
    println!("received {} message(s)", received.len());

    assert!(
        received.iter().any(|m| m.as_slice() == message),
        "the published message should round-trip through nwaku; got {received:?}"
    );
}
