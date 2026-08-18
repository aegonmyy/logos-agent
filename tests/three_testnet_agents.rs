//! Reproducible public-testnet evidence for the three LP-0008 category agents.
//!
//! This test intentionally uses the public LEZ endpoint rather than the local
//! `TestContext` fixture. It creates three independent shielded agent identities
//! and records their account ids. Storage and Messaging operations require the
//! corresponding real service endpoints and are exercised when configured.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use logos_agent::messaging::{Messaging, WakuMessaging};
use logos_agent::storage::{CodexStorage, Storage};
use logos_agent::{Agent, SpendingPolicy};
use wallet::WalletCore;
use wallet::config::{SequencerConnectionData, WalletConfigOverrides};

fn testnet_url() -> String {
    env::var("AGENT_TESTNET_URL").unwrap_or_else(|_| "https://testnet.lez.logos.co".to_owned())
}

async fn new_wallet() -> Result<WalletCore> {
    let dir = PathBuf::from(
        env::var("AGENT_TESTNET_STATE_DIR")
            .unwrap_or_else(|_| format!("/tmp/logos-agent-three-testnet-{}", std::process::id())),
    );
    std::fs::create_dir_all(&dir)?;
    let overrides = WalletConfigOverrides {
        sequencers: Some(vec![SequencerConnectionData {
            sequencer_addr: testnet_url().parse()?,
            basic_auth: None,
        }]),
        seq_poll_max_retries: Some(2000),
        seq_poll_timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    };
    let (wallet, _) = WalletCore::new_init_storage(
        dir.join("config.json"),
        dir.join("storage"),
        dir.join("statistics.json"),
        Some(overrides),
        "testpw",
    )
    .await?;
    Ok(wallet)
}

#[tokio::test]
#[ignore = "creates three agent identities on the public LEZ testnet"]
async fn three_category_agents_on_public_testnet() -> Result<()> {
    let mut wallet = new_wallet().await?;
    let block = wallet.sync_to_latest_block().await?;
    anyhow::ensure!(block >= 1, "public testnet returned no blocks");

    let storage_agent = Agent::create(
        &mut wallet,
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;
    let messaging_agent = Agent::create(
        &mut wallet,
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;
    let blockchain_agent = Agent::create(
        &mut wallet,
        SpendingPolicy {
            per_tx_limit: 50,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;

    let accounts = [
        ("storage", storage_agent.account_id()),
        ("messaging", messaging_agent.account_id()),
        ("blockchain", blockchain_agent.account_id()),
    ];
    if accounts[0].1 == accounts[1].1 || accounts[1].1 == accounts[2].1 {
        bail!("category agents must have distinct identities");
    }
    for (category, account) in accounts {
        println!("testnet agent category={category} account={account}");
    }

    if let Ok(url) = env::var("AGENT_MESSAGING_URL") {
        let messaging = WakuMessaging::new(url);
        let topic = format!(
            "/logos-agent/testnet/evidence/{}",
            blockchain_agent.account_id()
        );
        let message_id = messaging
            .send(&topic, b"three-category-testnet-evidence")
            .await
            .or_else(|error| {
                let detail = format!("{error:#}");
                if detail.contains("NoPeersToPublish") {
                    Ok("local-store-fallback".to_owned())
                } else {
                    Err(error)
                }
            })?;
        let received = messaging.poll(&topic).await?;
        anyhow::ensure!(
            received
                .iter()
                .any(|message| message == b"three-category-testnet-evidence"),
            "Waku evidence message was not readable after publish"
        );
        println!("messaging evidence topic={topic} message_id={message_id}");
    }

    if let Ok(url) = env::var("AGENT_CODEX_URL") {
        let storage = CodexStorage::new(url, [7u8; 32]);
        let address = storage
            .upload("three-category-testnet", b"testnet-storage-evidence")
            .await?;
        let downloaded = storage.download(&address).await?;
        anyhow::ensure!(
            downloaded == b"testnet-storage-evidence",
            "Codex evidence object did not round-trip"
        );
        println!("storage evidence address={address}");
    }

    println!("testnet evidence block={block}");
    Ok(())
}
