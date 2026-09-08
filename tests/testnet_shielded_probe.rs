//! Shielded (PrivacyPreserving) token Send on the public LEZ testnet at
//! RISC0_DEV_MODE=0. The on-chain transaction carries a real Groth16 proof in
//! its witness set: hundreds of kilobytes, type byte 0x01, not a 271-byte
//! public transaction with no proof.
//!
//! This is the evidence the reviewer asked for: a public-testnet transaction
//! that carries a proof, reproducible at real-proof mode. The run on
//! 2026-09-08 landed a 271,076-byte PrivacyPreserving transaction in block
//! 42709 (tx 606bb9b5...); the RPC snapshot is committed at
//! docs/testnet-evidence/v0.1.0/rpc/shielded-send-01.json.
//!
//! The testnet exposes `getProofsAndRoot` (verified 2026-09-08), so the
//! wallet can construct the PrivacyPreserving transaction. The prior
//! "private txs dropped" finding (TESTNET_EVIDENCE.md, 2026-08-22) was a
//! transient sequencer condition; it is no longer reproducible.
//!
//! Ignored by default (hits the network, slow: real Groth16 proving is
//! ~10-15 min). Run with:
//!   RISC0_DEV_MODE=0 cargo test --test testnet_shielded_probe -- --ignored --nocapture --test-threads=1

use std::time::Duration;

use anyhow::{Result, bail};
use test_fixtures::{private_mention, public_mention};
use token_core::TokenHolding;
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
                            eprintln!("[{}] transient (attempt {attempt}): {m}; retrying", $label);
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

async fn public_token_balance(wallet: &WalletCore, account: lee::AccountId) -> Result<u128> {
    let acc = wallet.get_account_public(account).await?;
    match TokenHolding::try_from(&acc.data) {
        Ok(TokenHolding::Fungible { balance, .. }) => Ok(balance),
        _ => Ok(0),
    }
}

/// Fetch the on-chain transaction and return (total_size_bytes, type_byte).
/// The type byte is 0x00 for Public (no proof) and 0x01 for
/// PrivacyPreserving (carries the ZK proof in the witness set).
async fn on_chain_tx_size_and_type(
    tx_hash: &str,
    url: &str,
) -> Result<(usize, u8)> {
    use base64::Engine;
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getTransaction",
        "params": [tx_hash],
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;
    let resp: serde_json::Value = client
        .post(url)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    let result = resp
        .get("result")
        .ok_or_else(|| anyhow::anyhow!("no result in getTransaction response"))?;
    let arr = result
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("result is not an array (tx not on chain)"))?;
    let mut total = 0usize;
    let mut type_byte = 0u8;
    for elem in arr {
        if let Some(s) = elem.as_str() {
            let raw = base64::engine::general_purpose::STANDARD.decode(s)?;
            total += raw.len();
            if type_byte == 0 {
                type_byte = raw[0];
            }
        }
    }
    Ok((total, type_byte))
}

#[tokio::test]
#[ignore = "hits the live public LEZ testnet with a real shielded tx; slow (~15 min proving)"]
async fn shielded_send_lands_on_public_testnet() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("agent-shielded-probe-{}", std::process::id()));
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

    let start = net_retry!(wallet.sync_to_latest_block(), "initial-sync");
    eprintln!("PROBE: testnet start block: {start}");

    // Mint to a PUBLIC supply account (known to land), then send to a PRIVATE
    // recipient. The send to a private account is a PrivacyPreserving tx, which
    // is the proof-bearing transaction class we are verifying.
    let definition = net_retry!(new_public_account(&mut wallet), "new-definition");
    let holder = net_retry!(new_public_account(&mut wallet), "new-holder");
    let recipient = net_retry!(new_private_account(&mut wallet), "new-private-recipient");
    eprintln!("PROBE: definition={definition}");
    eprintln!("PROBE: holder (public)={holder}");
    eprintln!("PROBE: recipient (private)={recipient}");

    // 1. Mint 100 tokens to the public holder.
    eprintln!("PROBE: minting 100 tokens to public holder...");
    wallet::cli::execute_subcommand(
        &mut wallet,
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: public_mention(holder),
            name: "SHIELDED-PROBE".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(20)).await;
    net_retry!(wallet.sync_to_latest_block(), "sync-after-mint");
    let minted = net_retry!(public_token_balance(&wallet, holder), "balance-mint");
    eprintln!("PROBE: holder balance after mint: {minted}");
    assert_eq!(minted, 100, "mint should land on the testnet");

    // 2. The shielded send: 10 tokens, public -> private. This is a
    // PrivacyPreserving transaction whose on-chain body carries the ZK proof.
    eprintln!("PROBE: submitting shielded send (10 tokens, public -> private)...");
    let send_result = wallet::cli::execute_subcommand(
        &mut wallet,
        Command::Token(TokenProgramAgnosticSubcommand::Send {
            from: public_mention(holder),
            to: Some(private_mention(recipient)),
            to_npk: None,
            to_vpk: None,
            to_keys: None,
            to_identifier: Some(0),
            amount: 10,
        }),
    )
    .await;

    let tx_hash = match &send_result {
        Ok(rv) => {
            let s = format!("{rv:?}");
            // TransactionExecuted { tx_hash: <64 hex> }
            let h = s
                .split("tx_hash:")
                .nth(1)
                .and_then(|t| t.split(|c: char| !c.is_ascii_hexdigit()).next())
                .unwrap_or("")
                .trim();
            eprintln!("PROBE: send submitted, tx_hash={h}");
            h.to_owned()
        }
        Err(e) => {
            eprintln!("PROBE: send FAILED to submit: {e}");
            let _ = std::fs::remove_dir_all(&dir);
            bail!("shielded send failed to submit: {e}");
        }
    };
    assert_eq!(tx_hash.len(), 64, "tx hash should be 64 hex chars");

    // 3. Wait for inclusion.
    eprintln!("PROBE: waiting for inclusion...");
    tokio::time::sleep(Duration::from_secs(30)).await;
    net_retry!(wallet.sync_to_latest_block(), "sync-after-send");

    // 4. Assert the balance moved: 100 -> 90. This is the on-chain settlement.
    let holder_after = net_retry!(public_token_balance(&wallet, holder), "balance-holder-after");
    eprintln!("PROBE: holder balance after send: {holder_after}");
    assert_eq!(
        holder_after, 90,
        "shielded send should settle on chain: holder 100 -> 90"
    );

    // 5. Assert the on-chain transaction carries a proof. A PrivacyPreserving
    // tx is hundreds of KB (the proof) with type byte 0x01; a Public tx is a
    // few hundred bytes with type byte 0x00. This is the assertion that closes
    // the "304-byte no proof" objection: the transaction is large because it
    // carries the Groth16 proof.
    let (size, type_byte) = on_chain_tx_size_and_type(&tx_hash, &url).await?;
    eprintln!("PROBE: on-chain tx size: {size} bytes, type byte: 0x{type_byte:02x}");
    assert_eq!(
        type_byte, 0x01,
        "the shielded send must be a PrivacyPreserving transaction (type 0x01), not Public (0x00)"
    );
    assert!(
        size > 100_000,
        "a PrivacyPreserving tx at DEV_MODE=0 carries the ZK proof and is hundreds of KB; got {size} bytes"
    );
    eprintln!("PROBE: SUCCESS - shielded send landed and carries a real ZK proof ({size} bytes)");

    let end = net_retry!(wallet.sync_to_latest_block(), "final-sync");
    eprintln!("PROBE: testnet end block: {end} (started at {start})");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
