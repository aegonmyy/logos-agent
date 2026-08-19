//! Stage-2 end-to-end proof: the agent's skills are invoked *by name* through
//! the skill registry — reflection (`meta.*`), reading balance, and spending
//! under the owner's policy — all against a live local chain.

use std::time::Duration;

use anyhow::{Result, bail};
use logos_agent::skills::{SkillContext, SkillRegistry};
use logos_agent::{Agent, SpendingPolicy};
use serde_json::json;
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
async fn skills_are_dispatched_by_name() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition = new_account(&mut ctx, false).await?;
    let agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 50,
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
            name: "SKILL-COIN".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;
    wait_for_block().await;

    let registry = SkillRegistry::with_defaults();

    // meta.skills — reflection lists the catalogue, including the money skills.
    {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        let catalogue = registry
            .dispatch("meta.skills", &mut sctx, json!({}))
            .await?;
        let names: Vec<String> = catalogue
            .as_array()
            .expect("catalogue is an array")
            .iter()
            .map(|item| item["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        for expected in [
            "wallet.balance",
            "wallet.send",
            "meta.skills",
            "meta.status",
        ] {
            assert!(
                names.contains(&expected.to_owned()),
                "missing skill {expected}"
            );
        }
    }

    // wallet.balance — reads the funded balance.
    {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        let result = registry
            .dispatch(
                "wallet.balance",
                &mut sctx,
                json!({ "token": definition.to_string() }),
            )
            .await?;
        assert_eq!(result["balance"], "100");
    }

    // wallet.send under the limit — executes autonomously.
    {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        let result = registry
            .dispatch(
                "wallet.send",
                &mut sctx,
                json!({ "to": recipient.to_string(), "amount": 10 }),
            )
            .await?;
        assert_eq!(result["status"], "executed");
    }
    wait_for_block().await;
    {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        let result = registry
            .dispatch(
                "wallet.balance",
                &mut sctx,
                json!({ "token": definition.to_string() }),
            )
            .await?;
        assert_eq!(
            result["balance"], "90",
            "balance should drop after an autonomous send"
        );
    }

    // wallet.send over the limit — held for owner approval, no funds move.
    {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        let result = registry
            .dispatch(
                "wallet.send",
                &mut sctx,
                json!({ "to": recipient.to_string(), "amount": 75 }),
            )
            .await?;
        assert_eq!(result["status"], "needs_owner_approval");
        assert_eq!(result["limit"], "50");
    }
    wait_for_block().await;
    {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        let result = registry
            .dispatch(
                "wallet.balance",
                &mut sctx,
                json!({ "token": definition.to_string() }),
            )
            .await?;
        assert_eq!(
            result["balance"], "90",
            "an unapproved over-limit send must not move funds"
        );
    }

    // Unknown skill — dispatch errors rather than silently succeeding.
    {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        let result = registry
            .dispatch("does.not.exist", &mut sctx, json!({}))
            .await;
        assert!(result.is_err(), "unknown skill should error");
    }

    Ok(())
}
