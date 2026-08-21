//! Stage-3 end-to-end proof: an over-limit spend is held, surfaced to the owner
//! over the owner channel, and only moves funds once the owner approves. Denials
//! move nothing; a reconfigure raises the limit so the next spend is autonomous.
//! Money movement is against the live chain; the owner channel is Logos Messaging
//! (in-memory backend here).

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use logos_agent::messaging::InMemoryMessaging;
use logos_agent::owner::{AgentRuntime, OwnerChannel, Resolved, SpendDecision};
use logos_agent::{Agent, SpendingPolicy};
use test_fixtures::{TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext, private_mention, public_mention};
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};

async fn new_account(ctx: &mut TestContext, private: bool) -> Result<lee::AccountId> {
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
    let result = wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Account(sub)).await?;
    let SubcommandReturnValue::RegisterAccount { account_id } = result else {
        bail!("expected a registered account id");
    };
    Ok(account_id)
}

async fn wait_for_block() {
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;
}

#[tokio::test]
async fn owner_approves_denies_and_reconfigures() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition = new_account(&mut ctx, false).await?;
    let agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 30,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;
    let recipient = new_account(&mut ctx, true).await?;

    // Fund the agent with 100 tokens.
    wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(agent.account_id()),
            name: "OWNER-COIN".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;
    wait_for_block().await;

    // Wire the owner channel over Logos Messaging. Both sides derive the same
    // topics from the (agent, owner) pair.
    let messaging = Arc::new(InMemoryMessaging::new());
    let agent_id = agent.account_id();
    let owner = "owner-identity";
    let owner_view = OwnerChannel::open(Arc::clone(&messaging) as Arc<_>, &agent_id, owner);
    let channel = OwnerChannel::open(Arc::clone(&messaging) as Arc<_>, &agent_id, owner);
    let mut runtime = AgentRuntime::new(agent, channel);

    let balance = |ctx: &TestContext, runtime: &AgentRuntime| {
        runtime.agent().balance(ctx.wallet(), definition)
    };

    // (1) Over-limit spend of 50 (> 30): held, request sent to owner.
    let decision = runtime
        .propose_send(ctx.wallet_mut(), recipient, 50)
        .await?;
    let SpendDecision::Pending { id: id_a } = decision else {
        bail!("expected the 50-token spend to be held for approval");
    };
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        balance(&ctx, &runtime),
        100,
        "no funds move before approval"
    );

    // Owner sees the request.
    let requests = owner_view.poll_agent_requests().await?;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["amount"], "50");
    assert_eq!(requests[0]["id"], id_a.as_str());

    // (2) Owner approves → the spend executes.
    owner_view.decide(&id_a, true).await?;
    let resolved = runtime.process_owner_messages(ctx.wallet_mut()).await?;
    assert_eq!(
        resolved,
        vec![Resolved::Executed {
            id: id_a,
            amount: 50
        }]
    );
    wait_for_block().await;
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        balance(&ctx, &runtime),
        50,
        "approved spend should move funds"
    );

    // (3) Over-limit spend of 40, then owner denies → nothing moves.
    let SpendDecision::Pending { id: id_b } = runtime
        .propose_send(ctx.wallet_mut(), recipient, 40)
        .await?
    else {
        bail!("expected the 40-token spend to be held for approval");
    };
    owner_view.decide(&id_b, false).await?;
    let resolved = runtime.process_owner_messages(ctx.wallet_mut()).await?;
    assert_eq!(resolved, vec![Resolved::Denied { id: id_b }]);
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        balance(&ctx, &runtime),
        50,
        "denied spend must not move funds"
    );

    // (4) Owner raises the limit to 45; the next 40-token spend is autonomous.
    owner_view.configure_limit(45).await?;
    let resolved = runtime.process_owner_messages(ctx.wallet_mut()).await?;
    assert_eq!(resolved, vec![Resolved::Reconfigured { per_tx_limit: 45 }]);
    assert_eq!(runtime.agent().policy_limit(), 45);

    owner_view.configure_period(60, 86_400).await?;
    let resolved = runtime.process_owner_messages(ctx.wallet_mut()).await?;
    assert_eq!(
        resolved,
        vec![Resolved::PeriodReconfigured {
            per_period_limit: 60,
            period_seconds: 86_400,
        }]
    );
    assert_eq!(runtime.agent().period_policy(), (60, 86_400));

    let decision = runtime
        .propose_send(ctx.wallet_mut(), recipient, 40)
        .await?;
    assert_eq!(
        decision,
        SpendDecision::Executed {
            amount: 40,
            to: recipient
        }
    );
    wait_for_block().await;
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        balance(&ctx, &runtime),
        10,
        "raised limit lets the 40-token spend through"
    );

    Ok(())
}

