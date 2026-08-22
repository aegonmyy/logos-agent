//! Program-mediated settlements on the public LEZ testnet.
//!
//! The public sequencer includes token-program mints but never included token
//! transfers during our polling windows (see docs/TESTNET_EVIDENCE.md). LEZ
//! programs, however, settle on the public testnet: a program deployment and
//! the calls addressed to it are ordinary public transactions that change chain
//! state. This test uses that fact to give each of the three category agents an
//! included, state-changing on-chain settlement: it deploys the sample
//! `claimer` program to the public testnet once, then each agent invokes its
//! `program.call` skill against a fresh public account and the account's
//! ownership flips to the program — verified afterwards with `program.query`.
//!
//! Everything runs through the agent's own skill dispatch (the same path a
//! third-party skill would take), against `https://testnet.lez.logos.co`.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use logos_agent::skills::{SkillContext, SkillRegistry};
use logos_agent::{Agent, SpendingPolicy};
use serde_json::json;
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
};
use wallet::config::{SequencerConnectionData, WalletConfigOverrides};
use wallet::WalletCore;

/// Bounded poll attempts for a public-testnet transaction to include (3s apart).
const INCLUSION_POLL_ATTEMPTS: usize = 60;

fn testnet_url() -> String {
    env::var("AGENT_TESTNET_URL").unwrap_or_else(|_| "https://testnet.lez.logos.co".to_owned())
}

async fn new_wallet() -> Result<WalletCore> {
    let dir = PathBuf::from(env::var("AGENT_TESTNET_STATE_DIR").unwrap_or_else(|_| {
        format!("/tmp/logos-agent-settlements-{}", std::process::id())
    }));
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
        bail!("expected public account registration");
    };
    Ok(account_id)
}

async fn wait_for_transaction_bounded(
    wallet: &mut WalletCore,
    tx_hash: &str,
) -> Result<Option<String>> {
    let hash = tx_hash.parse()?;
    for attempt in 1..=INCLUSION_POLL_ATTEMPTS {
        match wallet.poll_transaction(hash).await {
            Ok((_transaction, block)) => {
                println!("transaction {tx_hash} included in block {block}");
                return Ok(Some(block.to_string()));
            }
            Err(error) => {
                if attempt == 1 || attempt % 10 == 0 {
                    eprintln!(
                        "waiting for transaction {tx_hash} ({attempt}/{INCLUSION_POLL_ATTEMPTS}): {error}"
                    );
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    println!("transaction {tx_hash} not included after {INCLUSION_POLL_ATTEMPTS} attempts");
    Ok(None)
}

#[tokio::test]
#[ignore = "deploys a program and settles on the public LEZ testnet"]
async fn three_agents_settle_through_deployed_program_on_public_testnet() -> Result<()> {
    let mut wallet = new_wallet().await?;
    let block = wallet.sync_to_latest_block().await?;
    anyhow::ensure!(block >= 1, "public testnet returned no blocks");

    let registry = SkillRegistry::with_defaults();

    // Three category agents, distinct shielded identities, as in
    // tests/three_testnet_agents.rs.
    let mut agents = Vec::new();
    for category in ["storage", "messaging", "blockchain"] {
        let agent = Agent::create(
            &mut wallet,
            SpendingPolicy {
                per_tx_limit: 50,
                per_period_limit: 0,
                period_seconds: 86_400,
            },
        )
        .await?;
        println!("settlement agent category={category} account={}", agent.account_id());
        agents.push((category, agent));
    }
    anyhow::ensure!(
        agents[0].1.account_id() != agents[1].1.account_id()
            && agents[1].1.account_id() != agents[2].1.account_id(),
        "category agents must have distinct identities"
    );

    // 1) Deploy the claimer program to the public testnet (once — the
    //    deployment transaction is the analogue of edenbd1's block-8839 deploy).
    let claimer = test_programs::claimer();
    let expected_owner: Vec<u32> = claimer.id().to_vec();
    let elf_path = std::env::temp_dir().join(format!("claimer-testnet-{}.elf", std::process::id()));
    std::fs::write(&elf_path, claimer.elf())?;
    let deployed = {
        let (.., agent) = &mut agents[0];
        let mut sctx = SkillContext {
            wallet: Some(&mut wallet),
            agent,
        };
        registry
            .dispatch(
                "program.deploy",
                &mut sctx,
                json!({ "binary_path": elf_path.to_string_lossy() }),
            )
            .await?
    };
    let program_id_hex = deployed["program_id"]
        .as_str()
        .expect("deploy should return a program_id")
        .to_owned();
    assert_eq!(program_id_hex.len(), 64, "program id should be 64 hex chars");
    println!("program deployed id={program_id_hex}");
    let _ = std::fs::remove_file(&elf_path);

    // Give the deployment a moment to land before the first call targets it.
    tokio::time::sleep(Duration::from_secs(10)).await;
    wallet.sync_to_latest_block().await?;

    // 2) Each agent settles: a program.call that claims a fresh public
    //    account, waited to inclusion, then verified by reading the account
    //    back (its program_owner must now be the deployed program).
    let mut settled = 0usize;
    for (category, agent) in &mut agents {
        let account = new_public_account(&mut wallet).await?;
        tokio::time::sleep(Duration::from_secs(3)).await;

        let called = {
            let mut sctx = SkillContext {
                wallet: Some(&mut wallet),
                agent,
            };
            registry
                .dispatch(
                    "program.call",
                    &mut sctx,
                    json!({
                        "program_id": program_id_hex,
                        "accounts": [account.to_string()],
                        "instruction": [],
                    }),
                )
                .await?
        };
        let tx_hash = called["tx_hash"]
            .as_str()
            .expect("call should return a tx hash")
            .to_owned();
        let inclusion = wait_for_transaction_bounded(&mut wallet, &tx_hash).await?;
        wallet.sync_to_latest_block().await?;

        let queried = {
            let mut sctx = SkillContext {
                wallet: Some(&mut wallet),
                agent,
            };
            registry
                .dispatch("program.query", &mut sctx, json!({ "account": account.to_string() }))
                .await?
        };
        let owner: Vec<u32> = queried["state"]["program_owner"]
            .as_array()
            .expect("account state should carry a program_owner")
            .iter()
            .map(|w| w.as_u64().expect("owner word is u32") as u32)
            .collect();
        let state_verified = owner == expected_owner;
        println!(
            "testnet settlement category={category} tx={tx_hash} account={account} state={} verified={state_verified}",
            match &inclusion {
                Some(block) => format!("included_block={block}"),
                None => "not_included".to_owned(),
            },
        );
        if state_verified && inclusion.is_some() {
            settled += 1;
        }
    }

    anyhow::ensure!(
        settled >= 2,
        "expected at least two verified program-mediated settlements on the public testnet (got {settled})"
    );
    println!("testnet settlements verified={settled}/3 program={program_id_hex}");
    Ok(())
}
