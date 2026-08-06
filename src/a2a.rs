//! Agent-to-agent coordination (LP-0008, stage 4).
//!
//! This is an [A2A](https://a2a-protocol.org)-compatible coordination layer with
//! two Logos-native substitutions: the transport is Logos Messaging (Waku
//! topics) rather than HTTP, and payment is a LEZ token transfer rather than an
//! out-of-band arrangement A2A leaves open. Agents publish an **Agent Card**
//! describing their skills and per-task price, discover each other on a shared
//! topic, and run tasks through the A2A **task lifecycle**
//! (`submitted → working → completed/failed`), paying autonomously on request.
//!
//! [`A2aProvider`] advertises skills and serves tasks from its
//! [`SkillRegistry`](crate::skills::SkillRegistry); [`A2aClient`] discovers
//! providers, pays, and awaits results.

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, bail};
use lee::AccountId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wallet::WalletCore;

use crate::Agent;
use crate::messaging::Messaging;
use crate::skills::{SkillContext, SkillRegistry};

/// A2A task lifecycle state. Serialized with the A2A wire names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
}

/// One skill a provider advertises, with its LEZ price per task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Price per task in LEZ token units (string to stay exact for u128).
    #[serde(rename = "priceLez")]
    pub price_lez: String,
}

/// Declared A2A capabilities of an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub streaming: bool,
}

/// An A2A-compatible Agent Card, extended with the LEZ payment account and a
/// Logos Messaging address in place of A2A's HTTP `url`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub name: String,
    pub description: String,
    pub version: String,
    /// Logos Messaging topic this agent receives task requests on (A2A transport).
    pub address: String,
    /// The agent's LEZ account, where task payments are sent.
    #[serde(rename = "lezAccount")]
    pub lez_account: String,
    pub capabilities: Capabilities,
    pub skills: Vec<CardSkill>,
}

impl AgentCard {
    /// The advertised price for `skill_id`, if this agent offers it.
    fn price_of(&self, skill_id: &str) -> Option<u128> {
        self.skills
            .iter()
            .find(|skill| skill.id == skill_id)
            .and_then(|skill| skill.price_lez.parse().ok())
    }
}

/// A task as tracked by the client.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub state: TaskState,
    pub result: Option<Value>,
}

fn inbox_topic(agent: &AccountId) -> String {
    format!("/logos-agent/1/a2a/{agent}/inbox/proto")
}

fn updates_topic(agent: &AccountId) -> String {
    format!("/logos-agent/1/a2a/{agent}/updates/proto")
}

/// A provider agent: advertises skills, serves task requests from its registry,
/// and gets paid in LEZ per task.
pub struct A2aProvider {
    agent: Agent,
    messaging: Arc<dyn Messaging>,
    registry: SkillRegistry,
    card: AgentCard,
}

impl A2aProvider {
    /// Build a provider that advertises `advertised` skills (by id, with a LEZ
    /// price) served from `registry`.
    pub fn new(
        agent: Agent,
        messaging: Arc<dyn Messaging>,
        registry: SkillRegistry,
        name: impl Into<String>,
        advertised: &[(&str, u128)],
    ) -> Self {
        let catalogue = registry.catalogue();
        let known: Vec<Value> = catalogue.as_array().cloned().unwrap_or_default();
        let describe = |id: &str| -> String {
            known
                .iter()
                .find(|entry| entry["name"] == id)
                .and_then(|entry| entry["description"].as_str())
                .unwrap_or("")
                .to_owned()
        };

        let skills = advertised
            .iter()
            .map(|(id, price)| CardSkill {
                id: (*id).to_owned(),
                name: (*id).to_owned(),
                description: describe(id),
                price_lez: price.to_string(),
            })
            .collect();

        let card = AgentCard {
            protocol_version: "0.2.5".to_owned(),
            name: name.into(),
            description: "Logos-native A2A agent".to_owned(),
            version: "1.0.0".to_owned(),
            address: inbox_topic(&agent.account_id()),
            lez_account: agent.account_id().to_string(),
            capabilities: Capabilities { streaming: true },
            skills,
        };

        Self {
            agent,
            messaging,
            registry,
            card,
        }
    }

    /// This agent's Agent Card (the `agent.card()` skill).
    #[must_use]
    pub const fn card(&self) -> &AgentCard {
        &self.card
    }

    /// The provider's own agent (payee).
    #[must_use]
    pub const fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Publish the Agent Card to a shared discovery topic (`agent.card` publish).
    pub async fn publish_card(&self, discovery_topic: &str) -> Result<()> {
        let bytes = serde_json::to_vec(&self.card).context("encoding agent card")?;
        self.messaging.send(discovery_topic, &bytes).await?;
        Ok(())
    }

