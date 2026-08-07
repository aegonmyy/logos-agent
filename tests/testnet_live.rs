//! Real-testnet evidence: point an agent wallet at the live public LEZ testnet
//! and read its chain state. Ignored by default (needs the network). Run with:
//!   cargo test -p logos_agent --test testnet_live -- --ignored --nocapture

use std::time::Duration;

use wallet::WalletCore;
use wallet::config::WalletConfigOverrides;

const TESTNET: &str = "https://seq-testnet.paradox.computer";

#[tokio::test]
#[ignore = "hits the live public LEZ testnet"]
async fn agent_wallet_reaches_live_testnet() {
    let dir = std::env::temp_dir().join(format!("agent-testnet-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // The public endpoint flaps (intermittent nginx 502s), so retry patiently.
    let overrides = WalletConfigOverrides {
        sequencer_addr: Some(TESTNET.parse().unwrap()),
        seq_poll_max_retries: Some(2000),
        seq_poll_timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    };
    let (mut wallet, _mnemonic) = WalletCore::new_init_storage(
        dir.join("config.json"),
        dir.join("storage"),
        Some(overrides),
        "testpw",
    )
    .expect("build wallet pointed at the testnet");

    // Reaching the live sequencer and reading its chain height proves the agent
    // wallet is talking to the real testnet, not a local sequencer.
    let block = wallet
        .sync_to_latest_block()
        .await
        .expect("sync against the live testnet");
    println!("live testnet latest block id: {block}");
    assert!(block >= 1, "should read a real block from the live testnet");

    let _ = std::fs::remove_dir_all(&dir);
}
