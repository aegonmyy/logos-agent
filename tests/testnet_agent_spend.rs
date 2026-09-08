//! The agent itself spending from its own shielded account on the PUBLIC LEZ
//! testnet at `RISC0_DEV_MODE=0` — the same `Agent::create` / `Agent::send`
//! path the `wallet.send` skill drives, not the bare wallet path the
//! shielded probe (`testnet_shielded_probe.rs`) exercises.
//!
//! What this fuses into one artifact, against the public testnet:
//! 1. the agent has its own shielded account (created by `Agent::create`);
//! 2. it RECEIVES tokens: the token supply mints straight into its private
//!    account (a proof-bearing `PrivacyPreserving` transaction);
//! 3. it SENDS autonomously below its owner-set limit (`Agent::send` →
//!    `SpendOutcome::Executed`, another proof-bearing transaction);
//! 4. it HOLDS above the limit (`SpendOutcome::NeedsOwnerApproval`, no
//!    transaction submitted);
//! 5. the on-chain transaction its autonomous spend produced is
//!    `PrivacyPreserving` (type byte `0x01`) and carries the ZK proof
//!    (hundreds of KB), verified via `getTransaction` / `getBlockRange`.
//!
//! Ignored by default (hits the network; two real Groth16 proofs, ~20-30
//! min). Run with:
//!   RISC0_DEV_MODE=0 cargo test --test testnet_agent_spend -- --ignored --nocapture --test-threads=1

use std::time::Duration;

use anyhow::{Result, bail};
use logos_agent::{Agent, SpendOutcome, SpendingPolicy};
use test_fixtures::{private_mention, public_mention};
use wallet::WalletCore;
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};
use wallet::config::{SequencerConnectionData, WalletConfigOverrides};

fn testnet_url() -> String {
    std::env::var("AGENT_TESTNET_URL")
        .unwrap_or_else(|_| "https://testnet.lez.logos.co".to_owned())
}

