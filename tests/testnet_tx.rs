//! Real on-chain transactions on the live public LEZ testnet, with real proofs
//! (RISC0_DEV_MODE=0), submitted to and accepted by the testnet sequencer.
//!
//! Note: the testnet's LEZ build does not expose `getProofForCommitment`, the
//! shielded-pool RPC our private transactions need, so the agent's *shielded*
//! flow can't complete there. This exercises the same token program with *public*
//! accounts — a real, proof-backed mint + transfer on the live testnet.
//!
//! Ignored by default (needs the network, slow). Run with:
//!   RISC0_DEV_MODE=0 cargo test -p logos_agent --test testnet_tx -- --ignored --nocapture

use std::time::Duration;

use anyhow::{Result, bail};
use test_fixtures::public_mention;
use token_core::TokenHolding;
use wallet::WalletCore;
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};
use wallet::config::WalletConfigOverrides;

const TESTNET: &str = "https://seq-testnet.paradox.computer";

/// The public endpoint flaps (its upstream sequencer goes up and down for
/// minutes at a time behind an nginx that then returns 502). Retry a network op
/// through those transient failures, re-evaluating the expression each attempt.
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

async fn public_token_balance(wallet: &WalletCore, account: lee::AccountId) -> Result<u128> {
    let acc = wallet.get_account_public(account).await?;
    match TokenHolding::try_from(&acc.data) {
        Ok(TokenHolding::Fungible { balance, .. }) => Ok(balance),
        _ => Ok(0),
    }
}

#[tokio::test]
#[ignore = "hits the live public LEZ testnet with real proofs; slow"]
async fn public_token_mint_and_transfer_on_testnet() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("agent-testnet-tx-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;

    // The public testnet produces blocks slowly, so wait patiently for inclusion.
    let overrides = WalletConfigOverrides {
        sequencer_addr: Some(TESTNET.parse().unwrap()),
        seq_tx_poll_max_blocks: Some(200),
        seq_poll_max_retries: Some(2000),
        seq_poll_timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    };
    let (mut wallet, _mnemonic) = WalletCore::new_init_storage(
        dir.join("config.json"),
        dir.join("storage"),
        Some(overrides),
        "testpw",
    )?;

    let start = net_retry!(wallet.sync_to_latest_block(), "initial-sync");
    println!("testnet start block: {start}");

    let definition = net_retry!(new_public_account(&mut wallet), "new-account");
    let holder = net_retry!(new_public_account(&mut wallet), "new-account");
    let recipient = net_retry!(new_public_account(&mut wallet), "new-account");
    println!("holder: {holder}");
    println!("recipient: {recipient}");

    // Mint 100 tokens to the holder — a real proof-backed transaction on testnet.
    wallet::cli::execute_subcommand(
        &mut wallet,
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: public_mention(holder),
            name: "TESTNET-COIN".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(20)).await;
    net_retry!(wallet.sync_to_latest_block(), "sync-after-mint");
    let minted = net_retry!(public_token_balance(&wallet, holder), "balance-mint");
    println!("holder balance after mint: {minted}");
    assert_eq!(minted, 100, "mint should land on the testnet");

    // Transfer 10 to the recipient — another real proof-backed transaction.
    wallet::cli::execute_subcommand(
        &mut wallet,
        Command::Token(TokenProgramAgnosticSubcommand::Send {
            from: public_mention(holder),
            to: Some(public_mention(recipient)),
            to_npk: None,
            to_vpk: None,
            to_keys: None,
            to_identifier: None,
            amount: 10,
        }),
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(20)).await;
    net_retry!(wallet.sync_to_latest_block(), "sync-after-transfer");
    let holder_after = net_retry!(public_token_balance(&wallet, holder), "balance-holder");
    let recipient_after = net_retry!(public_token_balance(&wallet, recipient), "balance-recipient");
    println!("holder: {holder_after}, recipient: {recipient_after}");
    assert_eq!(holder_after, 90, "holder should have sent 10 on the testnet");
    assert_eq!(recipient_after, 10, "recipient should have received 10 on the testnet");

    let end = net_retry!(wallet.sync_to_latest_block(), "final-sync");
    println!("testnet end block: {end} (started at {start})");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
