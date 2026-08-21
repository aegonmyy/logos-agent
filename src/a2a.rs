//! Agent-to-agent coordination (LP-0008, stage 4).
//!
//! This is an [A2A](https://a2a-protocol.org)-compatible coordination layer with
//! two Logos-native substitutions: the transport is Logos Messaging (Waku
//! topics) rather than HTTP, and payment is a LEZ token transfer rather than an
//! out-of-band arrangement A2A leaves open. Agents publish a **signed Agent
//! Card** describing their skills and per-task price, discover each other on a
//! shared topic, and run tasks through the A2A **task lifecycle**
//! (`submitted → working → completed/failed`), paying autonomously on request.
//! Each card is Ed25519-signed by its publisher and embeds the verifying key,
//! so a tampered or mis-attributed card fails [`AgentCard::verify`].
//!
//! [`A2aProvider`] advertises skills and serves tasks from its
//! [`SkillRegistry`](crate::skills::SkillRegistry); [`A2aClient`] discovers
//! providers, pays, and awaits results.

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use ed25519_dalek::{Signer as _, Verifier as _};
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
///
/// The card is a **signed** document (LP-0008): `signing_pubkey` carries the
/// agent's Ed25519 verifying key and `signature` is an Ed25519 signature over
/// the card's canonical JSON (the same document with `signature` absent). The
/// key is self-certifying — it is generated per agent and embedded in the very
/// document it signs — because a shielded LEZ account's nullifier/viewing keys
/// produce ZK proofs, not message signatures, and the wallet's `sign_message`
/// only covers public accounts and keycards. A verifier that obtains the card
/// over an authenticated channel once (e.g. the owner channel) can then trust
/// every later card carrying the same `signing_pubkey`.
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
    /// Hex-encoded Ed25519 verifying key certifying this card.
    #[serde(rename = "signingPubkey", default)]
    pub signing_pubkey: String,
    /// Hex-encoded Ed25519 signature over this card's canonical JSON (with the
    /// `signature` field absent). `None` on legacy unsigned cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl AgentCard {
    /// The advertised price for `skill_id`, if this agent offers it.
    fn price_of(&self, skill_id: &str) -> Option<u128> {
        self.skills
            .iter()
            .find(|skill| skill.id == skill_id)
            .and_then(|skill| skill.price_lez.parse().ok())
    }

    /// The canonical byte string a card signature commits to: the card's JSON
    /// with the `signature` field absent. Field order is the struct's, so
    /// serialization is deterministic.
    fn canonical_bytes(&self) -> Result<Vec<u8>> {
        let unsigned = Self {
            signature: None,
            ..self.clone()
        };
        serde_json::to_vec(&unsigned).context("encoding agent card for signing")
    }

    /// Sign this card with `signing_key`, filling in `signing_pubkey` and
    /// `signature`.
    fn sign_with(&mut self, signing_key: &ed25519_dalek::SigningKey) -> Result<()> {
        self.signing_pubkey = hex::encode(signing_key.verifying_key().as_bytes());
        let bytes = self.canonical_bytes()?;
        let signature = signing_key.sign(&bytes);
        self.signature = Some(hex::encode(signature.to_bytes()));
        Ok(())
    }

    /// Whether the card's signature is present and valid over its canonical
    /// JSON under its embedded verifying key. Unsigned or tampered cards
    /// return `false`.
    #[must_use]
    pub fn verify(&self) -> bool {
        let Some(signature_hex) = &self.signature else {
            return false;
        };
        let Ok(pubkey_bytes) = hex::decode(&self.signing_pubkey) else {
            return false;
        };
        let Ok(signature_bytes) = hex::decode(signature_hex) else {
            return false;
        };
        let Ok(pubkey): Result<[u8; 32], _> = pubkey_bytes.try_into() else {
            return false;
        };
        let Ok(signature): Result<[u8; 64], _> = signature_bytes.try_into() else {
            return false;
        };
        let Ok(verifying_key) = ed25519_dalek::VerifyingKey::from_bytes(&pubkey) else {
            return false;
        };
        let Ok(canonical) = self.canonical_bytes() else {
            return false;
        };
        verifying_key
            .verify(&canonical, &ed25519_dalek::Signature::from_bytes(&signature))
            .is_ok()
    }
}

