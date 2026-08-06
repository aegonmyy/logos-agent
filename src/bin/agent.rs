//! `agent` — deploy and run a Logos autonomous agent headless, with one command.
//!
//! Reads the wallet/sequencer connection from the standard LEE wallet
//! environment, gives the agent a shielded identity and a spending limit, opens
//! the owner channel over Logos Messaging, and runs the event loop that applies
//! the owner's approvals and configuration changes.
//!
//! Example:
//!   agent --owner <owner-identity> --spending-limit 50

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use clap::Parser;
use logos_agent::messaging::WakuMessaging;
use logos_agent::owner::{AgentRuntime, OwnerChannel};
use logos_agent::{Agent, SpendingPolicy};
use wallet::WalletCore;

#[derive(Parser)]
#[command(name = "agent", about = "Run a Logos autonomous agent headless")]
struct Args {
    /// Owner identity the agent reports to for spending approvals.
    #[arg(long, env = "AGENT_OWNER")]
    owner: String,

    /// Per-transaction spending limit (tokens) the agent may spend on its own.
    #[arg(long, env = "AGENT_SPENDING_LIMIT", default_value_t = 0)]
    spending_limit: u128,

    /// nwaku REST endpoint for Logos Messaging.
    #[arg(
        long,
        env = "AGENT_MESSAGING_URL",
        default_value = "http://127.0.0.1:8645"
    )]
    messaging_url: String,

    /// How often to poll the owner channel, in seconds.
    #[arg(long, default_value_t = 5)]
    poll_secs: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Wallet + sequencer connection come from the standard LEE wallet env.
    let mut wallet = WalletCore::from_env().context("initialising wallet from environment")?;

    // Give the agent its shielded identity and spending policy.
    let agent = Agent::create(
        &mut wallet,
        SpendingPolicy {
            per_tx_limit: args.spending_limit,
        },
    )
    .await
    .context("creating the agent account")?;
    let account_id = agent.account_id();
    println!("agent account: {account_id}");

    // Open the owner channel over Logos Messaging and run the event loop.
    let messaging = Arc::new(WakuMessaging::new(args.messaging_url));
    let channel = OwnerChannel::open(messaging, &account_id, &args.owner);
    let mut runtime = AgentRuntime::new(agent, channel);

    println!(
        "agent deployed; awaiting owner instructions (spending limit {}).",
        args.spending_limit
    );
    loop {
        wallet.sync_to_latest_block().await.ok();
        for resolved in runtime.process_owner_messages(&mut wallet).await? {
            println!("resolved: {resolved:?}");
        }
        tokio::time::sleep(Duration::from_secs(args.poll_secs)).await;
    }
}
