//! LP-0008 skill-interface criterion: a skill defined entirely OUTSIDE the
//! agent core (in this test crate, compiled against the public API only),
//! registered into a running registry, dispatched by name, listed by
//! `meta.skills`, and shadowing a default skill — all without any change to
//! the library. This is the executable proof for "add new skills without
//! modifying the core agent module" (see docs/SKILL_INTERFACE.md).

use async_trait::async_trait;
use logos_agent::skills::{ParamSpec, Skill, SkillContext, SkillRegistry};
use logos_agent::{Agent, SpendingPolicy};
use serde_json::{Value, json};

/// A third-party skill: transforms text. It exists only in this file; the
/// agent crate knows nothing about it.
struct ShoutSkill;

#[async_trait(?Send)]
impl Skill for ShoutSkill {
    fn name(&self) -> &'static str {
        "demo.shout"
    }
    fn description(&self) -> &'static str {
        "Return the text in upper case."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::required("text", "Text to shout back.")]
    }
    async fn invoke(
        &self,
        _ctx: &mut SkillContext<'_>,
        args: Value,
    ) -> anyhow::Result<Value> {
        let text = args
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_uppercase();
        Ok(json!({ "shout": text }))
    }
}

/// A third-party override of a default skill: same name as a registered
/// default, different behavior. Registration order decides which runs.
struct QuietListSkill;

#[async_trait(?Send)]
impl Skill for QuietListSkill {
    fn name(&self) -> &'static str {
        "storage.list"
    }
    fn description(&self) -> &'static str {
        "Third-party override of storage.list."
    }
    async fn invoke(
        &self,
        _ctx: &mut SkillContext<'_>,
        _args: Value,
    ) -> anyhow::Result<Value> {
        Ok(json!({ "objects": [], "source": "third-party override" }))
    }
}

fn agent() -> Agent {
    let account_id: lee::AccountId = "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
        .parse()
        .expect("valid account id literal");
    Agent::from_parts(
        account_id,
        SpendingPolicy {
            per_tx_limit: 0,
            per_period_limit: 0,
            period_seconds: 86_400,
        },
    )
}

#[tokio::test]
async fn third_party_skill_registers_dispatches_and_lists() -> anyhow::Result<()> {
    let agent = agent();
    let mut registry = SkillRegistry::new();
    registry.register(Box::new(ShoutSkill));

    let mut ctx = SkillContext {
        wallet: None,
        agent: &agent,
    };

    // Dispatch by name through the ordinary path.
    let result = registry
        .dispatch("demo.shout", &mut ctx, json!({ "text": "hello" }))
        .await?;
    assert_eq!(result["shout"], "HELLO");

    // The custom skill is first-class in the reflective catalogue, with its
    // parameter spec intact.
    let catalogue = registry.dispatch("meta.skills", &mut ctx, json!({})).await?;
    let entry = catalogue
        .as_array()
        .expect("meta.skills returns an array")
        .iter()
        .find(|entry| entry["name"] == "demo.shout")
        .expect("the third-party skill should be listed by meta.skills");
    assert_eq!(entry["description"], "Return the text in upper case.");
    assert_eq!(entry["params"][0]["name"], "text");
    assert_eq!(entry["params"][0]["required"], true);

    // A missing required argument is an ordinary dispatch error.
    assert!(
        registry.dispatch("demo.shout", &mut ctx, json!({})).await.is_err(),
        "missing required param should fail"
    );
    Ok(())
}

#[tokio::test]
async fn third_party_skill_shadows_a_default_without_touching_core() -> anyhow::Result<()> {
    use std::sync::Arc;
    use logos_agent::storage::InMemoryStorage;

    let agent = agent();
    let mut registry = SkillRegistry::new();
    registry.register_storage(Arc::new(InMemoryStorage::new([0u8; 32])) as Arc<_>);

    let mut ctx = SkillContext {
        wallet: None,
        agent: &agent,
    };
    let before = registry
        .dispatch("storage.list", &mut ctx, json!({}))
        .await?;
    assert!(
        before.get("source").is_none(),
        "the default storage.list should not carry a source marker"
    );

    // Re-registering the name after the default installs the third-party
    // version: last registration wins, no core change involved.
    registry.register(Box::new(QuietListSkill));
    let after = registry
        .dispatch("storage.list", &mut ctx, json!({}))
        .await?;
    assert_eq!(after["source"], "third-party override");
    Ok(())
}