/// A task as tracked by the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// The card's signing key, kept so deployments can persist it and keep a
    /// stable `signing_pubkey` across restarts.
    signing_key: ed25519_dalek::SigningKey,
}

impl A2aProvider {
    /// Build a provider that advertises `advertised` skills (by id, with a LEZ
    /// price) served from `registry`. The Agent Card is signed with a freshly
    /// generated Ed25519 key; use [`A2aProvider::new_with_signing_key`] when the
    /// key must stay stable across restarts.
    pub fn new(
        agent: Agent,
        messaging: Arc<dyn Messaging>,
        registry: SkillRegistry,
        name: impl Into<String>,
        advertised: &[(&str, u128)],
    ) -> Self {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        Self::new_with_signing_key(agent, messaging, registry, name, advertised, signing_key)
    }

    /// [`A2aProvider::new`], but the card is signed with `signing_key`. Persist
    /// the key's bytes (`SigningKey::to_bytes`) alongside agent state and pass
    /// the restored key here so repeat deployments verify under the same
    /// `signing_pubkey`.
    pub fn new_with_signing_key(
        agent: Agent,
        messaging: Arc<dyn Messaging>,
        registry: SkillRegistry,
        name: impl Into<String>,
        advertised: &[(&str, u128)],
        signing_key: ed25519_dalek::SigningKey,
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

        let mut card = AgentCard {
            protocol_version: "0.2.5".to_owned(),
            name: name.into(),
            description: "Logos-native A2A agent".to_owned(),
            version: "1.0.0".to_owned(),
            address: inbox_topic(&agent.account_id()),
            lez_account: agent.account_id().to_string(),
            // A2A's `streaming` capability means SSE streaming of a task's
            // response. This transport delivers task updates over Logos
            // Messaging topics (polled via `agent.subscribe`), not SSE, so the
            // card advertises `streaming: false` rather than claiming a
            // streaming transport it does not provide.
            capabilities: Capabilities { streaming: false },
            skills,
            signing_pubkey: String::new(),
            signature: None,
        };
        card.sign_with(&signing_key)
            .expect("signing an agent card cannot fail");

        Self {
            agent,
            messaging,
            registry,
            card,
            signing_key,
        }
    }

    /// This agent's Agent Card (the `agent.card()` skill). The card is signed;
    /// see [`AgentCard::verify`].
    #[must_use]
    pub const fn card(&self) -> &AgentCard {
        &self.card
    }

