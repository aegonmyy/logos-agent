//! Recording driver for the Basecamp GUI demo.
//!
//! Creates an agent, funds it, opens the owner channel over real Waku, and
//! proposes spends that are held for owner approval. The Basecamp owner app
//! (loaded in logoscore on a virtual display) receives the pending requests
//! over Waku; the recording script drives the QML approve/deny/reconfigure
//! controls with xdotool. This driver polls for the owner's decisions (sent
//! over Waku from the QML UI) and executes or cancels the spends accordingly.
//!
//! Bring up nwaku and run:
//!   docker run -d --name nwaku -p 8645:8645 wakuorg/nwaku:v0.38.0 \
//!     --rest=true --rest-address=0.0.0.0 --rest-port=8645 --relay=true --cluster-id=2
//!   RISC0_DEV_MODE=0 cargo test --test record_gui_driver -- --ignored --nocapture
//!
//! The script scripts/record-gui-demo.sh orchestrates nwaku, logoscore, Xvfb,
//! xdotool, and ffmpeg around this test.

use std::sync::Arc;
use std::time::Duration;

use logos_agent::messaging::WakuMessaging;
use logos_agent::owner::{AgentRuntime, OwnerChannel, Resolved, SpendDecision};
use logos_agent::{Agent, SpendingPolicy};
use test_fixtures::{TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext, private_mention, public_mention};
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};

fn messaging_url() -> String {
    std::env::var("AGENT_MESSAGING_URL").unwrap_or_else(|_| "http://127.0.0.1:8645".into())
}

fn new_account(
    rt: &tokio::runtime::Runtime,
    ctx: &mut TestContext,
    private: bool,
) -> anyhow::Result<lee::AccountId> {
    let sub = if private {
        AccountSubcommand::New(NewSubcommand::Private { cci: None, label: None })
    } else {
        AccountSubcommand::New(NewSubcommand::Public { cci: None, label: None })
    };
    let result = rt.block_on(wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(sub),
    ))?;
    let SubcommandReturnValue::RegisterAccount { account_id } = result else {
        anyhow::bail!("expected a registered account id");
    };
    Ok(account_id)
}