    /// Serve every pending task request in the inbox: transition through the A2A
    /// lifecycle and publish status updates carrying the skill result.
    pub async fn serve_pending(&self) -> Result<usize> {
        let inbox = inbox_topic(&self.agent.account_id());
        let updates = updates_topic(&self.agent.account_id());
        let requests = self.messaging.poll(&inbox).await?;
        let mut served = 0;

        for raw in requests {
            let request: Value =
                serde_json::from_slice(&raw).context("decoding task request")?;
            if request.get("kind").and_then(Value::as_str) != Some("task_request") {
                continue;
            }
            let task_id = request
                .get("taskId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let skill = request
                .get("skill")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let params = request.get("params").cloned().unwrap_or(json!({}));

            self.publish_update(&updates, &task_id, &TaskState::Working, None)
                .await?;

            let mut ctx = SkillContext {
                wallet: None,
                agent: &self.agent,
            };
            match self.registry.dispatch(&skill, &mut ctx, params).await {
                Ok(result) => {
                    self.publish_update(&updates, &task_id, &TaskState::Completed, Some(result))
                        .await?;
                }
                Err(error) => {
                    self.publish_update(
                        &updates,
                        &task_id,
                        &TaskState::Failed,
                        Some(json!({ "error": error.to_string() })),
                    )
                    .await?;
                }
            }
            served += 1;
        }

        Ok(served)
    }

    async fn publish_update(
        &self,
        updates_topic: &str,
        task_id: &str,
        state: &TaskState,
        result: Option<Value>,
    ) -> Result<()> {
        let bytes = serde_json::to_vec(&json!({
            "kind": "task_update",
            "taskId": task_id,
            "state": state,
            "result": result,
        }))
        .context("encoding task update")?;
        self.messaging.send(updates_topic, &bytes).await?;
        Ok(())
    }
}

/// A client agent: discovers providers, pays for a task, and awaits its result.
pub struct A2aClient {
    agent: Agent,
    messaging: Arc<dyn Messaging>,
    next_task: u64,
}

impl A2aClient {
    #[must_use]
    pub fn new(agent: Agent, messaging: Arc<dyn Messaging>) -> Self {
        Self {
            agent,
            messaging,
            next_task: 0,
        }
    }

    /// The client agent's own account (payer).
    #[must_use]
    pub const fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Discover Agent Cards published on `discovery_topic` (`agent.discover`).
    pub async fn discover(&self, discovery_topic: &str) -> Result<Vec<AgentCard>> {
        let raw = self.messaging.poll(discovery_topic).await?;
        raw.iter()
            .map(|bytes| serde_json::from_slice(bytes).context("decoding agent card"))
            .collect()
    }

    /// Pay for and request `skill` from `provider`, then wait for the result
    /// (`agent.task`). Payment is a real LEZ transfer, made autonomously within
    /// the client's own spending policy.
    pub async fn run_task(
        &mut self,
        wallet: &mut WalletCore,
        provider: &AgentCard,
        skill: &str,
        params: Value,
    ) -> Result<Task> {
        let price = provider
            .price_of(skill)
            .with_context(|| format!("provider does not offer skill {skill}"))?;
        let payee: AccountId = provider
            .lez_account
            .parse()
            .map_err(|_| anyhow!("provider card has an invalid LEZ account"))?;

        // Pay the declared price, subject to our own spending policy.
        match self.agent.send(wallet, payee, price).await? {
            crate::SpendOutcome::Executed { .. } => {}
            crate::SpendOutcome::NeedsOwnerApproval { limit, .. } => {
                bail!("task price {price} exceeds autonomous limit {limit}; owner approval needed");
            }
        }

        // Send the A2A task request over Logos Messaging.
        let task_id = format!("task-{}", self.next_task);
        self.next_task += 1;
        let request = serde_json::to_vec(&json!({
            "kind": "task_request",
            "taskId": task_id,
            "skill": skill,
            "params": params,
        }))
        .context("encoding task request")?;
        self.messaging.send(&provider.address, &request).await?;

        Ok(Task {
            id: task_id,
            state: TaskState::Submitted,
            result: None,
        })
    }

    /// Read the latest status for `task` from `provider`'s update stream
    /// (`agent.subscribe`). Returns the task advanced to its newest known state.
    pub async fn poll_task(&self, provider: &AgentCard, task: &Task) -> Result<Task> {
        let payee: AccountId = provider
            .lez_account
            .parse()
            .map_err(|_| anyhow!("provider card has an invalid LEZ account"))?;
        let updates = self.messaging.poll(&updates_topic(&payee)).await?;

        let mut latest = task.clone();
        for raw in updates {
            let update: Value = serde_json::from_slice(&raw).context("decoding task update")?;
            if update.get("taskId").and_then(Value::as_str) != Some(task.id.as_str()) {
                continue;
            }
            if let Ok(state) =
                serde_json::from_value::<TaskState>(update["state"].clone())
            {
                latest.state = state;
                latest.result = update.get("result").cloned().filter(|value| !value.is_null());
            }
        }
        Ok(latest)
    }

    /// Request cancellation of a running task (`agent.cancel`).
    pub async fn cancel(&self, provider: &AgentCard, task: &Task) -> Result<()> {
        let bytes = serde_json::to_vec(&json!({
            "kind": "task_cancel",
            "taskId": task.id,
        }))
        .context("encoding cancel")?;
        self.messaging.send(&provider.address, &bytes).await?;
        Ok(())
    }
}
