//! Stage-2 proof for the Storage and Messaging skill categories. These skills
//! don't touch the ledger, so they run against in-memory backends — fast and
//! deterministic — exercising the same skill interface and encryption path the
//! real Codex / nwaku backends use.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use logos_agent::messaging::{InMemoryMessaging, Messaging};
use logos_agent::skills::{SkillContext, SkillRegistry};
use logos_agent::storage::InMemoryStorage;
use logos_agent::{Agent, SpendingPolicy};
use serde_json::json;

#[tokio::test]
async fn storage_and_messaging_skills_round_trip() -> Result<()> {
    let account_id: lee::AccountId = "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
        .parse()
        .map_err(|_| anyhow!("invalid account id literal"))?;
    let agent = Agent::from_parts(account_id, SpendingPolicy { per_tx_limit: 0 });

    let storage = Arc::new(InMemoryStorage::new([7u8; 32]));
    let messaging = Arc::new(InMemoryMessaging::new());

    let mut registry = SkillRegistry::new();
    registry.register_storage(Arc::clone(&storage) as Arc<_>);
    registry.register_messaging(Arc::clone(&messaging) as Arc<_>);

    // storage.upload — encrypt + store, get a content address.
    let address = {
        let mut ctx = SkillContext {
            wallet: None,
            agent: &agent,
        };
        let result = registry
            .dispatch(
                "storage.upload",
                &mut ctx,
                json!({ "label": "notes", "data": "top secret" }),
            )
            .await?;
        result["address"]
            .as_str()
            .ok_or_else(|| anyhow!("no address returned"))?
            .to_owned()
    };
    assert!(!address.is_empty());

    // storage.list — the object shows up.
    {
        let mut ctx = SkillContext {
            wallet: None,
            agent: &agent,
        };
        let result = registry.dispatch("storage.list", &mut ctx, json!({})).await?;
        let objects = result["objects"].as_array().expect("objects array");
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0]["label"], "notes");
    }

    // storage.download — round-trips the plaintext, proving encrypt/decrypt.
    {
        let mut ctx = SkillContext {
            wallet: None,
            agent: &agent,
        };
        let result = registry
            .dispatch("storage.download", &mut ctx, json!({ "address": address }))
            .await?;
        assert_eq!(result["data"], "top secret");
    }

    // storage.share — grants access.
    {
        let mut ctx = SkillContext {
            wallet: None,
            agent: &agent,
        };
        let result = registry
            .dispatch(
                "storage.share",
                &mut ctx,
                json!({ "address": address, "recipient": "bob" }),
            )
            .await?;
        assert_eq!(result["status"], "shared");
    }

    // messaging.create_group — deterministic group topic.
    let group_id = {
        let mut ctx = SkillContext {
            wallet: None,
            agent: &agent,
        };
        let result = registry
            .dispatch(
                "messaging.create_group",
                &mut ctx,
                json!({ "members": ["alice", "bob"] }),
            )
            .await?;
        result["group_id"]
            .as_str()
            .ok_or_else(|| anyhow!("no group id"))?
            .to_owned()
    };
    assert!(group_id.starts_with("/logos-agent/1/group-"));

    // messaging.join — joins it.
    {
        let mut ctx = SkillContext {
            wallet: None,
            agent: &agent,
        };
        let result = registry
            .dispatch("messaging.join", &mut ctx, json!({ "group_id": group_id }))
            .await?;
        assert_eq!(result["status"], "joined");
    }

    // messaging.send — delivered to the recipient's inbox.
    {
        let mut ctx = SkillContext {
            wallet: None,
            agent: &agent,
        };
        let result = registry
            .dispatch(
                "messaging.send",
                &mut ctx,
                json!({ "to": "alice", "message": "hello alice" }),
            )
            .await?;
        assert!(!result["message_id"].as_str().unwrap_or_default().is_empty());
    }
    let delivered = messaging.poll("alice").await?;
    assert_eq!(delivered, vec![b"hello alice".to_vec()]);

    // meta.skills — every default category is now catalogued.
    {
        let mut ctx = SkillContext {
            wallet: None,
            agent: &agent,
        };
        let catalogue = registry.dispatch("meta.skills", &mut ctx, json!({})).await?;
        let names: Vec<String> = catalogue
            .as_array()
            .expect("catalogue array")
            .iter()
            .map(|item| item["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        for expected in [
            "storage.upload",
            "storage.download",
            "storage.list",
            "storage.share",
            "messaging.send",
            "messaging.join",
            "messaging.create_group",
        ] {
            assert!(names.contains(&expected.to_owned()), "missing skill {expected}");
        }
    }

    Ok(())
}
