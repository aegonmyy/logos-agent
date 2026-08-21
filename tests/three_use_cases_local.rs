//! LP-0008 criterion #9: three use cases demonstrated end-to-end against a
//! real local LEZ sequencer (standalone, brought up via Docker by
//! `TestContext`). This is the reproducible, no-public-testnet companion to
//! `tests/three_use_cases.rs`, which anchors the same use cases on the public
//! LEZ testnet. The three use cases are:
//!
//! 1. **Personal file vault** — store a private file, advertise its address
//!    over Logos Messaging, and verify it round-trips intact.
//! 2. **Privacy-preserving notary** — store a document, record its content
//!    digest over Logos Messaging, and verify the stored copy matches the
//!    recorded digest.
//! 3. **Paid multi-agent task** — two agents discover each other via Agent
//!    Cards, run a task through the A2A lifecycle, and settle LEZ payment
//!    autonomously with a real on-chain transfer.
//!
//! Storage and Messaging use the in-memory backends here (the real Codex/Waku
//! backends are exercised separately in `storage_messaging_skills` and
//! `three_category_agents`); the on-chain payment is a real private transfer on
//! the live local sequencer.

use std::sync::Arc;

use anyhow::{Result, ensure};
use logos_agent::a2a::{A2aClient, A2aProvider, TaskState};
use logos_agent::messaging::{InMemoryMessaging, Messaging};
use logos_agent::skills::{EchoSkill, SkillRegistry};
use logos_agent::storage::{InMemoryStorage, Storage};
use logos_agent::{Agent, SpendingPolicy};
use sha2::{Digest, Sha256};
use test_fixtures::{TestContext, private_mention, public_mention};
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};

const VAULT_MESSAGE: &[u8] = b"LP-0008 personal file vault evidence";
const NOTARY_DOCUMENT: &[u8] = b"LP-0008 privacy-preserving notary evidence";
const DISCOVERY: &str = "/logos-agent/1/use-cases/discovery/proto";

fn topic(label: &str) -> String {
    format!("/logos-agent/1/use-cases/{label}/proto")
}

async fn new_public_account(ctx: &mut TestContext) -> Result<lee::AccountId> {
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount { account_id } = result else {
        anyhow::bail!("expected public account registration");
    };
    Ok(account_id)
}

/// Use case 1: personal file vault.
async fn personal_file_vault(
    storage: &dyn Storage,
    messaging: &dyn Messaging,
) -> Result<()> {
    let cid = storage.upload("personal-file-vault", VAULT_MESSAGE).await?;
    let topic = topic("personal-file-vault");
    let notice = format!("stored file CID={cid}");
    let _ = messaging.send(&topic, notice.as_bytes()).await;
    let downloaded = storage.download(&cid).await?;
    ensure!(downloaded == VAULT_MESSAGE, "vault file did not round-trip");
    let digest = hex::encode(Sha256::digest(&downloaded));
    println!("use_case=personal_file_vault cid={cid} topic={topic} sha256={digest}");
    Ok(())
}

/// Use case 2: privacy-preserving notary.
async fn privacy_preserving_notary(
    storage: &dyn Storage,
    messaging: &dyn Messaging,
) -> Result<()> {
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

/// Use case 3: paid multi-agent task. Two agents discover each other, run a
/// task through the A2A lifecycle, and settle LEZ payment with a real on-chain
/// transfer. No owner is in the loop.
async fn paid_multi_agent_task(ctx: &mut TestContext) -> Result<()> {
    let definition = new_public_account(ctx).await?;
    let client_agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 50,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;
    let provider_agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;

    // Fund the client with 100 tokens; the provider starts with nothing.
    wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(client_agent.account_id()),
            name: "USECASE-COIN".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;

    let messaging = Arc::new(InMemoryMessaging::new());

    let mut provider_registry = SkillRegistry::new();
    provider_registry.register(Box::new(EchoSkill));
    let provider = A2aProvider::new(
        provider_agent,
        Arc::clone(&messaging) as Arc<_>,
        provider_registry,
        "usecase-specialist",
        &[("demo.echo", 10)],
    );
    provider.publish_card(DISCOVERY).await?;

    let mut client = A2aClient::new(client_agent, Arc::clone(&messaging) as Arc<_>);
    let cards = client.discover(DISCOVERY).await?;
    ensure!(cards.len() == 1, "should discover exactly one provider");
    let card = &cards[0];

    let task = client
        .run_task(ctx.wallet_mut(), card, "demo.echo", serde_json::json!({ "text": "vault-ready" }))
        .await?;
    ensure!(task.state == TaskState::Submitted, "task did not enter submitted state");

    let served = provider.serve_pending(None).await?;
    ensure!(served == 1, "provider should serve one task");

    let done = client.poll_task(card, &task).await?;
    ensure!(done.state == TaskState::Completed, "task did not complete");
    ensure!(
        done.result
            .as_ref()
            .and_then(|value| value["echo"].as_str())
            == Some("vault-ready"),
        "task result was not returned"
    );

    // Payment settled autonomously: client 90, provider 10.
    ctx.wallet_mut().sync_to_latest_block().await?;
    ensure!(
        client.agent().balance(ctx.wallet(), definition) == 90,
        "client should have paid the task price"
    );
    ensure!(
        provider.agent().balance(ctx.wallet(), definition) == 10,
        "provider should have received the task price"
    );
    println!(
        "use_case=paid_multi_agent_task task_id={} provider={} token={} client_balance=90 provider_balance=10 state=completed",
        task.id, card.lez_account, definition
    );
    Ok(())
}

#[tokio::test]
async fn three_lp0008_use_cases_local() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // The storage and messaging backends are in-memory here; the real Codex
    // and Waku backends are exercised in storage_messaging_skills and
    // three_category_agents. The on-chain payment below is a real private
    // transfer on the live local sequencer.
    let storage = InMemoryStorage::new([11u8; 32]);
    let messaging = InMemoryMessaging::new();

    personal_file_vault(&storage, &messaging).await?;
    privacy_preserving_notary(&storage, &messaging).await?;
    paid_multi_agent_task(&mut ctx).await?;

    println!("three_use_cases_local=complete personal_file_vault privacy_preserving_notary paid_multi_agent_task");
    Ok(())
}