/// The per-period limit is a real second gate: a spend that is *under* the
/// per-transaction limit but would push the period total over the period limit
/// is held for the owner, and only moves once approved. A subsequent spend is
/// held again (the approved spend counts against the period), and a denial
/// moves nothing. This is the half of the spending-threshold criterion the
/// per-transaction-only flow above does not exercise.
#[tokio::test]
async fn period_limit_holds_approves_and_denies() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition = new_account(&mut ctx, false).await?;
    // per-transaction limit is loose (100); the per-period limit (60) is the
    // binding constraint, so the only thing that can hold a spend here is the
    // period accumulator.
    let agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 100,
            per_period_limit: 60,
            period_seconds: 86_400,
        },
    )
    .await?;
    let recipient = new_account(&mut ctx, true).await?;

    // Fund the agent with 100 tokens.
    wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(agent.account_id()),
            name: "PERIOD-COIN".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;
    wait_for_block().await;

    let messaging = Arc::new(InMemoryMessaging::new());
    let agent_id = agent.account_id();
    let owner = "owner-identity";
    let owner_view = OwnerChannel::open(Arc::clone(&messaging) as Arc<_>, &agent_id, owner);
    let channel = OwnerChannel::open(Arc::clone(&messaging) as Arc<_>, &agent_id, owner);
    let mut runtime = AgentRuntime::new(agent, channel);

    let balance = |ctx: &TestContext, runtime: &AgentRuntime| {
        runtime.agent().balance(ctx.wallet(), definition)
    };

    // (1) 40 tokens: under both limits, executes autonomously. Period total = 40.
    let decision = runtime
        .propose_send(ctx.wallet_mut(), recipient, 40)
        .await?;
    assert_eq!(
        decision,
        SpendDecision::Executed {
            amount: 40,
            to: recipient
        }
    );
    wait_for_block().await;
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(balance(&ctx, &runtime), 60, "40-token spend executes");

    // (2) 30 more: under the per-tx limit (30 <= 100) but 40 + 30 = 70 > 60
    // period limit, so it is held for the owner. No funds move.
    let decision = runtime
        .propose_send(ctx.wallet_mut(), recipient, 30)
        .await?;
    let SpendDecision::Pending { id: id_a } = decision else {
        bail!("expected the 30-token spend to be held by the period limit");
    };
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        balance(&ctx, &runtime),
        60,
        "a period-over spend must not move funds before approval"
    );

    // The owner sees the request, with the period limit (not the per-tx limit)
    // as the tripped limit.
    let requests = owner_view.poll_agent_requests().await?;
    let req = requests
        .iter()
        .find(|r| r["id"].as_str() == Some(id_a.as_str()))
        .expect("owner sees the period-over approval request");
    assert_eq!(req["amount"], "30");
    assert_eq!(req["limit"], "60", "the tripped limit is the period limit");

    // (3) Owner approves -> the spend executes. Period total is now 70.
    owner_view.decide(&id_a, true).await?;
    let resolved = runtime.process_owner_messages(ctx.wallet_mut()).await?;
    assert_eq!(
        resolved,
        vec![Resolved::Executed {
            id: id_a,
            amount: 30
        }]
    );
    wait_for_block().await;
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(balance(&ctx, &runtime), 30, "approved period-over spend moves funds");

    // (4) 20 more: under the per-tx limit, but 70 + 20 = 90 > 60, so held again.
    // The approved spend counted against the period; the limit was not reset.
    let decision = runtime
        .propose_send(ctx.wallet_mut(), recipient, 20)
        .await?;
    let SpendDecision::Pending { id: id_b } = decision else {
        bail!("expected the 20-token spend to be held again by the period limit");
    };
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(balance(&ctx, &runtime), 30, "second period-over spend holds");

    // (5) Owner denies -> nothing moves.
    owner_view.decide(&id_b, false).await?;
    let resolved = runtime.process_owner_messages(ctx.wallet_mut()).await?;
    assert_eq!(resolved, vec![Resolved::Denied { id: id_b }]);
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        balance(&ctx, &runtime),
        30,
        "a denied period-over spend must not move funds"
    );

    Ok(())
}
