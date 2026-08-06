//! The owner channel and approval workflow (LP-0008, stage 3).
//!
//! Every agent keeps a dedicated Logos Messaging channel with its owner. Small
//! spends happen autonomously; a spend above the owner's limit is *not* sent —
//! instead the agent posts an approval request to the owner over this channel
//! and waits. The owner replies over the same channel to approve or deny, and
//! can reconfigure the agent (e.g. raise the limit) the same way.
//!
//! [`OwnerChannel`] is the transport (two Waku topics: agent→owner and
//! owner→agent). [`AgentRuntime`] is the agent-side event loop that proposes
//! spends, tracks pending approvals, and applies owner decisions.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use lee::AccountId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wallet::WalletCore;

use crate::Agent;
use crate::messaging::{Messaging, Topic};

/// How many times the agent tries to reach the owner before giving up on an
/// approval request (and refusing to execute the spend).
const NOTIFY_ATTEMPTS: u32 = 3;
/// Delay between owner-notification attempts.
const NOTIFY_RETRY_DELAY: Duration = Duration::from_millis(200);

/// A private two-way channel between the agent and its owner.
pub struct OwnerChannel {
    messaging: Arc<dyn Messaging>,
    /// Agent → owner (approval requests, notifications).
    to_owner: Topic,
    /// Owner → agent (decisions, configuration).
    to_agent: Topic,
}

impl OwnerChannel {
    /// Open the channel for `agent_id` talking to `owner`. Both topics are
    /// derived from the pair, so both sides compute the same names.
    #[must_use]
    pub fn open(messaging: Arc<dyn Messaging>, agent_id: &AccountId, owner: &str) -> Self {
        let base = format!("/logos-agent/1/owner-{}-{}", agent_id, owner);
        Self {
            messaging,
            to_owner: format!("{base}/to-owner/proto"),
            to_agent: format!("{base}/to-agent/proto"),
        }
    }

    async fn post(&self, topic: &str, message: &Value) -> Result<()> {
        let bytes = serde_json::to_vec(message).context("encoding owner-channel message")?;
        self.messaging.send(topic, &bytes).await?;
        Ok(())
    }

    async fn read(&self, topic: &str) -> Result<Vec<Value>> {
        let raw = self.messaging.poll(topic).await?;
        raw.iter()
            .map(|bytes| serde_json::from_slice(bytes).context("decoding owner-channel message"))
            .collect()
    }

    // --- agent side ---

    /// Agent posts an approval request to the owner, retrying transient failures
    /// a few times before giving up. If every attempt fails the caller must not
    /// execute the spend.
    async fn request_approval(&self, request: &Value) -> Result<()> {
        let mut last_error = None;
        for attempt in 1..=NOTIFY_ATTEMPTS {
            match self.post(&self.to_owner, request).await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < NOTIFY_ATTEMPTS {
                        tokio::time::sleep(NOTIFY_RETRY_DELAY).await;
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("owner notification failed")))
            .context("could not reach the owner after retries")
    }

    /// Agent reads pending owner→agent messages.
    async fn owner_messages(&self) -> Result<Vec<Value>> {
        self.read(&self.to_agent).await
    }

    // --- owner side (used by the Logos app / tests) ---

    /// Owner reads the agent's outgoing requests/notifications.
    pub async fn poll_agent_requests(&self) -> Result<Vec<Value>> {
        self.read(&self.to_owner).await
    }

    /// Owner approves or denies a pending request by id.
    pub async fn decide(&self, request_id: &str, approve: bool) -> Result<()> {
        self.post(
            &self.to_agent,
            &json!({ "type": "decision", "id": request_id, "approve": approve }),
        )
        .await
    }

    /// Owner updates the agent's autonomous spending limit.
    pub async fn configure_limit(&self, per_tx_limit: u128) -> Result<()> {
        self.post(
            &self.to_agent,
            &json!({ "type": "configure", "per_tx_limit": per_tx_limit.to_string() }),
        )
        .await
    }
}

/// What happened to a proposed spend.
#[derive(Debug, PartialEq, Eq)]
pub enum SpendDecision {
    /// Within the limit — sent immediately.
    Executed { amount: u128, to: AccountId },
    /// Over the limit — an approval request was sent to the owner; the id can be
    /// matched against the resolution from [`AgentRuntime::process_owner_messages`].
    Pending { id: String },
}

