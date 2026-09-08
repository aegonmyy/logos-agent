//! LP-0008 agent-to-agent criterion over REAL Waku: two agents discover each
//! other from Agent Cards, run a task through the A2A lifecycle, and settle
//! the LEZ payment autonomously, with the coordination traffic carried by a
//! live nwaku node (the Logos Dev Network, cluster 2) instead of the in-memory
//! backend. No owner is involved anywhere in the flow.
//!
//! Ignored by default because it needs a running nwaku node + the local
//! sequencer. Bring up nwaku and run:
//!   docker run -d --name nwaku -p 8645:8645 wakuorg/nwaku:v0.38.0 \
//!     --rest=true --rest-address=0.0.0.0 --rest-port=8645 --relay=true --cluster-id=2
//!   RISC0_DEV_MODE=0 cargo test --test a2a_two_agents_waku -- --ignored --nocapture
//!
//! This is the Waku-transport companion to `tests/a2a_two_agents.rs` (which
//! uses the in-memory backend and runs in CI). The flow is identical; only the
//! messaging transport differs.

use std::sync::Arc;

use logos_agent::a2a::{A2aClient, A2aProvider, TaskState};
use logos_agent::messaging::WakuMessaging;
use logos_agent::skills::{EchoSkill, SkillRegistry};
use logos_agent::{Agent, SpendingPolicy};
use test_fixtures::{TestContext, private_mention, public_mention};
use wallet::cli::{
    Command, SubcommandReturnValue,
    programs::token::TokenProgramAgnosticSubcommand,
};

/// The discovery topic the in-memory companion test uses. A live node buffers
/// whatever was published there, so the test does not assume it is empty: the
/// provider is named uniquely per run and the client selects that card.
const DISCOVERY: &str = "/logos-agent/1/a2a-discovery/proto";

fn messaging_url() -> String {
    std::env::var("AGENT_MESSAGING_URL").unwrap_or_else(|_| "http://127.0.0.1:8645".into())
}

#[test]
#[ignore = "requires a running nwaku node (cluster-id=2) and the local sequencer"]
fn two_agents_discover_task_and_pay_over_waku() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let mut ctx = rt.block_on(TestContext::new())?;

    let definition = rt.block_on(wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(wallet::cli::account::AccountSubcommand::New(
            wallet::cli::account::NewSubcommand::Public {
                cci: None,
                label: None,
            },
        )),
    ))?;
    let SubcommandReturnValue::RegisterAccount { account_id: definition } = definition else {
        anyhow::bail!("expected a registered account id");
    };

    // Client can spend up to 50 autonomously; provider starts with nothing.
    // No OwnerChannel is created on either side: the criterion is the two
    // agents coordinating and paying without owner intervention.
    let client_agent = rt.block_on(Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 50,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    ))?;
    let provider_agent = rt.block_on(Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    ))?;

    // Fund the client with 100 tokens.
    rt.block_on(wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Token(TokenProgramAgnosticSubcommand::New {
            definition_account_id: public_mention(definition),
            supply_account_id: private_mention(client_agent.account_id()),
            name: "A2A-WAKU-COIN".to_owned(),
            total_supply: 100,
        }),
    ))?;

    let messaging = Arc::new(WakuMessaging::new(messaging_url()));

    // Provider offers `demo.echo` for 10 LEZ and publishes its Agent Card.
    // The name is unique per run so a live node's buffered discovery topic
    // (which may hold cards from earlier runs) cannot be mistaken for this one.
    let provider_name = format!("greeter-agent-waku-{}", std::process::id());
    let mut provider_registry = SkillRegistry::new();
    provider_registry.register(Box::new(EchoSkill));
    let provider = A2aProvider::new(
        provider_agent,
        Arc::clone(&messaging) as Arc<_>,
        provider_registry,
        provider_name.clone(),
        &[("demo.echo", 10)],
    );
    rt.block_on(provider.publish_card(DISCOVERY))?;

    // Client discovers the provider through the live node, retrying while the
    // publish round-trips.
    let mut client = A2aClient::new(client_agent, Arc::clone(&messaging) as Arc<_>);
    let mut card = None;
    for attempt in 1..=10 {
        let cards = rt.block_on(client.discover(DISCOVERY))?;
        card = cards
            .into_iter()
            .find(|card| card.name == provider_name && card.verify());
        if card.is_some() {
            break;
        }
        eprintln!("[waku-a2a] discovery attempt {attempt}: card not visible yet");
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let card = card.expect("the provider's signed card should be discoverable over waku");

    // Client pays and requests the task, all under its own spending policy.
    let task = rt.block_on(client.run_task(
        ctx.wallet_mut(),
        &card,
        "demo.echo",
        serde_json::json!({ "text": "hello-waku" }),
    ))?;
    assert_eq!(task.state, TaskState::Submitted);

    // Provider serves the pending task once the request arrives.
    let mut served = 0;
    for attempt in 1..=10 {
        served = rt.block_on(provider.serve_pending(None))?;
        if served > 0 {
            break;
        }
        eprintln!("[waku-a2a] serve attempt {attempt}: inbox empty");
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    assert!(served > 0, "the provider should serve the task over waku");

    // Client reads the result off the update stream.
    let mut done = task.clone();
    for attempt in 1..=10 {
        done = rt.block_on(client.poll_task(&card, &task))?;
        if done.state == TaskState::Completed || done.state == TaskState::Failed {
            break;
        }
        eprintln!("[waku-a2a] poll attempt {attempt}: {:?}", done.state);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    assert_eq!(done.state, TaskState::Completed, "task should complete");
    assert_eq!(
        done.result
            .as_ref()
            .and_then(|value| value["echo"].as_str()),
        Some("hello-waku"),
        "task result should echo the input"
    );

    // The payment settled autonomously: client 90, provider 10.
    rt.block_on(ctx.wallet_mut().sync_to_latest_block())?;
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
    eprintln!("[waku-a2a] discovery, task lifecycle, and payment all over live waku");

    // Teardown: TestContext::Drop needs an active reactor (testcontainers).
    rt.block_on(async move {
        drop(ctx);
    });
    Ok(())
}
