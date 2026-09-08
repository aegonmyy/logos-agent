//! `agent-wallet-init` — create the wallet home the `agent` binary deploys from.
//!
//! The `agent` binary reads the wallet (and through it the sequencer
//! connection) from the standard LEE wallet environment: `LEE_WALLET_HOME_DIR`
//! pointing at a wallet directory. This companion creates that directory once,
//! pointed at a sequencer, so a fresh machine reaches a working
//! `agent` deploy in two commands:
//!
//!   LEE_WALLET_HOME_DIR=~/.agent-wallet agent-wallet-init --sequencer https://testnet.lez.logos.co
//!   LEE_WALLET_HOME_DIR=~/.agent-wallet agent --owner <owner-id> --spending-limit 50
//!
//! An existing wallet home is left untouched (this tool refuses to overwrite
//! one); owners who already have a wallet skip this step entirely.

use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use clap::Parser;
use wallet::WalletCore;
use wallet::config::{SequencerConnectionData, WalletConfigOverrides};

#[derive(Parser)]
#[command(
    name = "agent-wallet-init",
    about = "Create the LEE wallet home the `agent` binary deploys from"
)]
struct Args {
    /// Sequencer RPC URL the wallet will talk to (LEZ testnet by default).
    #[arg(long, env = "AGENT_SEQUENCER_URL", default_value = "https://testnet.lez.logos.co")]
    sequencer: String,

    /// Wallet password. Read from this flag / AGENT_WALLET_PASSWORD, else
    /// prompted for on the terminal.
    #[arg(long, env = "AGENT_WALLET_PASSWORD")]
    password: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // The same three paths `WalletCore::from_env` (and so `agent`) resolves
    // later, so what this tool creates is exactly what the agent finds.
    let config_path = wallet::helperfunctions::fetch_config_path()
        .context("resolving the wallet home (set LEE_WALLET_HOME_DIR)")?;
    let home = config_path
        .parent()
        .context("wallet config path has no parent directory")?
        .to_path_buf();
    let storage_path = wallet::helperfunctions::fetch_persistent_storage_path()
        .context("resolving the wallet storage path")?;
    let statistics_path = wallet::helperfunctions::fetch_statistics_path()
        .context("resolving the wallet statistics path")?;

    if storage_path.exists() {
        bail!(
            "wallet home {} already exists; `agent` can deploy from it as is",
            home.display()
        );
    }
    std::fs::create_dir_all(&home)?;

    let sequencer_addr = args.sequencer.parse().with_context(|| {
        format!("parsing sequencer URL {}", args.sequencer)
    })?;
    let overrides = WalletConfigOverrides {
        sequencers: Some(vec![SequencerConnectionData {
            sequencer_addr,
            basic_auth: None,
        }]),
        // Same polling patience the integration tests use against the public
        // testnet, so the agent's background sync rides out sequencer flapping.
        seq_tx_poll_max_blocks: Some(400),
        seq_poll_max_retries: Some(4000),
        seq_poll_timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    };

    let password = match args.password {
        Some(p) => p,
        None => wallet::cli::read_password_from_stdin()?,
    };

    let (wallet, mnemonic) = WalletCore::new_init_storage(
        config_path,
        storage_path,
        statistics_path,
        Some(overrides),
        &password,
    )
    .await
    .context("initialising the wallet storage")?;
    // Write the storage to disk: `WalletCore::from_env` (and so `agent`)
    // loads it from storage_path on every start.
    wallet
        .store_persistent_data()
        .context("persisting the wallet storage")?;

    println!("wallet home: {}", home.display());
    println!("sequencer:   {}", args.sequencer);
    println!();
    println!("Recovery phrase (store securely; the agent's keys live in this wallet):");
    println!("  {mnemonic}");
    println!();
    println!("Wallet ready. Deploy an agent from it with:");
    println!("  agent --owner <owner-id> --spending-limit 50 \\");
    println!("        --messaging-url http://127.0.0.1:8645");
    Ok(())
}
