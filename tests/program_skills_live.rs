//! Live proof of the generic LEZ program skills against a standalone sequencer.
//!
//! `program.deploy`, `program.call`, and `program.query` are exercised end to
//! end: the agent deploys the sample `claimer` program, calls it to claim a
//! public account, and reads the account back to confirm the program now owns
//! it. This mirrors the LEZ's own `program_deployment` integration test, but
//! driven entirely through the agent's skill dispatch.

use std::time::Duration;

use anyhow::{Result, bail};
use logos_agent::skills::{SkillContext, SkillRegistry};
use logos_agent::{Agent, SpendingPolicy};
use serde_json::json;
use test_fixtures::{TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext};
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
};

async fn wait_for_block() {
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;
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

#[tokio::test]
async fn program_deploy_call_and_query_on_a_sequencer() -> Result<()> {
    let mut ctx = TestContext::new().await?;
    let agent = Agent::create(
        ctx.wallet_mut(),
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
    .await?;
    let registry = SkillRegistry::with_defaults();

    // The sample program we will deploy and call.
    let claimer = test_programs::claimer();
    let expected_id: Vec<u32> = claimer.id().to_vec();

    // Write the compiled ELF somewhere program.deploy can read it.
    let elf_path = std::env::temp_dir().join(format!("claimer-{}.elf", std::process::id()));
    std::fs::write(&elf_path, claimer.elf())?;

    // 1) program.deploy — deploy the ELF; the skill returns its program id.
    let deployed = {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        registry
            .dispatch(
                "program.deploy",
                &mut sctx,
                json!({ "binary_path": elf_path.to_string_lossy() }),
            )
            .await?
    };
    let program_id_hex = deployed["program_id"]
        .as_str()
        .expect("deploy should return a program_id")
        .to_owned();
    assert_eq!(
        program_id_hex.len(),
        64,
        "program id should be 64 hex chars"
    );
    wait_for_block().await;

    // A fresh public account for the program to claim.
    let account = new_public_account(&mut ctx).await?;
    wait_for_block().await;

    // 2) program.call — invoke the claimer against that account (empty instruction).
    let called = {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        registry
            .dispatch(
                "program.call",
                &mut sctx,
                json!({
                    "program_id": program_id_hex,
                    "accounts": [account.to_string()],
                    "instruction": [],
                }),
            )
            .await?
    };
    assert!(
        called.get("tx_hash").and_then(|v| v.as_str()).is_some(),
        "call should return a tx hash"
    );
    // A program-deploying transaction is large; give it extra time to land.
    tokio::time::sleep(Duration::from_secs(2 * TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;
    ctx.wallet_mut().sync_to_latest_block().await?;

    // 3) program.query — read the account; the claimer now owns it.
    let queried = {
        let mut sctx = SkillContext {
            wallet: Some(ctx.wallet_mut()),
            agent: &agent,
        };
        registry
            .dispatch(
                "program.query",
                &mut sctx,
                json!({ "account": account.to_string() }),
            )
            .await?
    };
    let owner: Vec<u32> = queried["state"]["program_owner"]
        .as_array()
        .expect("account state should carry a program_owner")
        .iter()
        .map(|w| w.as_u64().expect("owner word is u32") as u32)
        .collect();
    assert_eq!(
        owner, expected_id,
        "the queried account should now be owned by the deployed program"
    );

    let _ = std::fs::remove_file(&elf_path);
    Ok(())
}