    /// The card's Ed25519 signing key. Persist its bytes so a redeployed agent
    /// re-signs under the same [`AgentCard::signing_pubkey`].
    #[must_use]
    pub const fn signing_key(&self) -> &ed25519_dalek::SigningKey {
        &self.signing_key
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

    /// Serve every pending item in the inbox: transition task requests through
    /// the A2A lifecycle, and honour cancellations. A cancel for a task we have
    /// not yet served ends it in `Canceled` and — if a wallet is supplied and the
    /// request named its payer and price — refunds the payment.
    ///
    /// Pass `Some(wallet)` to enable refund-on-cancel; `None` serves requests but
    /// can only acknowledge a cancel (no refund).
    pub async fn serve_pending(&self, mut wallet: Option<&mut WalletCore>) -> Result<usize> {
        let inbox = inbox_topic(&self.agent.account_id());
        let updates = updates_topic(&self.agent.account_id());
        let messages = self.messaging.poll(&inbox).await?;

        // Split the inbox into task requests and the ids cancelled this round.
        let mut requests = Vec::new();
        let mut cancelled = std::collections::HashSet::new();
        for raw in messages {
            let msg: Value = serde_json::from_slice(&raw).context("decoding a2a message")?;
            match msg.get("kind").and_then(Value::as_str) {
                Some("task_request") => requests.push(msg),
                Some("task_cancel") => {
                    if let Some(id) = msg.get("taskId").and_then(Value::as_str) {
                        cancelled.insert(id.to_owned());
                    }
                }
                _ => {}
            }
        }

        let mut served = 0;
        for request in requests {
            let task_id = request
                .get("taskId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();

            if cancelled.contains(&task_id) {
                self.refund_task(wallet.as_deref_mut(), &request).await?;
                self.publish_update(&updates, &task_id, &TaskState::Canceled, None)
                    .await?;
                served += 1;
                continue;
            }

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

    /// Refund a cancelled task's payment to the payer named in its request.
    /// A no-op when no wallet is supplied or the request omitted `from`/`priceLez`
    /// (e.g. an unpaid task). The refund bypasses the spending policy — returning
    /// money the client already paid is inherently authorised.
    async fn refund_task(&self, wallet: Option<&mut WalletCore>, request: &Value) -> Result<()> {
        let Some(wallet) = wallet else { return Ok(()) };
        let Some(from) = request.get("from").and_then(Value::as_str) else {
            return Ok(());
        };
        let Some(price) = request
            .get("priceLez")
            .and_then(Value::as_str)
            .and_then(|p| p.parse::<u128>().ok())
        else {
            return Ok(());
        };
        if price == 0 {
            return Ok(());
        }
        let payer: AccountId = from
            .parse()
            .map_err(|_| anyhow!("cancel refund: request has an invalid payer account"))?;
        self.agent.send_approved(wallet, payer, price).await?;
        Ok(())
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

/// The durable part of a client: its task ledger and the next task counter.
/// Persisted so a restarted client keeps tracking in-flight tasks and never
/// reuses a task id.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedClientState {
    next_task: u64,
    tasks: std::collections::HashMap<String, Task>,
}

/// A client agent: discovers providers, pays for a task, and awaits its result.
pub struct A2aClient {
    agent: Agent,
    messaging: Arc<dyn Messaging>,
    next_task: u64,
    /// Last-known state of every task this client has started.
    tasks: std::collections::HashMap<String, Task>,
    /// Where the ledger is persisted, if durability is enabled.
    state_path: Option<std::path::PathBuf>,
}

impl A2aClient {
    #[must_use]
    pub fn new(agent: Agent, messaging: Arc<dyn Messaging>) -> Self {
        Self {
            agent,
            messaging,
            next_task: 0,
            tasks: std::collections::HashMap::new(),
            state_path: None,
        }
    }

    /// Build a client whose task ledger is persisted at `state_path`, restoring
    /// any previously-saved tasks so they survive a restart. The client's
    /// per-period spending accumulator is persisted alongside (at
    /// `<state_path>.period.json`), so task payments count against the period
    /// limit across restarts too.
    pub fn with_state(
        agent: Agent,
        messaging: Arc<dyn Messaging>,
        state_path: std::path::PathBuf,
    ) -> Result<Self> {
        let mut client = Self::new(agent, messaging);
        if state_path.exists() {
            let bytes = std::fs::read(&state_path).context("reading a2a client state")?;
            let state: PersistedClientState =
                serde_json::from_slice(&bytes).context("parsing a2a client state")?;
            client.next_task = state.next_task;
            client.tasks = state.tasks;
        }
        client.state_path = Some(state_path.clone());
        let period_state = state_path.with_extension("period.json");
        client
            .agent
            .enable_period_persistence(period_state)
            .context("loading a2a client period-spend state")?;
        Ok(client)
    }

    /// Persist the task ledger if durability is enabled.
    fn persist(&self) -> Result<()> {
        if let Some(path) = &self.state_path {
            let state = PersistedClientState {
                next_task: self.next_task,
                tasks: self.tasks.clone(),
            };
            let bytes = serde_json::to_vec(&state).context("encoding a2a client state")?;
            std::fs::write(path, bytes).context("writing a2a client state")?;
        }
        Ok(())
    }

    /// The last-known state of a tracked task, if any (e.g. after a restart).
    #[must_use]
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Ids of every task this client is tracking.
    #[must_use]
    pub fn tracked_ids(&self) -> Vec<String> {
        self.tasks.keys().cloned().collect()
    }

    /// The client agent's own account (payer).
    #[must_use]
    pub const fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Discover Agent Cards published on `discovery_topic` (`agent.discover`).
    /// Cards whose signature is present and valid can be checked with
    /// [`AgentCard::verify`]; unsigned cards are returned as-is for backward
    /// compatibility with earlier deployments.
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
            // Named so the provider can refund this exact payment on cancel.
            "from": self.agent.account_id().to_string(),
            "priceLez": price.to_string(),
        }))
        .context("encoding task request")?;
        self.messaging.send(&provider.address, &request).await?;

        let task = Task {
            id: task_id.clone(),
            state: TaskState::Submitted,
            result: None,
        };
        self.tasks.insert(task_id, task.clone());
        self.persist()?;
        Ok(task)
    }

    /// Submit a task after an external wallet has settled its payment.
    ///
    /// This is useful on networks where public token transfers work but the
    /// shielded sender proof needed by `run_task` is unavailable. The caller is
    /// responsible for verifying `payment_tx` before calling this method.
    pub async fn run_task_with_payment(
        &mut self,
        provider: &AgentCard,
        skill: &str,
        params: Value,
        payment_tx: &str,
    ) -> Result<Task> {
        let price = provider
            .price_of(skill)
            .with_context(|| format!("provider does not offer skill {skill}"))?;
        ensure!(
            !payment_tx.is_empty(),
            "payment transaction must not be empty"
        );
        ensure!(
            provider.lez_account != self.agent.account_id().to_string(),
            "provider payment account must differ from the client account"
        );
        let task_id = format!("task-{}", self.next_task);
        self.next_task += 1;
        let request = serde_json::to_vec(&json!({
            "kind": "task_request",
            "taskId": task_id,
            "skill": skill,
            "params": params,
            "from": self.agent.account_id().to_string(),
            "priceLez": price.to_string(),
            "paymentTx": payment_tx,
        }))
        .context("encoding externally paid task request")?;
        self.messaging.send(&provider.address, &request).await?;
        let task = Task {
            id: task_id.clone(),
            state: TaskState::Submitted,
            result: None,
        };
        self.tasks.insert(task_id, task.clone());
        self.persist()?;
        Ok(task)
    }

    /// Read the latest status for `task` from `provider`'s update stream
    /// (`agent.subscribe`). Returns the task advanced to its newest known state
    /// and records that state in the (optionally persisted) ledger.
    pub async fn poll_task(&mut self, provider: &AgentCard, task: &Task) -> Result<Task> {
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
            if let Ok(state) = serde_json::from_value::<TaskState>(update["state"].clone()) {
                latest.state = state;
                latest.result = update
                    .get("result")
                    .cloned()
                    .filter(|value| !value.is_null());
            }
        }
        self.tasks.insert(latest.id.clone(), latest.clone());
        self.persist()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpendingPolicy;
    use crate::messaging::InMemoryMessaging;

    fn test_agent() -> Agent {
        let account_id: AccountId = "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
            .parse()
            .expect("valid account id");
        Agent::from_parts(
            account_id,
            SpendingPolicy {
                per_tx_limit: 10,
                per_period_limit: 0,
                period_seconds: 86_400,
            },
        )
    }

    /// The client's task ledger and next-id counter survive a restart.
    #[tokio::test]
    async fn client_task_ledger_survives_a_restart() {
        let path = std::env::temp_dir().join(format!("a2a-client-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let messaging = Arc::new(InMemoryMessaging::new());

        // First client tracks a task, then goes away.
        {
            let mut client =
                A2aClient::with_state(test_agent(), Arc::clone(&messaging) as Arc<_>, path.clone())
                    .unwrap();
            client.next_task = 5;
            client.tasks.insert(
                "task-4".to_owned(),
                Task {
                    id: "task-4".to_owned(),
                    state: TaskState::Working,
                    result: None,
                },
            );
            client.persist().unwrap();
        }

        // A restarted client restores the ledger from disk.
        let restored =
            A2aClient::with_state(test_agent(), Arc::clone(&messaging) as Arc<_>, path.clone())
                .unwrap();
        assert_eq!(restored.tracked_ids(), vec!["task-4".to_owned()]);
        assert_eq!(
            restored.task("task-4").map(|t| &t.state),
            Some(&TaskState::Working),
            "task state should survive the restart"
        );
        assert_eq!(restored.next_task, 5, "next-id counter should survive too");

        let _ = std::fs::remove_file(&path);
    }

    /// A provider's Agent Card is signed and verifies against its embedded key.
    #[tokio::test]
    async fn agent_card_is_signed_and_verifies() {
        let messaging = Arc::new(InMemoryMessaging::new());
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(crate::skills::EchoSkill));
        let provider = A2aProvider::new(
            test_agent(),
            Arc::clone(&messaging) as Arc<_>,
            registry,
            "signer-agent",
            &[("demo.echo", 5)],
        );
        let card = provider.card();
        assert!(!card.signing_pubkey.is_empty(), "card must carry a pubkey");
        assert!(card.signature.is_some(), "card must be signed");
        assert!(card.verify(), "card must verify against its own key");
    }

    /// A tampered card (a changed price) fails verification — the signature
    /// commits to the full card body, not just the identity fields.
    #[tokio::test]
    async fn tampered_agent_card_fails_verification() {
        let messaging = Arc::new(InMemoryMessaging::new());
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(crate::skills::EchoSkill));
        let provider = A2aProvider::new(
            test_agent(),
            Arc::clone(&messaging) as Arc<_>,
            registry,
            "signer-agent",
            &[("demo.echo", 5)],
        );
        let mut card = provider.card().clone();
        card.skills[0].price_lez = "999".to_owned();
        assert!(!card.verify(), "a tampered price must fail verification");
    }

    /// A card discovered over the wire round-trips through serialization and
    /// still verifies — the signature survives the JSON transport.
    #[tokio::test]
    async fn signed_card_round_trips_through_discovery() {
        const DISCOVERY: &str = "/logos-agent/1/a2a/discovery-test/proto";
        let messaging = Arc::new(InMemoryMessaging::new());
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(crate::skills::EchoSkill));
        let provider = A2aProvider::new(
            test_agent(),
            Arc::clone(&messaging) as Arc<_>,
            registry,
            "discovered-agent",
            &[("demo.echo", 10)],
        );
        provider.publish_card(DISCOVERY).await.unwrap();

        let client = A2aClient::new(test_agent(), Arc::clone(&messaging) as Arc<_>);
        let cards = client.discover(DISCOVERY).await.unwrap();
        assert_eq!(cards.len(), 1, "should discover the published card");
        assert!(cards[0].verify(), "discovered card must verify");
    }

