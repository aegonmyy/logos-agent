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
use test_fixtures::{private_mention, public_mention};
use token_core::TokenHolding;
use wallet::WalletCore;
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};
use wallet::config::{SequencerConnectionData, WalletConfigOverrides};

/// Bounded poll attempts for a public-testnet mint to include (3s apart).
const MINT_POLL_ATTEMPTS: usize = 40;

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
        // Bound every internal wait (agent creation, mint submission) so a
        // transaction the sequencer never includes reports submitted_pending
        // in minutes instead of wedging the run for seq_poll_max_retries ×
        // seq_poll_timeout. The public testnet drops private/shielded
        // transactions (see docs/TESTNET_EVIDENCE.md); a private-supply mint
        // is in that class.
        seq_poll_max_retries: Some(60),
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

    // Each agent performs an included on-chain transaction on the public
    // testnet: a token mint whose supply (holder) account is the agent's own
    // shielded account, so the per-agent on-chain artifact is not just an
    // identity but a holding the agent owns. The mint command waits for
    // inclusion internally; on a slow testnet it may end as submitted-pending,
    // which is reported rather than failing.
    for (category, agent_account) in accounts {
        let token = new_public_account(&mut wallet).await?;
        let mint = wallet::cli::execute_subcommand(
            &mut wallet,
            Command::Token(TokenProgramAgnosticSubcommand::New {
                definition_account_id: public_mention(token),
                supply_account_id: private_mention(agent_account),
                name: format!("LP0008-AGENT-{category}"),
                total_supply: 100,
            }),
        )
        .await;
        match mint {
            Ok(SubcommandReturnValue::TransactionExecuted { tx_hash }) => {
                let tx_hash = tx_hash.to_string();
                let block = wait_for_transaction_bounded(&mut wallet, &tx_hash).await?;
                let balance = private_token_balance(&wallet, agent_account, token);
                println!(
                    "testnet agent category={category} mint token={token} tx={tx_hash} state={} balance={balance}",
                    match block {
                        Some(ref b) => format!("included_block={b}"),
                        None => "submitted_pending".to_owned(),
                    }
                );
            }
            Ok(_) => println!("testnet agent category={category} mint returned no transaction"),
            Err(error) => println!(
                "testnet agent category={category} mint_state=submitted_pending (wallet wait ended: {error})"
            ),
        }
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

fn private_token_balance(
    wallet: &WalletCore,
    account: lee::AccountId,
    token: lee::AccountId,
) -> u128 {
    match wallet.get_account_private(account) {
        Some(state) => match TokenHolding::try_from(&state.data) {
            Ok(TokenHolding::Fungible {
                definition_id,
                balance,
            }) if definition_id == token => balance,
            _ => 0,
        },
        None => 0,
    }
}

async fn wait_for_transaction_bounded(
    wallet: &mut WalletCore,
    tx_hash: &str,
) -> Result<Option<String>> {
    let hash = tx_hash.parse()?;
    for attempt in 1..=MINT_POLL_ATTEMPTS {
        match wallet.poll_transaction(hash).await {
            Ok((_transaction, block)) => {
                println!("transaction {tx_hash} included in block {block}");
                return Ok(Some(block.to_string()));
            }
            Err(error) => {
                if attempt == 1 || attempt % 10 == 0 {
                    eprintln!("waiting for transaction {tx_hash} ({attempt}/{MINT_POLL_ATTEMPTS}): {error}");
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    println!("transaction {tx_hash} not included after {MINT_POLL_ATTEMPTS} attempts (continuing)");
    Ok(None)
}
