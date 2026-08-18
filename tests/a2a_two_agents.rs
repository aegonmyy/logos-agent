//! Stage-4 end-to-end proof: two agents discover each other via Agent Cards,
//! run a task through the A2A lifecycle, and settle the LEZ payment autonomously
//! — no owner in the loop. Coordination is over Logos Messaging (in-memory);
//! the payment is a real private transfer on the live chain.

use std::sync::Arc;

use anyhow::{Result, bail};
use logos_agent::a2a::{A2aClient, A2aProvider, TaskState};
use logos_agent::messaging::InMemoryMessaging;
use logos_agent::skills::{EchoSkill, SkillRegistry};
use logos_agent::{Agent, SpendingPolicy};
use serde_json::json;
use std::path::PathBuf;
use test_fixtures::{TestContext, private_mention, public_mention};
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

#[tokio::test]
async fn a2a_task_ledger_survives_restart() -> Result<()> {
    let account_id: lee::AccountId = "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid test account"))?;
    let provider_id: lee::AccountId = "8QCqovq4QLCZBkMToNs1sXmTAX6NCJzicJB3umRuQaeA"
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid provider account"))?;
    let path = PathBuf::from(format!(
        "/tmp/logos-agent-a2a-ledger-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let messaging = Arc::new(InMemoryMessaging::new());
    {
        let mut client = logos_agent::a2a::A2aClient::with_state(
            Agent::from_parts(
                account_id,
                SpendingPolicy {
                    per_tx_limit: 10,
                    per_period_limit: 0,
                    period_seconds: 86_400,
                },
            ),
            Arc::clone(&messaging) as Arc<_>,
            path.clone(),
        )?;
        client
            .run_task_with_payment(
                &A2aProvider::new(
                    Agent::from_parts(
                        provider_id,
                        SpendingPolicy {
                            per_tx_limit: 0,
                            per_period_limit: 0,
                            period_seconds: 86_400,
                        },
                    ),
                    Arc::clone(&messaging) as Arc<_>,
                    SkillRegistry::new(),
                    "provider",
                    &[("demo.echo", 1)],
                )
                .card()
                .clone(),
                "demo.echo",
                json!({}),
                "local-payment",
            )
            .await?;
    }
    let restored = logos_agent::a2a::A2aClient::with_state(
        Agent::from_parts(
            provider_id,
            SpendingPolicy {
                per_tx_limit: 10,
                per_period_limit: 0,
                period_seconds: 86_400,
            },
        ),
        messaging,
        path.clone(),
    )?;
    assert_eq!(restored.tracked_ids(), vec!["task-0"]);
    let _ = std::fs::remove_file(path);
    Ok(())
}

const DISCOVERY: &str = "/logos-agent/1/a2a/discovery/proto";

#[tokio::test]
async fn two_agents_discover_run_task_and_settle_payment() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition = new_account(&mut ctx, false).await?;
    // Client can spend up to 50 autonomously; provider starts with nothing.
    let client_agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 50,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;
    let provider_agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;

    // Fund the client with 100 tokens.
    wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(client_agent.account_id()),
            name: "A2A-COIN".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;

    let messaging = Arc::new(InMemoryMessaging::new());

    // Provider offers `demo.echo` for 10 LEZ and publishes its Agent Card.
    let mut provider_registry = SkillRegistry::new();
    provider_registry.register(Box::new(EchoSkill));
    let provider = A2aProvider::new(
        provider_agent,
        Arc::clone(&messaging) as Arc<_>,
        provider_registry,
        "greeter-agent",
        &[("demo.echo", 10)],
    );
    provider.publish_card(DISCOVERY).await?;

    // Client discovers the provider.
    let mut client = A2aClient::new(client_agent, Arc::clone(&messaging) as Arc<_>);
    let cards = client.discover(DISCOVERY).await?;
    assert_eq!(cards.len(), 1, "should discover exactly one provider");
    let card = &cards[0];
    assert_eq!(card.protocol_version, "0.2.5");
    assert_eq!(card.skills.len(), 1);
    assert_eq!(card.skills[0].id, "demo.echo");
    assert_eq!(card.skills[0].price_lez, "10");

    // Client pays and requests the task — autonomously, no owner involved.
    let task = client
        .run_task(
            ctx.wallet_mut(),
            card,
            "demo.echo",
            json!({ "text": "hello" }),
        )
        .await?;
    assert_eq!(task.state, TaskState::Submitted);
    // Provider serves the pending task through the lifecycle.
    let served = provider.serve_pending(None).await?;
    assert_eq!(served, 1, "provider should serve one task");

    // Client reads the result off the update stream.
    let done = client.poll_task(card, &task).await?;
    assert_eq!(done.state, TaskState::Completed, "task should complete");
    assert_eq!(
        done.result
            .as_ref()
            .and_then(|value| value["echo"].as_str()),
        Some("hello"),
        "task result should echo the input"
    );

    // The payment settled autonomously: client 90, provider 10.
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        client.agent().balance(ctx.wallet(), definition),
        90,
        "client should have paid the task price"
    );
    assert_eq!(
        provider.agent().balance(ctx.wallet(), definition),
        10,
        "provider should have received the task price"
    );

    Ok(())
}

/// A task cancelled before the provider serves it settles as `Canceled` and the
/// payment is refunded — a real reversing LEZ transfer on chain.
#[tokio::test]
async fn canceled_task_is_refunded() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let definition = new_account(&mut ctx, false).await?;
    let client_agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 50,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;
    // Provider can spend nothing autonomously — the refund must bypass the policy.
    let provider_agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;

    // Fund the client with 100 tokens.
    wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(client_agent.account_id()),
            name: "A2A-COIN".to_owned(),
            total_supply: 100,
        }),
    )
    .await?;

    let messaging = Arc::new(InMemoryMessaging::new());

    let mut provider_registry = SkillRegistry::new();
    provider_registry.register(Box::new(EchoSkill));
    let provider = A2aProvider::new(
        provider_agent,
        Arc::clone(&messaging) as Arc<_>,
        provider_registry,
        "greeter-agent",
        &[("demo.echo", 10)],
    );
    provider.publish_card(DISCOVERY).await?;

    let mut client = A2aClient::new(client_agent, Arc::clone(&messaging) as Arc<_>);
    let cards = client.discover(DISCOVERY).await?;
    let card = &cards[0];

    // Client pays and requests, then cancels before the provider serves it.
    let task = client
        .run_task(ctx.wallet_mut(), card, "demo.echo", json!({ "text": "hi" }))
        .await?;
    // The payment left the client and reached the provider.
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(client.agent().balance(ctx.wallet(), definition), 90);
    assert_eq!(provider.agent().balance(ctx.wallet(), definition), 10);

    client.cancel(card, &task).await?;

    // Provider serves: sees the cancel, refunds the payment, marks it canceled.
    let served = provider.serve_pending(Some(ctx.wallet_mut())).await?;
    assert_eq!(served, 1, "provider should process the cancelled task");
    let done = client.poll_task(card, &task).await?;
    assert_eq!(done.state, TaskState::Canceled, "task should be canceled");

    // The refund reversed the payment: client back to 100, provider back to 0.
    ctx.wallet_mut().sync_to_latest_block().await?;
    assert_eq!(
        client.agent().balance(ctx.wallet(), definition),
        100,
        "cancel should refund the client in full"
    );
    assert_eq!(
        provider.agent().balance(ctx.wallet(), definition),
        0,
        "provider should hold nothing after refunding"
    );

    Ok(())
}
