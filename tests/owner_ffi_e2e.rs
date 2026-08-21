//! LP-0008 owner-interaction criterion: the owner drives the agent from the
//! FFI owner handle — the exact Rust type the C ABI (`logos_agent_owner_*`) and
//! the Basecamp C++/QML owner app wrap — and the agent's approved spend executes
//! as a real on-chain transfer on the live local LEZ sequencer.
//!
//! This closes the loop the unit test (`ffi::tests::owner_channel_round_trips`)
//! and the runtime test (`owner_approval_flow`) each cover one half of: here a
//! single run goes FFI-handle poll → decide → on-chain execution, over Logos
//! Messaging with no intermediary server.
//!
//! The test is a plain (non-async) test that drives each async step through an
//! explicit runtime, because `OwnerChannelHandle` manages its own tokio runtime
//! the way the C/Qt caller does; calling it from inside `#[tokio::test]` would
//! panic ("cannot start a runtime from within a runtime").

use std::sync::Arc;

use logos_agent::ffi::OwnerChannelHandle;
use logos_agent::messaging::InMemoryMessaging;
use logos_agent::owner::{AgentRuntime, OwnerChannel, Resolved, SpendDecision};
use logos_agent::{Agent, SpendingPolicy};
use test_fixtures::{TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext, private_mention, public_mention};
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};

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
fn owner_ffi_handle_holds_approves_denies_and_reconfigures_on_chain() -> anyhow::Result<()> {
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

    // Fund the agent with 100 tokens.
    rt.block_on(wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(agent.account_id()),
            name: "FFI-OWNER-COIN".to_owned(),
            total_supply: 100,
        }),
    ))?;

    // Agent side: the runtime with its owner channel over Logos Messaging.
    // Owner side: the FFI handle over the same backend — the interface the
    // Basecamp app calls through the C ABI.
    let messaging = Arc::new(InMemoryMessaging::new());
    let agent_id = agent.account_id();
    let channel = OwnerChannel::open(Arc::clone(&messaging) as Arc<_>, &agent_id, "owner");
    let mut runtime = AgentRuntime::new(agent, channel);
    let owner = OwnerChannelHandle::from_messaging(
        Arc::clone(&messaging) as Arc<_>,
        &agent_id.to_string(),
        "owner",
    )?;

    let balance =
        |ctx: &TestContext, runtime: &AgentRuntime| runtime.agent().balance(ctx.wallet(), definition);

    // (1) Over-limit spend of 50 (> 30): held; the request reaches the owner.
    let decision = rt.block_on(runtime.propose_send(ctx.wallet_mut(), recipient, 50))?;
    let SpendDecision::Pending { id: id_a } = decision else {
        anyhow::bail!("expected the 50-token spend to be held for approval");
    };
    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
    assert_eq!(balance(&ctx, &runtime), 100, "no funds move before approval");

    // The owner sees the request through the FFI handle.
    let requests = owner.poll()?;
    let seen = requests
        .iter()
        .find(|req| req["id"].as_str() == Some(id_a.as_str()))
        .expect("FFI owner handle sees the approval request");
    assert_eq!(seen["amount"], "50");

    // (2) The owner approves through the FFI handle; the spend executes on-chain.
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
    assert_eq!(balance(&ctx, &runtime), 50, "the FFI-approved spend moves funds");

    // (3) A second over-limit spend, denied through the FFI handle: no movement.
    let decision = rt.block_on(runtime.propose_send(ctx.wallet_mut(), recipient, 40))?;
    let SpendDecision::Pending { id: id_b } = decision else {
        anyhow::bail!("expected the 40-token spend to be held for approval");
    };
    owner.decide(&id_b, false)?;
    let resolved = rt.block_on(runtime.process_owner_messages(ctx.wallet_mut()))?;
    assert_eq!(resolved, vec![Resolved::Denied { id: id_b }]);
    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
    assert_eq!(balance(&ctx, &runtime), 50, "the FFI-denied spend must not move funds");

    // (4) The owner raises the limit through the FFI handle; the next spend is
    // autonomous (no approval round-trip).
    owner.configure_limit(45)?;
    let resolved = rt.block_on(runtime.process_owner_messages(ctx.wallet_mut()))?;
    assert_eq!(resolved, vec![Resolved::Reconfigured { per_tx_limit: 45 }]);
    let decision = rt.block_on(runtime.propose_send(ctx.wallet_mut(), recipient, 40))?;
    assert_eq!(
        decision,
        SpendDecision::Executed {
            amount: 40,
            to: recipient
        }
    );
    rt.block_on(async {
        tokio::time::sleep(std::time::Duration::from_secs(
            TIME_TO_WAIT_FOR_BLOCK_SECONDS,
        ))
        .await;
    });
    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
    assert_eq!(balance(&ctx, &runtime), 10, "the raised limit lets the spend through");

    println!(
        "owner_ffi_e2e=ok agent={agent_id} token={definition} approved=50 denied=40 reconfigured_per_tx=45 final_balance=10"
    );

    // TestContext::Drop tears down the Docker stack via `testcontainers`, whose
    // async drop needs an active reactor. In a plain #[test] the locals would
    // drop outside any runtime, so drop the context inside rt.block_on. (The
    // FFI handle and the agent runtime drop normally afterward; they do not
    // need a reactor.)
    rt.block_on(async move {
        drop(ctx);
    });
    Ok(())
}
