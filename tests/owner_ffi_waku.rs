//! LP-0008 owner-interaction criterion over REAL Waku: the owner drives the
//! agent from the FFI owner handle through a live nwaku node (the Logos Dev
//! Network, cluster 2) — the real deployment topology, where the headless agent
//! and the separate Basecamp owner app each run their own Waku client and talk
//! through the node with no intermediary server. The approved spend then
//! executes as a real on-chain transfer on the live local LEZ sequencer.
//!
//! Ignored by default because it needs a running nwaku node + the local
//! sequencer. Bring up nwaku and run:
//!   docker run -d --name nwaku -p 8645:8645 wakuorg/nwaku:v0.38.0 \
//!     --rest=true --rest-address=0.0.0.0 --rest-port=8645 --relay=true --cluster-id=2
//!   RISC0_DEV_MODE=0 cargo test --test owner_ffi_waku -- --ignored --nocapture
//!
//! This is the Waku-transport companion to `tests/owner_ffi_e2e.rs` (which uses
//! the in-memory backend and runs in CI). The flow is identical; only the
//! messaging transport differs.

use std::sync::Arc;

use logos_agent::ffi::OwnerChannelHandle;
use logos_agent::messaging::{Messaging, WakuMessaging};
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
        AccountSubcommand::New(NewSubcommand::Private {
            cci: None,
            label: None,
        })
    } else {
        AccountSubcommand::New(NewSubcommand::Public {
            cci: None,
            label: None,
        })
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

#[test]
#[ignore = "requires a running nwaku node (cluster-id=2) and the local sequencer"]
fn owner_ffi_handle_over_waku_holds_approves_and_executes() -> anyhow::Result<()> {
    let url = messaging_url();
    let rt = tokio::runtime::Runtime::new()?;
    let mut ctx = rt.block_on(TestContext::new())?;

    let definition = new_account(&rt, &mut ctx, false)?;
    let agent = rt.block_on(Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 30,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    ))?;
    let recipient = new_account(&rt, &mut ctx, true)?;

    rt.block_on(wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(agent.account_id()),
            name: "WAKU-OWNER-COIN".to_owned(),
            total_supply: 100,
        }),
    ))?;

    // Agent side: its owner channel over a real Waku client. Owner side: the
    // FFI handle over its own Waku client. Both point at the same nwaku node;
    // they never share memory — this is the separate-app topology.
    let agent_id = agent.account_id();
    let owner_label = "owner-identity";
    let agent_messaging: Arc<dyn Messaging> = Arc::new(WakuMessaging::new(url.clone()));
    let channel = OwnerChannel::open(agent_messaging, &agent_id, owner_label);
    // Waku only stores/serves messages for subscribed content topics; subscribe
    // both channel topics on the node (once, from either side) before the flow.
    rt.block_on(channel.subscribe())?;
    let mut runtime = AgentRuntime::new(agent, channel);

    let balance = |ctx: &TestContext, runtime: &AgentRuntime| {
        runtime.agent().balance(ctx.wallet(), definition)
    };

    // (1) Over-limit spend of 50 (> 30): held; the request is posted over Waku.
    eprintln!("[waku-test] subscribed; proposing over-limit spend");
    let decision = match rt.block_on(runtime.propose_send(ctx.wallet_mut(), recipient, 50)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[waku-test] propose_send FAILED: {e:#}");
            return Err(e);
        }
    };
    let SpendDecision::Pending { id: id_a } = decision else {
        anyhow::bail!("expected the 50-token spend to be held for approval");
    };

    // Construct the owner handle after the agent has posted, so the FFI
    // handle's own tokio runtime never coexists with the agent-side Waku HTTP
    // call (each runs its reqwest client on its own runtime, uncontended).
    let owner = OwnerChannelHandle::new(&url, &agent_id.to_string(), owner_label)?;

    // The owner polls the live Waku node and sees the request through the FFI
    // handle. (Waku stores messages locally on a single node, so the round-trip
    // works without a relay mesh.)
    let mut seen = None;
    for attempt in 1..=20 {
        let requests = owner.poll()?;
        if attempt <= 2 || attempt % 10 == 0 {
            eprintln!("[waku-test] poll attempt {attempt}: {} request(s)", requests.len());
        }
        if let Some(req) = requests
            .iter()
            .find(|req| req["id"].as_str() == Some(id_a.as_str()))
        {
            assert_eq!(req["amount"], "50");
            seen = Some(());
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = attempt;
    }
    assert!(seen.is_some(), "FFI owner handle did not see the request over Waku");

    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
    assert_eq!(balance(&ctx, &runtime), 100, "no funds move before approval");

    // (2) The owner approves over Waku; the agent applies it and executes on-chain.
    eprintln!("[waku-test] owner saw request; approving over Waku");
    owner.decide(&id_a, true)?;
    let resolved = rt.block_on(runtime.process_owner_messages(ctx.wallet_mut()))?;
    assert_eq!(
        resolved,
        vec![Resolved::Executed {
            id: id_a,
            amount: 50
        }]
    );
    rt.block_on(async {
        tokio::time::sleep(std::time::Duration::from_secs(
            TIME_TO_WAIT_FOR_BLOCK_SECONDS,
        ))
        .await;
    });
    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
    assert_eq!(balance(&ctx, &runtime), 50, "the Waku-approved spend moves funds");

    println!(
        "owner_ffi_waku=ok agent={agent_id} token={definition} url={url} approved=50 final_balance=50"
    );

    // Teardown: TestContext::Drop needs an active reactor (testcontainers).
    rt.block_on(async move {
        drop(ctx);
    });
    Ok(())
}
