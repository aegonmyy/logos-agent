//! Verifies the complete default skill surface is registered and that the
//! reflective meta skills work — no chain needed (these paths don't touch the
//! ledger).

use std::sync::Arc;

use anyhow::{Result, anyhow};
use logos_agent::messaging::InMemoryMessaging;
use logos_agent::skills::{SkillContext, SkillRegistry};
use logos_agent::storage::InMemoryStorage;
use logos_agent::{Agent, SpendingPolicy};
use serde_json::json;

#[tokio::test]
async fn full_default_skill_catalogue_and_meta_configure() -> Result<()> {
    let account_id: lee::AccountId = "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
        .parse()
        .map_err(|_| anyhow!("invalid account id literal"))?;
    let agent = Agent::from_parts(account_id, SpendingPolicy { per_tx_limit: 5 });

    let mut registry = SkillRegistry::with_defaults();
    registry.register_storage(Arc::new(InMemoryStorage::new([0u8; 32])) as Arc<_>);
    registry.register_messaging(Arc::new(InMemoryMessaging::new()) as Arc<_>);

    // Every default skill across all categories is catalogued.
    {
        let mut ctx = SkillContext { wallet: None, agent: &agent };
        let catalogue = registry.dispatch("meta.skills", &mut ctx, json!({})).await?;
        let names: Vec<String> = catalogue
            .as_array()
            .expect("catalogue array")
            .iter()
            .map(|item| item["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        for expected in [
            "storage.upload", "storage.download", "storage.list", "storage.share",
            "messaging.send", "messaging.join", "messaging.create_group",
            "wallet.balance", "wallet.send", "wallet.history",
            "program.query", "program.call", "program.deploy",
            "meta.skills", "meta.status", "meta.configure",
        ] {
            assert!(names.contains(&expected.to_owned()), "missing skill {expected}");
        }
    }

    // wallet.history starts empty.
    {
        let mut ctx = SkillContext { wallet: None, agent: &agent };
        let history = registry.dispatch("wallet.history", &mut ctx, json!({})).await?;
        assert_eq!(history["transactions"].as_array().map(Vec::len), Some(0));
    }

    // meta.configure raises the spending limit at runtime.
    {
        let mut ctx = SkillContext { wallet: None, agent: &agent };
        let result = registry
            .dispatch("meta.configure", &mut ctx, json!({ "key": "per_tx_limit", "value": 99 }))
            .await?;
        assert_eq!(result["status"], "configured");
    }
    assert_eq!(agent.policy_limit(), 99, "meta.configure should change the live limit");

    Ok(())
}
