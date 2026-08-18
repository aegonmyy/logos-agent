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
use test_fixtures::public_mention;
use token_core::TokenHolding;
use wallet::WalletCore;
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};
use wallet::config::{SequencerConnectionData, WalletConfigOverrides};

const VAULT_MESSAGE: &[u8] = b"LP-0008 personal file vault evidence";
const NOTARY_DOCUMENT: &[u8] = b"LP-0008 privacy-preserving notary evidence";
const TRANSACTION_POLL_ATTEMPTS: usize = 600;

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

async fn public_anchor(
    wallet: &mut WalletCore,
    label: &str,
    amount: u128,
) -> Result<(lee::AccountId, lee::AccountId, String, String)> {
    let token = new_public_account(wallet).await?;
    let holder = new_public_account(wallet).await?;
    let recipient = new_public_account(wallet).await?;
    let mint = wallet::cli::execute_subcommand(
        wallet,
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(token),
            supply_account_id: public_mention(holder),
            name: format!("LP0008-{label}"),
            total_supply: amount + 1,
        }),
    )
    .await?;
    let SubcommandReturnValue::TransactionExecuted { tx_hash: mint_hash } = mint else {
        anyhow::bail!("expected {label} mint transaction");
    };
    let mint_block = wait_for_transaction(wallet, &mint_hash.to_string()).await?;
    wallet.sync_to_latest_block().await?;
    ensure!(
        public_token_balance(wallet, holder).await? >= amount,
        "{label} anchor holder was not funded"
    );
    let transfer = wallet::cli::execute_subcommand(
        wallet,
        Command::Token(TokenProgramAgnosticSubcommand::Send {
            from: public_mention(holder),
            to: Some(public_mention(recipient)),
            to_npk: None,
            to_vpk: None,
            to_keys: None,
            to_identifier: Some(0),
            amount,
        }),
    )
    .await?;
    let SubcommandReturnValue::TransactionExecuted { tx_hash } = transfer else {
        anyhow::bail!("expected {label} anchor transaction");
    };
    let block = wait_for_transaction(wallet, &tx_hash.to_string()).await?;
    println!("public_anchor label={label} tx={tx_hash} block={block} mint_block={mint_block}");
    Ok((token, recipient, tx_hash.to_string(), block))
}

