//! Real-node evidence for the Storage backend: a round-trip against a running
//! Logos Storage (Codex) node — encrypt client-side, upload, download, decrypt.
//! Ignored by default (needs the node). Run with:
//!   docker run ... codexstorage/nim-codex ... storage ...   # REST on 127.0.0.1:8095
//!   cargo test -p logos_agent --test codex_live -- --ignored --nocapture

use logos_agent::storage::{CodexStorage, Storage};

#[tokio::test]
#[ignore = "requires a running Logos Storage (Codex) node on 127.0.0.1:8095"]
async fn codex_round_trip() {
    let storage = CodexStorage::new("http://127.0.0.1:8095", [9u8; 32]);

    let data = b"secret agent file";
    let cid = storage
        .upload("report", data)
        .await
        .expect("upload to Logos Storage");
    println!("stored CID: {cid}");
    assert!(!cid.is_empty());

    let retrieved = storage
        .download(&cid)
        .await
        .expect("download from Logos Storage");
    assert_eq!(
        retrieved, data,
        "file should round-trip: encrypted on upload, decrypted on download"
    );

    let listing = storage.list().await.unwrap();
    assert!(
        listing.iter().any(|(label, c)| label == "report" && c == &cid),
        "the stored object should appear in the agent's index"
    );
    println!("round-trip ok: {} bytes", retrieved.len());
}