/// The outcome of applying one owner message.
#[derive(Debug, PartialEq, Eq)]
pub enum Resolved {
    /// A previously-pending spend was approved and sent.
    Executed { id: String, amount: u128 },
    /// A pending spend was denied; nothing was sent.
    Denied { id: String },
    /// The spending limit was changed.
    Reconfigured { per_tx_limit: u128 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingSpend {
    to: AccountId,
    amount: u128,
}

/// The durable part of a runtime: pending approvals and cursors. Persisted so a
/// restarted agent does not lose spends awaiting owner approval.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    pending: HashMap<String, PendingSpend>,
    next_id: u64,
    consumed: usize,
}

/// The agent-side runtime: proposes spends, holds pending approvals, and applies
/// the owner's decisions and configuration changes.
pub struct AgentRuntime {
    agent: Agent,
    channel: OwnerChannel,
    pending: HashMap<String, PendingSpend>,
    next_id: u64,
    // How many owner→agent messages have already been consumed.
    consumed: usize,
    // Where pending state is persisted, if durability is enabled.
    state_path: Option<PathBuf>,
}

impl AgentRuntime {
    #[must_use]
    pub fn new(agent: Agent, channel: OwnerChannel) -> Self {
        Self {
            agent,
            channel,
            pending: HashMap::new(),
            next_id: 0,
            consumed: 0,
            state_path: None,
        }
    }

    /// Build a runtime whose pending state is persisted at `state_path`, loading
    /// any previously-saved state so approvals survive a restart.
    pub fn with_state(agent: Agent, channel: OwnerChannel, state_path: PathBuf) -> Result<Self> {
        let mut runtime = Self::new(agent, channel);
        if state_path.exists() {
            let bytes = std::fs::read(&state_path).context("reading agent state")?;
            let state: PersistedState =
                serde_json::from_slice(&bytes).context("parsing agent state")?;
            runtime.pending = state.pending;
            runtime.next_id = state.next_id;
            runtime.consumed = state.consumed;
        }
        runtime.state_path = Some(state_path);
        Ok(runtime)
    }

    /// Persist pending state if durability is enabled.
    fn persist(&self) -> Result<()> {
        if let Some(path) = &self.state_path {
            let state = PersistedState {
                pending: self.pending.clone(),
                next_id: self.next_id,
                consumed: self.consumed,
            };
            let bytes = serde_json::to_vec(&state).context("encoding agent state")?;
            std::fs::write(path, bytes).context("writing agent state")?;
        }
        Ok(())
    }

    /// Ids of spends currently awaiting owner approval.
    #[must_use]
    pub fn pending_ids(&self) -> Vec<String> {
        self.pending.keys().cloned().collect()
    }

    /// The agent this runtime drives.
    #[must_use]
    pub const fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Propose spending `amount` to `to`. Under the limit it is sent at once;
    /// over the limit an approval request goes to the owner and it waits.
    pub async fn propose_send(
        &mut self,
        wallet: &mut WalletCore,
        to: AccountId,
        amount: u128,
    ) -> Result<SpendDecision> {
        if amount <= self.agent.policy_limit() {
            self.agent.send_approved(wallet, to, amount).await?;
            return Ok(SpendDecision::Executed { amount, to });
        }
        // Over the limit: notify the owner and hold — no wallet needed.
        self.propose_send_no_wallet(to, amount).await
    }

    /// The over-limit half of [`Self::propose_send`]: notify the owner and hold
    /// the spend for approval, without touching the wallet. Errors if `amount` is
    /// within the limit (executing that path needs a wallet) or if the owner
    /// cannot be reached.
    pub async fn propose_send_no_wallet(
        &mut self,
        to: AccountId,
        amount: u128,
    ) -> Result<SpendDecision> {
        if amount <= self.agent.policy_limit() {
            anyhow::bail!("amount is within the limit; call propose_send with a wallet to execute");
        }

        // Reach the owner FIRST. Only if the request is delivered do we hold the
        // spend as pending — a notification we cannot deliver must never leave a
        // spend that could later be executed without the owner having seen it.
        let id = format!("req-{}", self.next_id);
        self.channel
            .request_approval(&json!({
                "type": "approval_request",
                "id": id,
                "skill": "wallet.send",
                "to": to.to_string(),
                "amount": amount.to_string(),
                "limit": self.agent.policy_limit().to_string(),
            }))
            .await
            .context("owner not notified; over-limit spend not held or executed")?;

        self.next_id += 1;
        self.pending.insert(id.clone(), PendingSpend { to, amount });
        self.persist()?;
        Ok(SpendDecision::Pending { id })
    }