async fn new_public_account(wallet: &mut WalletCore) -> Result<lee::AccountId> {
    let result = wallet::cli::execute_subcommand(
        wallet,
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

async fn wait_for_transaction(wallet: &mut WalletCore, tx_hash: &str) -> Result<String> {
    let hash = tx_hash.parse()?;
    for attempt in 1..=TRANSACTION_POLL_ATTEMPTS {
        match wallet.poll_transaction(hash).await {
            Ok((_transaction, block)) => {
                println!("transaction {tx_hash} included in block {block}");
                return Ok(block.to_string());
            }
            Err(error) => {
                if attempt == 1 || attempt % 20 == 0 {
                    eprintln!(
                        "waiting for transaction {tx_hash} ({attempt}/{TRANSACTION_POLL_ATTEMPTS}): {error}"
                    );
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    anyhow::bail!(
        "timed out waiting for transaction {tx_hash} after {} minutes",
        TRANSACTION_POLL_ATTEMPTS * 3 / 60
    )
}

async fn public_token_balance(wallet: &WalletCore, account: lee::AccountId) -> Result<u128> {
    let state = wallet.get_account_public(account).await?;
    match TokenHolding::try_from(&state.data) {
        Ok(TokenHolding::Fungible { balance, .. }) => Ok(balance),
        _ => Ok(0),
    }
}

async fn paid_multi_agent_task(wallet: &mut WalletCore) -> Result<()> {
    let definition = new_public_account(wallet).await?;
    let payer = new_public_account(wallet).await?;
    let provider_account = new_public_account(wallet).await?;
    let transfer = wallet::cli::execute_subcommand(
        wallet,
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: public_mention(payer),
            name: "LP0008-A2A".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;
    let SubcommandReturnValue::TransactionExecuted { tx_hash } = transfer else {
        anyhow::bail!("expected payment transaction hash");
    };
    let mint_block = wait_for_transaction(wallet, &tx_hash.to_string()).await?;
    wallet.sync_to_latest_block().await?;
    let payer_balance = public_token_balance(wallet, payer).await?;
    ensure!(
        payer_balance >= 10,
        "payer balance is {payer_balance}; mint was not readable before payment"
    );
    println!("public_payment payer={payer} balance={payer_balance}");
    let transfer = wallet::cli::execute_subcommand(
        wallet,
        Command::Token(TokenProgramAgnosticSubcommand::Send {
            from: public_mention(payer),
            to: Some(public_mention(provider_account)),
            to_npk: None,
            to_vpk: None,
            to_keys: None,
            to_identifier: Some(0),
            amount: 10,
        }),
    )
    .await?;
    let SubcommandReturnValue::TransactionExecuted {
        tx_hash: payment_tx,
    } = transfer
    else {
        anyhow::bail!("expected payment transaction hash");
    };
    let payment_tx = payment_tx.to_string();
    let transfer_block = wait_for_transaction(wallet, &payment_tx).await?;

    let messaging = Arc::new(logos_agent::messaging::InMemoryMessaging::new());
    let client_agent = Agent::from_parts(
        payer,
        SpendingPolicy {
            per_tx_limit: 10,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    );
    let provider_agent = Agent::from_parts(
        provider_account,
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    );
    let mut registry = SkillRegistry::new();
    registry.register(Box::new(EchoSkill));
    let provider = A2aProvider::new(
        provider_agent,
        Arc::clone(&messaging) as Arc<_>,
        registry,
        "lp0008-specialist",
        &[("demo.echo", 10)],
    );
    ensure!(
        provider.card().lez_account == provider_account.to_string(),
        "payment account must match the provider Agent Card"
    );
    let discovery = topic("paid-multi-agent-task");
    provider.publish_card(&discovery).await?;
    let mut client = A2aClient::new(client_agent, Arc::clone(&messaging) as Arc<_>);
    let cards = client.discover(&discovery).await?;
    let card = cards.first().context("provider card was not discovered")?;
    let task = client
        .run_task_with_payment(
            card,
            "demo.echo",
            serde_json::json!({ "text": "vault-ready" }),
            &payment_tx,
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
        "use_case=paid_multi_agent_task task_id={} provider={} public_payment_from={} token={} amount=10 mint_block={} transfer_block={} state=completed",
        task.id, card.lez_account, payer, definition, mint_block, transfer_block
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

async fn public_testnet_use_cases(
    storage: &CodexStorage,
    messaging: &WakuMessaging,
    wallet: &mut WalletCore,
) -> Result<()> {
    let vault_cid = storage
        .upload("public-testnet-vault", VAULT_MESSAGE)
        .await?;
    let vault_anchor = public_anchor(wallet, "Vault", 1).await?;
    let vault_topic = topic("public-testnet-vault");
    let vault_notice = format!("cid={vault_cid} tx={}", vault_anchor.2);
    let _ = messaging.send(&vault_topic, vault_notice.as_bytes()).await;
    ensure!(
        storage.download(&vault_cid).await? == VAULT_MESSAGE,
        "vault verification failed"
    );
    println!(
        "public_use_case=personal_file_vault cid={vault_cid} tx={} block={} topic={vault_topic}",
        vault_anchor.2, vault_anchor.3
    );

    let notary_cid = storage
        .upload("public-testnet-notary", NOTARY_DOCUMENT)
        .await?;
    let digest = hex::encode(Sha256::digest(NOTARY_DOCUMENT));
    let notary_anchor = public_anchor(wallet, "Notary", 1).await?;
    let notary_topic = topic("public-testnet-notary");
    let notary_record = format!("cid={notary_cid} sha256={digest} tx={}", notary_anchor.2);
    let _ = messaging
        .send(&notary_topic, notary_record.as_bytes())
        .await;
    ensure!(
        hex::encode(Sha256::digest(&storage.download(&notary_cid).await?)) == digest,
        "notary verification failed"
    );
    println!(
        "public_use_case=privacy_preserving_notary cid={notary_cid} sha256={digest} tx={} block={} topic={notary_topic}",
        notary_anchor.2, notary_anchor.3
    );

    let event_anchor = public_anchor(wallet, "EventAlert", 1).await?;
    let event_topic = topic("public-testnet-event-alert");
    let alert = format!(
        "event=token_transfer tx={} block={}",
        event_anchor.2, event_anchor.3
    );
    let _ = messaging.send(&event_topic, alert.as_bytes()).await;
    println!(
        "public_use_case=on_chain_event_alerter tx={} block={} topic={event_topic}",
        event_anchor.2, event_anchor.3
    );
    Ok(())
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
    if env::var("RUN_PUBLIC_USE_CASES").as_deref() == Ok("1") {
        public_testnet_use_cases(&storage, &messaging, &mut wallet).await?;
    } else if env::var("RUN_PAID_A2A").as_deref() == Ok("1") {
        paid_multi_agent_task(&mut wallet).await?;
    } else {
        println!("use_case=paid_multi_agent_task skipped=requires_RUN_PAID_A2A=1");
    }
    Ok(())
}