    /// A stable signing key keeps the same `signing_pubkey` across providers,
    /// so a redeployed agent's card still verifies for clients that pinned it.
    #[test]
    fn stable_signing_key_keeps_pubkey_across_redeploys() {
        let key = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let pubkey = hex::encode(key.verifying_key().as_bytes());

        let card_of = |name: &str| {
            let messaging = Arc::new(InMemoryMessaging::new());
            let registry = SkillRegistry::new();
            A2aProvider::new_with_signing_key(
                test_agent(),
                messaging,
                registry,
                name,
                &[],
                key.clone(),
            )
            .card()
            .clone()
        };

        let first = card_of("v1");
        let second = card_of("v2");
        assert_eq!(first.signing_pubkey, pubkey);
        assert_eq!(second.signing_pubkey, pubkey, "pubkey must be stable");
        assert!(first.verify() && second.verify());
    }

    /// A skill that always fails is isolated: the provider surfaces the task as
    /// `failed` with the error and keeps serving — a concurrently queued working
    /// task in the same round still completes. This is the reliability
    /// requirement that a failing skill must not crash the module or affect
    /// other concurrently running skills.
    #[tokio::test]
    async fn failing_skill_is_isolated_and_does_not_affect_other_tasks() {
        use crate::skills::{EchoSkill, Skill, SkillContext, SkillRegistry as Registry};

        /// A third-party skill that always fails — registered exactly as a real
        /// third-party skill would be.
        struct ExplodingSkill;
        #[async_trait::async_trait(?Send)]
        impl Skill for ExplodingSkill {
            fn name(&self) -> &'static str {
                "demo.explode"
            }
            fn description(&self) -> &'static str {
                "Always fails; used to prove failure isolation."
            }
            async fn invoke(
                &self,
                _ctx: &mut SkillContext<'_>,
                _args: serde_json::Value,
            ) -> anyhow::Result<serde_json::Value> {
                anyhow::bail!("demo.explode failed on purpose")
            }
        }

        let messaging = Arc::new(InMemoryMessaging::new());
        let mut registry = Registry::new();
        registry.register(Box::new(EchoSkill));
        registry.register(Box::new(ExplodingSkill));
        let provider_agent = test_agent();
        let provider_account = provider_agent.account_id();
        let provider = A2aProvider::new(
            provider_agent,
            Arc::clone(&messaging) as Arc<_>,
            registry,
            "isolated-provider",
            &[("demo.echo", 1), ("demo.explode", 1)],
        );
        let card = provider.card().clone();

        // A client with a distinct account queues a failing task and a working
        // task against the same provider.
        let client_account: AccountId = "8QCqovq4QLCZBkMToNs1sXmTAX6NCJzicJB3umRuQaeA"
            .parse()
            .expect("valid client account id");
        let mut client = A2aClient::new(
            Agent::from_parts(
                client_account,
                SpendingPolicy {
                    per_tx_limit: 10,
                    per_period_limit: 0,
                    period_seconds: 86_400,
                },
            ),
            Arc::clone(&messaging) as Arc<_>,
        );
        let failing = client
            .run_task_with_payment(&card, "demo.explode", json!({}), "unit-test-payment")
            .await
            .expect("failing task request should submit");
        let working = client
            .run_task_with_payment(&card, "demo.echo", json!({ "text": "still-alive" }), "unit-test-payment")
            .await
            .expect("working task request should submit");

        // One serve round processes both: the failing task must be surfaced as
        // `failed` (not crash the provider), and the working task must complete
        // normally despite its neighbour failing.
        let served = provider.serve_pending(None).await.expect("serve round");
        assert_eq!(served, 2, "both tasks should be served in the round");
        let failed = client
            .poll_task(&card, &failing)
            .await
            .expect("poll failing task");
        assert_eq!(failed.state, TaskState::Failed);
        assert!(
            failed
                .result
                .as_ref()
                .and_then(|value| value["error"].as_str())
                .unwrap_or_default()
                .contains("demo.explode failed on purpose"),
            "the failure error should be surfaced, got {:?}",
            failed.result
        );
        let done = client
            .poll_task(&card, &working)
            .await
            .expect("poll working task");
        assert_eq!(
            done.state,
            TaskState::Completed,
            "the neighbouring task must not be affected by the failing skill"
        );
        assert_eq!(
            done.result.as_ref().and_then(|value| value["echo"].as_str()),
            Some("still-alive")
        );

        // The provider survived the failing skill: it returned 2 (both tasks
        // served) rather than panicking, and the working task completed despite
        // its neighbour failing. (A second serve round is not asserted here:
        // the in-memory messaging backend does not consume messages on poll, so
        // it would re-serve; the real Waku backend's store has retention, not
        // consumption. The isolation is proven by the single round above.)
        let _ = provider_account; // documented: provider identity unused beyond setup
    }
}
