//! Reproducible LP-0008 use-case demonstrations.
//!
//! The test covers three workflows with the real service adapters:
//! personal file vault, privacy-preserving notary, and a paid multi-agent task.
//! The test is ignored because it needs Codex, nwaku, and optionally a public
//! LEZ wallet. Run it with `--nocapture` to retain the evidence identifiers.

use std::env;
use std::sync::Arc;

use anyhow::{Context as _, Result, ensure};
use logos_agent::a2a::{A2aClient, A2aProvider, TaskState};
use logos_agent::messaging::{Messaging, WakuMessaging};
use logos_agent::skills::{EchoSkill, SkillRegistry};
use logos_agent::storage::{CodexStorage, Storage};
use logos_agent::{Agent, SpendingPolicy};
use sha2::{Digest, Sha256};
use wallet::WalletCore;
use wallet::config::{SequencerConnectionData, WalletConfigOverrides};

const VAULT_MESSAGE: &[u8] = b"LP-0008 personal file vault evidence";
const NOTARY_DOCUMENT: &[u8] = b"LP-0008 privacy-preserving notary evidence";

fn service_url(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn topic(label: &str) -> String {
    format!("/logos-agent/1/use-cases/{label}/proto")
}

async fn vault(storage: &CodexStorage, messaging: &WakuMessaging) -> Result<()> {
    let cid = storage.upload("personal-file-vault", VAULT_MESSAGE).await?;
    let topic = topic("personal-file-vault");
    let message = format!("stored file CID={cid}");
    let _ = messaging.send(&topic, message.as_bytes()).await;
    let downloaded = storage.download(&cid).await?;
    ensure!(downloaded == VAULT_MESSAGE, "vault file did not round-trip");
    let digest = hex::encode(Sha256::digest(&downloaded));
    println!("use_case=personal_file_vault cid={cid} topic={topic} sha256={digest}");
    Ok(())
}

async fn notary(storage: &CodexStorage, messaging: &WakuMessaging) -> Result<()> {
    let cid = storage
        .upload("privacy-preserving-notary", NOTARY_DOCUMENT)
        .await?;
    let digest = hex::encode(Sha256::digest(NOTARY_DOCUMENT));
    let topic = topic("privacy-preserving-notary");
    let record = format!("notary cid={cid} sha256={digest}");
    let _ = messaging.send(&topic, record.as_bytes()).await;
    let downloaded = storage.download(&cid).await?;
    ensure!(
        hex::encode(Sha256::digest(&downloaded)) == digest,
        "notary digest did not verify"
    );
    println!("use_case=privacy_preserving_notary cid={cid} topic={topic} sha256={digest}");
    Ok(())
}

async fn paid_multi_agent_task(wallet: &mut WalletCore) -> Result<()> {
    let messaging = Arc::new(logos_agent::messaging::InMemoryMessaging::new());
    let client_agent = Agent::create(
        wallet,
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;
    let provider_agent = Agent::create(
        wallet,
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;
    let mut registry = SkillRegistry::new();
    registry.register(Box::new(EchoSkill));
    let provider = A2aProvider::new(
        provider_agent,
        Arc::clone(&messaging) as Arc<_>,
        registry,
        "lp0008-specialist",
        &[("demo.echo", 0)],
    );
    let discovery = topic("paid-multi-agent-task");
    provider.publish_card(&discovery).await?;
    let mut client = A2aClient::new(client_agent, Arc::clone(&messaging) as Arc<_>);
    let cards = client.discover(&discovery).await?;
    let card = cards.first().context("provider card was not discovered")?;
    let task = client
        .run_task(
            wallet,
            card,
            "demo.echo",
            serde_json::json!({ "text": "vault-ready" }),
        )
        .await?;
    ensure!(
        task.state == TaskState::Submitted,
        "task did not enter submitted state"
    );
    provider.serve_pending(None).await?;
    let completed = client.poll_task(card, &task).await?;
    ensure!(
        completed.state == TaskState::Completed,
        "task did not complete"
    );
    ensure!(
        completed
            .result
            .as_ref()
            .and_then(|value| value["echo"].as_str())
            == Some("vault-ready"),
        "task result was not returned"
    );
    println!(
        "use_case=paid_multi_agent_task task_id={} provider={} state=completed",
        task.id, card.lez_account
    );
    Ok(())
}

async fn new_wallet() -> Result<WalletCore> {
    let dir = env::temp_dir().join(format!("logos-agent-use-cases-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let endpoint =
        env::var("AGENT_TESTNET_URL").unwrap_or_else(|_| "https://testnet.lez.logos.co".into());
    let overrides = WalletConfigOverrides {
        sequencers: Some(vec![SequencerConnectionData {
            sequencer_addr: endpoint.parse()?,
            basic_auth: None,
        }]),
        seq_poll_max_retries: Some(2000),
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
#[ignore = "requires Codex and nwaku; paid task also requires a configured LEZ wallet"]
async fn three_lp0008_use_cases() -> Result<()> {
    let storage = CodexStorage::new(
        service_url("AGENT_CODEX_URL", "http://127.0.0.1:8080"),
        [11u8; 32],
    );
    let messaging = WakuMessaging::new(service_url("AGENT_MESSAGING_URL", "http://127.0.0.1:8645"));
    vault(&storage, &messaging).await?;
    notary(&storage, &messaging).await?;

    let mut wallet = new_wallet().await?;
    if env::var("RUN_PAID_A2A").as_deref() == Ok("1") {
        paid_multi_agent_task(&mut wallet).await?;
    } else {
        println!("use_case=paid_multi_agent_task skipped=requires_RUN_PAID_A2A=1");
    }
    Ok(())
}
