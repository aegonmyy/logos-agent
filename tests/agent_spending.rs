//! Stage-1 end-to-end proof: an agent with a shielded account holds tokens,
//! reports its balance, spends autonomously under its owner-set limit, and
//! refuses to spend above it without approval — all against a live local chain.

use std::time::Duration;

use anyhow::{Result, bail};
use logos_agent::{Agent, SpendOutcome, SpendingPolicy};
use test_fixtures::{TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext, private_mention, public_mention};
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};

async fn new_private_account(ctx: &mut TestContext) -> Result<lee::AccountId> {
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
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

async fn new_public_account(ctx: &mut TestContext) -> Result<lee::AccountId> {
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
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

async fn wait_for_block() {
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;
}

#[tokio::test]
async fn agent_spends_within_limit_and_holds_above_it() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // The token this scenario moves around. Its definition lives in a public
    // account; the agent will be the private holder of the whole supply.
    let definition = new_public_account(&mut ctx).await?;

    // Owner spins up the agent with a per-transaction limit of 50 tokens.
    let policy = SpendingPolicy {
        per_tx_limit: 50,
        per_period_limit: 0,
        period_seconds: 86_400,
    };
    let agent = Agent::create(ctx.wallet_mut(), policy).await?;

    // A counterparty the agent will pay.
    let recipient = new_private_account(&mut ctx).await?;

    // Fund the agent: mint a supply of 100 tokens straight into its account.
    let total_supply = 100;
    wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(agent.account_id()),
            name: "AGENT-COIN".to_owned(),
            total_supply,
        }),
    )
    .await?;
    wait_for_block().await;

    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        agent.balance(ctx.wallet(), definition),
        100,
        "agent should hold the full minted supply"
    );

    // Below the limit (10 <= 50): the agent sends on its own.
    let outcome = agent.send(ctx.wallet_mut(), recipient, 10).await?;
    assert_eq!(
        outcome,
        SpendOutcome::Executed {
            amount: 10,
            to: recipient
        },
        "a spend within the limit should execute autonomously"
    );
    wait_for_block().await;

    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        agent.balance(ctx.wallet(), definition),
        90,
        "agent balance should drop by the amount it sent"
    );

    // Above the limit (75 > 50): the agent must NOT send; it holds for approval.
    let balance_before = agent.balance(ctx.wallet(), definition);
    let outcome = agent.send(ctx.wallet_mut(), recipient, 75).await?;
    assert_eq!(
        outcome,
        SpendOutcome::NeedsOwnerApproval {
            amount: 75,
            to: recipient,
            limit: 50
        },
        "a spend above the limit should be held for owner approval"
    );
    wait_for_block().await;

    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        agent.balance(ctx.wallet(), definition),
        balance_before,
        "an unapproved over-limit spend must not move any funds"
    );

    Ok(())
}