macro_rules! net_retry {
    ($op:expr, $label:expr) => {{
        let mut attempt = 0;
        loop {
            attempt += 1;
            match $op.await {
                Ok(v) => break v,
                Err(e) => {
                    let m = e.to_string();
                    let transient = m.contains("502")
                        || m.contains("rejected")
                        || m.contains("timed out")
                        || m.contains("error sending request")
                        || m.contains("connection");
                    if transient && attempt < 1200 {
                        if attempt % 30 == 1 {
                            eprintln!("[{0}] transient (attempt {attempt}): {m}; retrying", $label);
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }
    }};
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
        bail!("expected a registered account id");
    };
    Ok(account_id)
}

async fn new_private_account(wallet: &mut WalletCore) -> Result<lee::AccountId> {
    let result = wallet::cli::execute_subcommand(
        wallet,
        Command::Account(AccountSubcommand::New(NewSubcommand::Private {
            cci: None,
            label: None,
        })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount { account_id } = result else {
        bail!("expected a registered account id");
    };
    Ok(account_id)
}

async fn rpc_post(url: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let resp: serde_json::Value = client.post(url).json(&body).send().await?.json().await?;
    Ok(resp
        .get("result")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no result in {method} response: {resp}"))?)
}

/// Fetch a transaction by hash; return (total_size_bytes, type_byte).
/// 0x00 = Public (no proof), 0x01 = PrivacyPreserving (carries the proof).
async fn on_chain_tx_size_and_type(tx_hash: &str, url: &str) -> Result<(usize, u8)> {
    use base64::Engine;
    let result = rpc_post(url, "getTransaction", serde_json::json!([tx_hash])).await?;
    let arr = result
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("getTransaction result is not an array (tx not on chain)"))?;
    let mut total = 0usize;
    let mut type_byte = 0u8;
    for elem in arr {
        if let Some(s) = elem.as_str() {
            let raw = base64::engine::general_purpose::STANDARD.decode(s)?;
            total += raw.len();
            if type_byte == 0 && !raw.is_empty() {
                type_byte = raw[0];
            }
        }
    }
    Ok((total, type_byte))
}

/// The last block id the sequencer reports.
async fn last_block_id(url: &str) -> Result<u64> {
    let v = rpc_post(url, "getLastBlockId", serde_json::json!([])).await?;
    Ok(v
        .as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| anyhow::anyhow!("unexpected getLastBlockId result: {v}"))?)
}

/// Scan blocks (start..=end) for included PrivacyPreserving transactions of
/// at least `min_size` borsh bytes; return (tx_hash_hex, size) pairs.
async fn scan_blocks_for_proof_txs(
    url: &str,
    start: u64,
    end: u64,
    min_size: usize,
) -> Result<Vec<(String, usize)>> {
    use base64::Engine;
    use borsh::BorshDeserialize;
    use common::block::Block;
    use common::transaction::LeeTransaction;

    let blocks = rpc_post(url, "getBlockRange", serde_json::json!([start, end])).await?;
    let arr = blocks
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("getBlockRange result is not an array"))?;
    let mut found = Vec::new();
    for entry in arr {
        let b64 = entry
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("block entry is not a base64 string"))?;
        let raw = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let block = Block::deserialize(&mut raw.as_slice())?;
        for tx in &block.body.transactions {
            if let LeeTransaction::PrivacyPreserving(_) = tx {
                let size = borsh::to_vec(tx)?.len();
                if size >= min_size {
                    found.push((tx.hash().to_string(), size));
                }
            }
        }
    }
    Ok(found)
}

#[tokio::test]
#[ignore = "hits the live public LEZ testnet with two real shielded txs; slow (~20-30 min proving)"]
async fn agent_spends_from_own_shielded_account_on_public_testnet() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("agent-spend-testnet-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;

    let url = testnet_url();
    let overrides = WalletConfigOverrides {
        sequencers: Some(vec![SequencerConnectionData {
            sequencer_addr: url.parse().unwrap(),
            basic_auth: None,
        }]),
        seq_tx_poll_max_blocks: Some(400),
        seq_poll_max_retries: Some(4000),
        seq_poll_timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    };
    let (mut wallet, _mnemonic) = WalletCore::new_init_storage(
        dir.join("config.json"),
        dir.join("storage"),
        dir.join("statistics.json"),
        Some(overrides),
        "testpw",
    )
    .await?;

    net_retry!(wallet.sync_to_latest_block(), "initial-sync");
    let b_start = last_block_id(&url).await?;
    eprintln!("[AGENT-SPEND] testnet url: {url}");
    eprintln!("[AGENT-SPEND] start block: {b_start}");

    // The token definition lives in a public account; the agent will hold the
    // whole supply in its own shielded account.
    let definition = net_retry!(new_public_account(&mut wallet), "new-definition");
    eprintln!("[AGENT-SPEND] token definition (public): {definition}");

    // The agent: its own shielded account, created by Agent::create — the
    // same constructor the deployed binary uses. Owner-set per-tx limit: 50.
    let policy = SpendingPolicy {
        per_tx_limit: 50,
        per_period_limit: 0,
        period_seconds: 86_400,
    };
    let agent = Agent::create(&mut wallet, policy).await?;
    eprintln!("[AGENT-SPEND] agent shielded account (private): {}", agent.account_id());

    // A counterparty the agent will pay.
    let recipient = net_retry!(new_private_account(&mut wallet), "new-recipient");
    eprintln!("[AGENT-SPEND] recipient (private): {recipient}");

    // 1. Fund the agent: mint 100 tokens straight into its shielded account.
    //    A supply mint into a private account is itself a proof-bearing
    //    PrivacyPreserving transaction: this leg is the agent RECEIVING on the
    //    public testnet.
    eprintln!("[AGENT-SPEND] minting 100 tokens into the agent's shielded account (real proving, ~10-15 min)...");
    wallet::cli::execute_subcommand(
        &mut wallet,
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(agent.account_id()),
            name: "AGENT-TESTNET".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(30)).await;
    net_retry!(wallet.sync_to_latest_block(), "sync-after-mint");
    let funded = agent.balance(&wallet, definition);
    eprintln!("[AGENT-SPEND] agent balance after mint: {funded}");
    assert_eq!(funded, 100, "the mint should land in the agent's shielded account on the public testnet");
    let b_after_mint = last_block_id(&url).await?;
    eprintln!("[AGENT-SPEND] block after mint: {b_after_mint}");

    // 2. The agent SENDS autonomously, below its limit (10 <= 50). This is
    //    Agent::send -> execute_send, the exact path wallet.send drives: a
    //    private -> private shielded transfer carrying a real proof.
    eprintln!("[AGENT-SPEND] agent spending 10 autonomously (real proving, ~10-15 min)...");
    let outcome = agent.send(&mut wallet, recipient, 10).await?;
    assert_eq!(
        outcome,
        SpendOutcome::Executed { amount: 10, to: recipient },
        "a below-limit spend should execute autonomously on the public testnet"
    );

    // 3. The agent HOLDS above its limit (75 > 50): no transaction submitted.
    let outcome = agent.send(&mut wallet, recipient, 75).await?;
    assert_eq!(
        outcome,
        SpendOutcome::NeedsOwnerApproval { amount: 75, to: recipient, limit: 50 },
        "an above-limit spend should be held for owner approval, not executed"
    );

    // 4. Settlement: balance 100 -> 90 on chain.
    tokio::time::sleep(Duration::from_secs(30)).await;
    net_retry!(wallet.sync_to_latest_block(), "sync-after-send");
    let after = agent.balance(&wallet, definition);
    eprintln!("[AGENT-SPEND] agent balance after send: {after}");
    assert_eq!(after, 90, "the agent's autonomous spend should settle on chain: 100 -> 90");
    let b_end = last_block_id(&url).await?;
    eprintln!("[AGENT-SPEND] end block: {b_end}");

    // 5. The on-chain proof: scan the blocks minted after the funding leg for
    //    the agent's spend — a PrivacyPreserving (0x01) transaction carrying
    //    the ZK proof. The mint is also proof-bearing; it landed at or before
    //    b_after_mint, so scanning (b_after_mint, b_end] isolates the spend.
    let start = b_after_mint + 1;
    if start <= b_end {
        let proofs = scan_blocks_for_proof_txs(&url, start, b_end, 100_000).await?;
        eprintln!("[AGENT-SPEND] proof-bearing txs in blocks {start}..={b_end}: {}", proofs.len());
        for (i, (hash, size)) in proofs.iter().enumerate() {
            eprintln!("[AGENT-SPEND]   tx {i}: {hash} ({size} bytes, PrivacyPreserving)");
        }
        assert!(
            !proofs.is_empty(),
            "the agent's spend should appear on chain as a PrivacyPreserving tx >100KB in blocks {start}..={b_end}"
        );
        // Re-fetch the first one via getTransaction so the doc can cite a
        // single hash verified end to end.
        let (hash, _) = proofs[0].clone();
        let (size, type_byte) = on_chain_tx_size_and_type(&hash, &url).await?;
        eprintln!("[AGENT-SPEND] getTransaction({hash}): {size} bytes, type byte 0x{type_byte:02x}");
        assert_eq!(type_byte, 0x01, "the agent's spend must be PrivacyPreserving (0x01), not Public (0x00)");
        assert!(size > 100_000, "the agent's spend carries the ZK proof; got {size} bytes");
    }

    eprintln!("[AGENT-SPEND] SUCCESS: agent received (proof-bearing mint), spent autonomously (proof-bearing send), held above-limit, balances 100 -> 90 on the public testnet");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
