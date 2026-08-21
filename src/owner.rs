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
        // Waku content topics must be exactly four segments
        // ({app}/{version}/{topic}/{encoding}); in-memory backends accept any
        // string, so keep the direction inside the third segment rather than
        // adding a fifth one.
        let base = format!("owner-{agent_id}-{owner}");
        Self {
            messaging,
            to_owner: format!("/logos-agent/1/{base}-to-owner/proto"),
            to_agent: format!("/logos-agent/1/{base}-to-agent/proto"),
        }
    }

    async fn post(&self, topic: &str, message: &Value) -> Result<()> {
        let bytes = serde_json::to_vec(message).context("encoding owner-channel message")?;
        self.messaging.send(topic, &bytes).await?;
        Ok(())
    }

    /// Subscribe the messaging backend to both channel topics. Waku only
    /// stores and serves messages for content topics the node has subscribed
    /// to, so a Waku-backed channel must call this once before use (on either
    /// side — subscription is per node, not per client). In-memory backends
    /// ignore it. Failures are swallowed: on backends where the round-trip
    /// works without subscribing, a rejection here must not break the channel.
    pub async fn subscribe(&self) -> Result<()> {
        let _ = self.messaging.join(&self.to_owner).await;
        let _ = self.messaging.join(&self.to_agent).await;
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

    /// Owner updates the aggregate spending limit and period length.
    pub async fn configure_period(
        &self,
        per_period_limit: u128,
        period_seconds: u64,
    ) -> Result<()> {
        self.post(
            &self.to_agent,
            &json!({
                "type": "configure_period",
                "per_period_limit": per_period_limit.to_string(),
                "period_seconds": period_seconds,
            }),
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
    /// The aggregate period spending policy was changed.
    PeriodReconfigured {
        per_period_limit: u128,
        period_seconds: u64,
    },
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

    /// Propose spending `amount` to `to`. Within both the per-transaction and
    /// per-period limits it is sent at once; over either it posts an approval
    /// request to the owner and waits. The owner-escalation path is the same
    /// whether the per-transaction or the per-period limit is tripped.
    pub async fn propose_send(
        &mut self,
        wallet: &mut WalletCore,
        to: AccountId,
        amount: u128,
    ) -> Result<SpendDecision> {
        match self.agent.check_policy(amount) {
            crate::PolicyDecision::Allow => {
                self.agent.send_approved(wallet, to, amount).await?;
                Ok(SpendDecision::Executed { amount, to })
            }
            crate::PolicyDecision::OverPerTx { limit }
            | crate::PolicyDecision::OverPerPeriod { limit } => {
                self.hold_for_approval(to, amount, limit).await
            }
        }
    }

    /// The over-limit half of [`Self::propose_send`]: notify the owner and hold
    /// the spend for approval, without touching the wallet. Errors if `amount`
    /// is within the policy (executing that path needs a wallet) or if the owner
    /// cannot be reached.
    pub async fn propose_send_no_wallet(
        &mut self,
        to: AccountId,
        amount: u128,
    ) -> Result<SpendDecision> {
        match self.agent.check_policy(amount) {
            crate::PolicyDecision::Allow => {
                anyhow::bail!(
                    "amount is within the policy; call propose_send with a wallet to execute"
                );
            }
            crate::PolicyDecision::OverPerTx { limit }
            | crate::PolicyDecision::OverPerPeriod { limit } => {
                self.hold_for_approval(to, amount, limit).await
            }
        }
    }

    /// Reach the owner FIRST, and only if the request is delivered hold the
    /// spend as pending. A notification we cannot deliver must never leave a
    /// spend that could later be executed without the owner having seen it.
    /// `limit` is whichever limit the spend tripped (per-transaction or
    /// per-period), surfaced to the owner in the request.
    async fn hold_for_approval(
        &mut self,
        to: AccountId,
        amount: u128,
        limit: u128,
    ) -> Result<SpendDecision> {
        let id = format!("req-{}", self.next_id);
        self.channel
            .request_approval(&json!({
                "type": "approval_request",
                "id": id,
                "skill": "wallet.send",
                "to": to.to_string(),
                "amount": amount.to_string(),
                "limit": limit.to_string(),
            }))
            .await
            .context("owner not notified; over-limit spend not held or executed")?;

        self.next_id += 1;
        self.pending.insert(id.clone(), PendingSpend { to, amount });
        self.persist()?;
        Ok(SpendDecision::Pending { id })
    }

    /// Read pending owner→agent messages without applying them (no wallet
    /// needed). Useful for inspection and tests; the live loop applies them via
    /// [`process_owner_messages`].
    pub async fn peek_owner_messages(&self) -> Result<Vec<Value>> {
        self.channel.owner_messages().await
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
                            self.agent
                                .send_approved(wallet, spend.to, spend.amount)
                                .await?;
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
                Some("configure_period") => {
                    let limit = message
                        .get("per_period_limit")
                        .and_then(Value::as_str)
                        .and_then(|value| value.parse::<u128>().ok());
                    let seconds = message.get("period_seconds").and_then(Value::as_u64);
                    if let (Some(limit), Some(seconds)) = (limit, seconds) {
                        self.agent.set_period_policy(limit, seconds);
                        resolved.push(Resolved::PeriodReconfigured {
                            per_period_limit: limit,
                            period_seconds: seconds,
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
        Agent::from_parts(
            account_id,
            SpendingPolicy {
                per_tx_limit: 10,
                per_period_limit: 0,
                period_seconds: 86_400,
            },
        )
    }

    /// A messaging backend whose sends always fail — to exercise notify-retry.
    struct FailingMessaging;

    #[async_trait::async_trait]
    impl Messaging for FailingMessaging {
        async fn send(
            &self,
            _recipient: &str,
            _message: &[u8],
        ) -> Result<crate::messaging::MessageId> {
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
        let channel = OwnerChannel::open(
            Arc::clone(&messaging) as Arc<_>,
            &agent.account_id(),
            "owner",
        );

        // First runtime: an over-limit spend is held and persisted.
        {
            let mut runtime = AgentRuntime::with_state(agent, channel, dir.clone()).unwrap();
            let decision = runtime
                .propose_send_no_wallet(
                    "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
                        .parse()
                        .unwrap(),
                    50,
                )
                .await
                .unwrap();
            assert!(matches!(decision, SpendDecision::Pending { .. }));
            assert_eq!(runtime.pending_ids().len(), 1);
        }

        // Second runtime restores the pending spend from disk.
        let agent2 = test_agent();
        let channel2 = OwnerChannel::open(
            Arc::clone(&messaging) as Arc<_>,
            &agent2.account_id(),
            "owner",
        );
        let restored = AgentRuntime::with_state(agent2, channel2, dir.clone()).unwrap();
        assert_eq!(
            restored.pending_ids().len(),
            1,
            "pending spend should survive restart"
        );

        let _ = std::fs::remove_file(&dir);
    }

    #[tokio::test]
    async fn unreachable_owner_means_no_pending_spend() {
        let agent = test_agent();
        let channel = OwnerChannel::open(
            Arc::new(FailingMessaging) as Arc<_>,
            &agent.account_id(),
            "owner",
        );
        let mut runtime = AgentRuntime::new(agent, channel);
        let result = runtime
            .propose_send_no_wallet(
                "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
                    .parse()
                    .unwrap(),
                50,
            )
            .await;
        assert!(
            result.is_err(),
            "if the owner can't be reached, propose_send must fail"
        );
        assert_eq!(runtime.pending_ids().len(), 0, "no spend should be held");
    }

    /// A spend that is *under* the per-transaction limit but over the aggregate
    /// per-period limit is held for the owner — the period limit gates the
    /// runtime path too, not just the skill path.
    #[tokio::test]
    async fn period_over_spend_is_held_for_owner() {
        let account_id: AccountId = "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
            .parse()
            .expect("valid account id");
        let agent = Agent::from_parts(
            account_id,
            SpendingPolicy {
                per_tx_limit: 100,
                per_period_limit: 60,
                period_seconds: 86_400,
            },
        );
        // Already spent 50 this period: 20 more is under the per-tx limit
        // (20 <= 100) but over the period limit (50 + 20 > 60).
        agent.record_period_spend(50);

        let messaging = Arc::new(InMemoryMessaging::new());
        let channel = OwnerChannel::open(
            Arc::clone(&messaging) as Arc<_>,
            &agent.account_id(),
            "owner",
        );
        let mut runtime = AgentRuntime::new(agent, channel);
        let decision = runtime
            .propose_send_no_wallet(
                "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
                    .parse()
                    .unwrap(),
                20,
            )
            .await
            .unwrap();
        assert!(
            matches!(decision, SpendDecision::Pending { .. }),
            "a period-over spend under the per-tx limit must be held, got {decision:?}"
        );

        // The owner's request names the period limit as the one that tripped.
        let owner_view = OwnerChannel::open(
            Arc::clone(&messaging) as Arc<_>,
            &test_agent().account_id(),
            "owner",
        );
        let requests = owner_view.poll_agent_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["limit"], "60");
        assert_eq!(requests[0]["amount"], "20");
    }
}