/// Wait for the owner to resolve a pending spend, polling the owner channel.
/// Returns the resolution (Approved, Denied, or timeout). Transient Waku poll
/// errors are retried (a single HTTP hiccup must not abort the whole flow);
/// only sustained failure or a timeout bails.
fn wait_for_resolution(
    rt: &tokio::runtime::Runtime,
    runtime: &mut AgentRuntime,
    wallet: &mut wallet::WalletCore,
    timeout_secs: u64,
) -> anyhow::Result<Vec<Resolved>> {
    let start = std::time::Instant::now();
    let mut all_resolved = Vec::new();
    let mut consecutive_errors = 0u32;
    while start.elapsed() < Duration::from_secs(timeout_secs) {
        match rt.block_on(runtime.process_owner_messages(wallet)) {
            Ok(resolved) => {
                consecutive_errors = 0;
                if !resolved.is_empty() {
                    all_resolved.extend(resolved);
                    return Ok(all_resolved);
                }
            }
            Err(e) => {
                consecutive_errors += 1;
                eprintln!("poll error #{consecutive_errors} in wait_for_resolution: {e:#}");
                if consecutive_errors >= 15 {
                    anyhow::bail!("sustained polling failure in wait_for_resolution: {e:#}");
                }
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    anyhow::bail!("timed out waiting for owner decision after {timeout_secs}s")
}

/// Wait for a config change (limit update) to arrive over the owner channel.
/// Transient poll errors are retried, same as [`wait_for_resolution`].
fn wait_for_reconfigure(
    rt: &tokio::runtime::Runtime,
    runtime: &mut AgentRuntime,
    wallet: &mut wallet::WalletCore,
    timeout_secs: u64,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let mut consecutive_errors = 0u32;
    while start.elapsed() < Duration::from_secs(timeout_secs) {
        match rt.block_on(runtime.process_owner_messages(wallet)) {
            Ok(resolved) => {
                consecutive_errors = 0;
                for r in &resolved {
                    println!("reconfigure event: {r:?}");
                    if matches!(r, Resolved::Reconfigured { .. }) {
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                consecutive_errors += 1;
                eprintln!("poll error #{consecutive_errors} in wait_for_reconfigure: {e:#}");
                if consecutive_errors >= 15 {
                    anyhow::bail!("sustained polling failure in wait_for_reconfigure: {e:#}");
                }
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    anyhow::bail!("timed out waiting for reconfigure after {timeout_secs}s")
}

#[test]
#[ignore = "requires nwaku + local sequencer + logoscore with owner module"]
fn record_gui_demo_flow() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let mut ctx = rt.block_on(TestContext::new())?;

    // Run the flow in an inner function so that `ctx` is never dropped outside
    // the runtime on an error path. TestContext::Drop tears down the Docker
    // stack via testcontainers, whose async drop needs an active reactor; in a
    // plain #[test] the locals would drop outside any runtime and panic (same
    // pattern as tests/owner_ffi_e2e.rs). We drop ctx inside rt.block_on
    // regardless of whether the flow succeeded, then return the real result.
    let result = record_gui_demo_flow_inner(&rt, &mut ctx);
    rt.block_on(async move { drop(ctx); });
    result
}

fn record_gui_demo_flow_inner(
    rt: &tokio::runtime::Runtime,
    ctx: &mut TestContext,
) -> anyhow::Result<()> {
    let url = messaging_url();

    // Create the agent account (public, so it can receive token mints).
    let definition = new_account(rt, ctx, false)?;
    let agent = rt.block_on(Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 30,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    ))?;
    let account_id = agent.account_id();
    let recipient = new_account(rt, ctx, true)?;

    // Fund the agent with 100 tokens.
    rt.block_on(wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(account_id),
            name: "RECORD-COIN".to_owned(),
            total_supply: 100,
        }),
    ))?;

    // Open the owner channel over Waku.  The owner is identified by a string
    // label (not an AccountId) — the Basecamp owner plugin reads the same label
    // from LOGOS_AGENT_OWNER_ID and opens the matching channel.
    let owner_label = "owner-recording";
    let messaging: Arc<dyn logos_agent::messaging::Messaging> = Arc::new(WakuMessaging::new(url));
    let channel = OwnerChannel::open(messaging, &account_id, owner_label);
    // Waku only stores/serves messages for subscribed content topics; subscribe
    // both channel topics on the node before the flow.
    rt.block_on(channel.subscribe())?;
    // Persist runtime state under the recording work dir (not the repo root)
    // so a stale repo-root state file can never bleed into a run. A leftover
    // `consumed` counter would make process_owner_messages skip the first
    // decision message on the owner channel (skip(consumed) steps over it),
    // so the approval never lands and the driver times out.
    let state_path = std::env::var("HOME").unwrap_or_else(|_| "/home/ubuntu".into())
        + "/recording/work/record-state.json";
    let mut runtime = AgentRuntime::with_state(agent, channel, std::path::PathBuf::from(state_path))?;

    let balance = |ctx: &TestContext, runtime: &AgentRuntime| {
        runtime.agent().balance(ctx.wallet(), definition)
    };

    // Write the agent account ID and owner label for the recording script.
    let ids_path = std::env::var("HOME").unwrap_or_else(|_| "/home/ubuntu".into())
        + "/recording/work/agent-ids.env";
    std::fs::write(
        &ids_path,
        format!(
            "export LOGOS_AGENT_ACCOUNT_ID=\"{account_id}\"\nexport LOGOS_AGENT_OWNER_ID=\"{owner_label}\"\n",
        ),
    )?;
    println!("AGENT_IDS_WRITTEN: account={account_id} owner={owner_label}");
    println!("agent balance: {}", balance(ctx, &runtime));

    // Wait for logoscore to signal the owner module is loaded and connected.
    // The recording script touches logoscore-ready.flag after loading agent_owner.
    let ready_path = std::env::var("HOME").unwrap_or_else(|_| "/home/ubuntu".into())
        + "/recording/work/logoscore-ready.flag";
    println!("waiting for logoscore ready flag at {ready_path}...");
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(120) {
        if std::path::Path::new(&ready_path).exists() {
            println!("logoscore ready, proceeding");
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    if !std::path::Path::new(&ready_path).exists() {
        anyhow::bail!("logoscore did not become ready within 120s");
    }
    // Give the owner module a moment to connect to Waku and subscribe.
    std::thread::sleep(Duration::from_secs(5));

    // (1) Over-limit spend of 50 (> 30): held for owner approval.
    let decision = rt.block_on(runtime.propose_send(ctx.wallet_mut(), recipient, 50))?;
    assert!(matches!(decision, SpendDecision::Pending { .. }), "expected the 50-token spend to be held");
    assert_eq!(balance(ctx, &runtime), 100, "no funds move before approval");
    println!("READY:approve");
    println!("pending: 50 tokens to recipient, over the 30 per-tx limit");

    // Wait for the QML UI to click Approve.
    let resolved = wait_for_resolution(&rt, &mut runtime, ctx.wallet_mut(), 600)?;
    println!("APPROVED: {:?}", resolved);
    // Wait for the on-chain transfer to settle.
    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
    std::thread::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS));
    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
    let bal = balance(ctx, &runtime);
    println!("balance after approve: {bal}");
    assert_eq!(bal, 50, "the approved spend moves funds");

    // (2) Another over-limit spend: held, then denied from the QML UI.
    let decision = rt.block_on(runtime.propose_send(ctx.wallet_mut(), recipient, 50))?;
    assert!(matches!(decision, SpendDecision::Pending { .. }), "expected the second spend to be held");
    println!("READY:deny");
    println!("pending: 50 tokens, over the 30 per-tx limit");

    let resolved = wait_for_resolution(&rt, &mut runtime, ctx.wallet_mut(), 600)?;
    println!("DENIED: {:?}", resolved);
    let bal = balance(ctx, &runtime);
    assert_eq!(bal, 50, "the denied spend must not move funds");
    println!("balance after deny: {bal}");

    // (3) Wait for the QML UI to raise the per-tx limit to 45.
    println!("READY:reconfigure");
    println!("waiting for owner to set per-tx limit to 45...");
    wait_for_reconfigure(&rt, &mut runtime, ctx.wallet_mut(), 600)?;
    println!("RECONFIGURED: per-tx limit is now 45");
    let per_tx = runtime.agent().policy_limit();
    let (per_period, period_secs) = runtime.agent().period_policy();
    println!("current policy: per_tx={per_tx}, per_period={per_period}, period_secs={period_secs}");

    // (4) A 40-token spend is now under the 45 limit: executes autonomously.
    let decision = rt.block_on(runtime.propose_send(ctx.wallet_mut(), recipient, 40))?;
    assert!(matches!(decision, SpendDecision::Executed { .. }), "expected the 40-token spend to execute autonomously under the new 45 limit");
    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
    std::thread::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS));
    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
    let bal = balance(ctx, &runtime);
    println!("AUTONOMOUS: 40 tokens spent under the raised limit");
    println!("balance after autonomous spend: {bal}");
    assert_eq!(bal, 10, "the autonomous spend moves funds");

    println!("RECORDING_COMPLETE");
    Ok(())
}