    /// Apply any new owner→agent messages: execute approved spends, drop denied
    /// ones, and apply configuration changes. Returns what was resolved.
    pub async fn process_owner_messages(
        &mut self,
        wallet: &mut WalletCore,
    ) -> Result<Vec<Resolved>> {
        let messages = self.channel.owner_messages().await?;
        let mut resolved = Vec::new();

        for message in messages.into_iter().skip(self.consumed) {
            self.consumed += 1;
            match message.get("type").and_then(Value::as_str) {
                Some("decision") => {
                    let id = message
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    let approve = message
                        .get("approve")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if let Some(spend) = self.pending.remove(&id) {
                        if approve {
                            self.agent.send_approved(wallet, spend.to, spend.amount).await?;
                            resolved.push(Resolved::Executed {
                                id,
                                amount: spend.amount,
                            });
                        } else {
                            resolved.push(Resolved::Denied { id });
                        }
                    }
                }
                Some("configure") => {
                    if let Some(limit) = message
                        .get("per_tx_limit")
                        .and_then(Value::as_str)
                        .and_then(|value| value.parse::<u128>().ok())
                    {
                        self.agent.set_policy_limit(limit);
                        resolved.push(Resolved::Reconfigured {
                            per_tx_limit: limit,
                        });
                    }
                }
                _ => {}
            }
        }

        if !resolved.is_empty() {
            self.persist()?;
        }
        Ok(resolved)
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
        Agent::from_parts(account_id, SpendingPolicy { per_tx_limit: 10 })
    }

    /// A messaging backend whose sends always fail — to exercise notify-retry.
    struct FailingMessaging;

    #[async_trait::async_trait]
    impl Messaging for FailingMessaging {
        async fn send(&self, _recipient: &str, _message: &[u8]) -> Result<crate::messaging::MessageId> {
            anyhow::bail!("network down")
        }
        async fn join(&self, _group_id: &str) -> Result<()> {
            Ok(())
        }
        async fn create_group(&self, _members: &[String]) -> Result<Topic> {
            Ok("t".to_owned())
        }
        async fn poll(&self, _topic: &str) -> Result<Vec<Vec<u8>>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn pending_state_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("agent-state-{}", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let agent = test_agent();
        let messaging = Arc::new(InMemoryMessaging::new());
        let channel = OwnerChannel::open(Arc::clone(&messaging) as Arc<_>, &agent.account_id(), "owner");

        // First runtime: an over-limit spend is held and persisted.
        {
            let mut runtime = AgentRuntime::with_state(agent, channel, dir.clone()).unwrap();
            let decision = runtime
                .propose_send_no_wallet("Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE".parse().unwrap(), 50)
                .await
                .unwrap();
            assert!(matches!(decision, SpendDecision::Pending { .. }));
            assert_eq!(runtime.pending_ids().len(), 1);
        }

        // Second runtime restores the pending spend from disk.
        let agent2 = test_agent();
        let channel2 = OwnerChannel::open(Arc::clone(&messaging) as Arc<_>, &agent2.account_id(), "owner");
        let restored = AgentRuntime::with_state(agent2, channel2, dir.clone()).unwrap();
        assert_eq!(restored.pending_ids().len(), 1, "pending spend should survive restart");

        let _ = std::fs::remove_file(&dir);
    }

    #[tokio::test]
    async fn unreachable_owner_means_no_pending_spend() {
        let agent = test_agent();
        let channel = OwnerChannel::open(Arc::new(FailingMessaging) as Arc<_>, &agent.account_id(), "owner");
        let mut runtime = AgentRuntime::new(agent, channel);
        let result = runtime
            .propose_send_no_wallet("Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE".parse().unwrap(), 50)
            .await;
        assert!(result.is_err(), "if the owner can't be reached, propose_send must fail");
        assert_eq!(runtime.pending_ids().len(), 0, "no spend should be held");
    }
}
