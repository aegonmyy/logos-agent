//! Foundation for a Logos-native autonomous agent (LP-0008, stage 1).
//!
//! An [`Agent`] owns a *shielded* LEZ account — the same private-account
//! machinery the wallet already provides — so it is indistinguishable from any
//! other account holder on-chain. It can report its token balance and spend
//! funds, but only up to an owner-set limit: anything larger is held for the
//! owner to approve rather than sent automatically.
//!
//! Later stages add the skill interface, the owner chat channel that carries
//! those approvals, and agent-to-agent coordination. This module is only the
//! identity + wallet + spending-control core they build on.

pub mod a2a;
pub mod ffi;
pub mod messaging;
pub mod owner;
pub mod skills;
pub mod storage;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use lee::AccountId;
use serde::{Deserialize, Serialize};
use token_core::TokenHolding;
use wallet::{
    WalletCore,
    account::AccountIdWithPrivacy,
    cli::{
        CliAccountMention, Command, SubcommandReturnValue,
        account::{AccountSubcommand, NewSubcommand},
        programs::token::TokenProgramAgnosticSubcommand,
    },
};

/// One entry in the agent's transaction history (`wallet.history`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxRecord {
    pub to: AccountId,
    pub amount: u128,
}

/// The limits an owner sets on what the agent may spend on its own.
#[derive(Debug, Clone, Copy)]
pub struct SpendingPolicy {
    /// Largest amount, in token units, the agent may send in a single
    /// transaction without asking the owner first.
    pub per_tx_limit: u128,
    /// Aggregate token limit for one spending period. Zero disables it.
    pub per_period_limit: u128,
    /// Length of the aggregate spending period in seconds.
    pub period_seconds: u64,
}

impl SpendingPolicy {
    /// Whether a spend of `amount` is within the autonomous limit.
    #[must_use]
    pub const fn allows(&self, amount: u128) -> bool {
        amount <= self.per_tx_limit
    }
}

/// The policy verdict for a proposed spend, evaluated *without* moving funds.
/// Both [`Agent::send`] (the skill / A2A path) and
/// [`AgentRuntime::propose_send`](crate::owner::AgentRuntime::propose_send)
/// (the owner-approval path) go through this, so the two enforce the same
/// per-transaction and per-period limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Within both the per-transaction and per-period limits; the caller may
    /// execute the transfer.
    Allow,
    /// Exceeds the per-transaction limit.
    OverPerTx { limit: u128 },
    /// Would exceed the aggregate per-period limit.
    OverPerPeriod { limit: u128 },
}

/// What happened when a spend request was run through the policy.
#[derive(Debug, PartialEq, Eq)]
pub enum SpendOutcome {
    /// Within the limit — the agent sent it autonomously.
    Executed { amount: u128, to: AccountId },
    /// Over the limit — nothing was sent; it awaits owner approval. Stage 3
    /// delivers this proposal to the owner over the chat channel.
    NeedsOwnerApproval {
        amount: u128,
        to: AccountId,
        limit: u128,
    },
}

/// An autonomous agent: a shielded identity plus a spending policy.
///
/// The policy and transaction history are held behind shared, interior-mutable
/// handles so the owner can reconfigure the limit at runtime (`meta.configure`)
/// and skills can read history, all through a shared `&Agent`.
pub struct Agent {
    account_id: AccountId,
    policy: Arc<Mutex<SpendingPolicy>>,
    history: Arc<Mutex<Vec<TxRecord>>>,
    period_spend: Arc<Mutex<PeriodSpend>>,
    /// Where the period-spend accumulator is persisted, if durability is on.
    /// A restart that re-`enable`s the same path restores the accumulator, so
    /// the per-period spending limit survives node/network restarts instead of
    /// resetting to zero.
    period_state_path: Arc<Mutex<Option<PathBuf>>>,
}

#[derive(Debug, Clone, Copy)]
struct PeriodSpend {
    started_at: u64,
    amount: u128,
}

impl Agent {
    /// Mint the agent a fresh shielded (private) LEZ account and bind it to
    /// `policy`. The wallet derives the account's keys; the agent's identity is
    /// this private account.
    pub async fn create(wallet: &mut WalletCore, policy: SpendingPolicy) -> Result<Self> {
        let result = wallet::cli::execute_subcommand(
            wallet,
            Command::Account(AccountSubcommand::New(NewSubcommand::Private {
                cci: None,
                label: None,
            })),
        )
        .await
        .context("failed to create the agent's shielded account")?;

        let SubcommandReturnValue::RegisterAccount { account_id } = result else {
            bail!("wallet did not return a new account id for the agent");
        };

        Ok(Self::from_parts(account_id, policy))
    }

    /// Build an agent around an already-known shielded account and policy —
    /// e.g. re-loading an agent whose account the wallet already holds. No new
    /// account is created.
    #[must_use]
    pub fn from_parts(account_id: AccountId, policy: SpendingPolicy) -> Self {
        Self {
            account_id,
            policy: Arc::new(Mutex::new(policy)),
            history: Arc::new(Mutex::new(Vec::new())),
            period_spend: Arc::new(Mutex::new(PeriodSpend {
                started_at: now_seconds(),
                amount: 0,
            })),
            period_state_path: Arc::new(Mutex::new(None)),
        }
    }

    /// The agent's shielded account id — its on-chain identity.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }

    /// The owner-set per-transaction spending limit.
    #[must_use]
    pub fn policy_limit(&self) -> u128 {
        self.policy
            .lock()
            .expect("policy lock poisoned")
            .per_tx_limit
    }

    /// Update the autonomous spending limit. Owner-driven (via `meta.configure`
    /// over the owner channel); never changed by the agent itself.
    pub fn set_policy_limit(&self, per_tx_limit: u128) {
        self.policy
            .lock()
            .expect("policy lock poisoned")
            .per_tx_limit = per_tx_limit;
    }

    /// Update the aggregate spending limit and period length.
    pub fn set_period_policy(&self, per_period_limit: u128, period_seconds: u64) {
        let mut policy = self.policy.lock().expect("policy lock poisoned");
        policy.per_period_limit = per_period_limit;
        policy.period_seconds = period_seconds;
    }

    /// Return the aggregate spending limit and period length.
    #[must_use]
    pub fn period_policy(&self) -> (u128, u64) {
        let policy = self.policy.lock().expect("policy lock poisoned");
        (policy.per_period_limit, policy.period_seconds)
    }

    /// Persist the per-period spending accumulator at `path`, restoring any
    /// previously-saved accumulator first. After this call every recorded spend
    /// is written through to disk, so a restarted agent keeps its per-period
    /// spending history instead of starting a fresh (empty) accumulator —
    /// without this, restarts would let the agent exceed the owner's period
    /// limit by simply being restarted.
    pub fn enable_period_persistence(&self, path: PathBuf) -> Result<()> {
        if path.exists() {
            let bytes = std::fs::read(&path).context("reading persisted period-spend state")?;
            let state: PersistedPeriod =
                serde_json::from_slice(&bytes).context("parsing persisted period-spend state")?;
            let mut spend = self.period_spend.lock().expect("period lock poisoned");
            spend.started_at = state.started_at;
            spend.amount = state.amount;
        }
        *self
            .period_state_path
            .lock()
            .expect("period path lock poisoned") = Some(path);
        Ok(())
    }

    /// A snapshot of the agent's recent transactions (`wallet.history`).
    #[must_use]
    pub fn history(&self) -> Vec<TxRecord> {
        self.history.lock().expect("history lock poisoned").clone()
    }

    /// The agent's holding of `token`, or `0` if it holds none. Reads the
    /// wallet's local private-account state, so sync the wallet first if you
    /// need the latest on-chain figure.
    pub fn balance(&self, wallet: &WalletCore, token: AccountId) -> u128 {
        match wallet.get_account_private(self.account_id) {
            Some(account) => match TokenHolding::try_from(&account.data) {
                Ok(TokenHolding::Fungible {
                    definition_id,
                    balance,
                }) if definition_id == token => balance,
                _ => 0,
            },
            None => 0,
        }
    }

    /// Evaluate the spending policy for `amount` without moving funds. Returns
    /// which limit a proposed spend would trip, or [`PolicyDecision::Allow`].
    /// The per-period check may roll the accumulator's window forward as a side
    /// effect of reading it, exactly as [`Agent::send`] does before executing.
    #[must_use]
    pub fn check_policy(&self, amount: u128) -> PolicyDecision {
        let per_tx = self.policy_limit();
        if amount > per_tx {
            return PolicyDecision::OverPerTx { limit: per_tx };
        }
        if !self.period_allows(amount) {
            let (period_limit, _) = self.period_policy();
            return PolicyDecision::OverPerPeriod { limit: period_limit };
        }
        PolicyDecision::Allow
    }

    /// Send `amount` of `token` to `recipient`, subject to the spending policy.
    ///
    /// Below the limit the transfer is submitted as a privacy-preserving
    /// transaction and [`SpendOutcome::Executed`] is returned. Above the limit
    /// nothing is submitted and [`SpendOutcome::NeedsOwnerApproval`] is returned
    /// for the owner to decide.
    pub async fn send(
        &self,
        wallet: &mut WalletCore,
        recipient: AccountId,
        amount: u128,
    ) -> Result<SpendOutcome> {
        match self.check_policy(amount) {
            PolicyDecision::Allow => {
                self.execute_send(wallet, recipient, amount).await?;
                Ok(SpendOutcome::Executed {
                    amount,
                    to: recipient,
                })
            }
            PolicyDecision::OverPerTx { limit }
            | PolicyDecision::OverPerPeriod { limit } => {
                Ok(SpendOutcome::NeedsOwnerApproval {
                    amount,
                    to: recipient,
                    limit,
                })
            }
        }
    }

    /// Send `amount` to `recipient` bypassing the spending policy. Only call this
    /// once the owner has explicitly approved an over-limit spend.
    pub async fn send_approved(
        &self,
        wallet: &mut WalletCore,
        recipient: AccountId,
        amount: u128,
    ) -> Result<()> {
        self.execute_send(wallet, recipient, amount).await
    }

    /// Submit the token transfer. No policy check — callers gate this.
    async fn execute_send(
        &self,
        wallet: &mut WalletCore,
        recipient: AccountId,
        amount: u128,
    ) -> Result<()> {
        wallet::cli::execute_subcommand(
            wallet,
            Command::Token(TokenProgramAgnosticSubcommand::Send {
                from: private_mention(self.account_id),
                to: Some(private_mention(recipient)),
                to_npk: None,
                to_vpk: None,
                to_keys: None,
                to_identifier: Some(0),
                amount,
            }),
        )
        .await
        .context("agent token send failed")?;
        self.history
            .lock()
            .expect("history lock poisoned")
            .push(TxRecord {
                to: recipient,
                amount,
            });
        self.record_period_spend(amount);
        Ok(())
    }

    fn period_allows(&self, amount: u128) -> bool {
        let (limit, period_seconds) = self.period_policy();
        if limit == 0 || period_seconds == 0 {
            return true;
        }
        let mut spend = self.period_spend.lock().expect("period lock poisoned");
        let now = now_seconds();
        if now.saturating_sub(spend.started_at) >= period_seconds {
            spend.started_at = now;
            spend.amount = 0;
        }
        spend.amount.saturating_add(amount) <= limit
    }

    fn record_period_spend(&self, amount: u128) {
        let (limit, period_seconds) = self.period_policy();
        if limit == 0 || period_seconds == 0 {
            return;
        }
        let snapshot = {
            let mut spend = self.period_spend.lock().expect("period lock poisoned");
            let now = now_seconds();
            if now.saturating_sub(spend.started_at) >= period_seconds {
                spend.started_at = now;
                spend.amount = 0;
            }
            spend.amount = spend.amount.saturating_add(amount);
            *spend
        };
        self.persist_period(&snapshot);
    }

    /// Write the accumulator through to disk if persistence is enabled. Best
    /// effort: the spend has already happened, so a persistence failure is
    /// logged and never fails the transaction path.
    fn persist_period(&self, snapshot: &PeriodSpend) {
        let path = self
            .period_state_path
            .lock()
            .expect("period path lock poisoned")
            .clone();
        let Some(path) = path else { return };
        let state = PersistedPeriod {
            started_at: snapshot.started_at,
            amount: snapshot.amount,
        };
        match serde_json::to_vec(&state) {
            Ok(bytes) => {
                if let Err(error) = std::fs::write(&path, bytes) {
                    log::warn!(
                        "could not persist period-spend state to {}: {error}",
                        path.display()
                    );
                }
            }
            Err(error) => log::warn!("could not encode period-spend state: {error}"),
        }
    }
}

/// The durable form of the period-spend accumulator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct PersistedPeriod {
    started_at: u64,
    amount: u128,
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Refer to a private account in a wallet CLI command.
fn private_mention(account_id: AccountId) -> CliAccountMention {
    CliAccountMention::Id(AccountIdWithPrivacy::Private(account_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_policy() -> SpendingPolicy {
        SpendingPolicy {
            per_tx_limit: 1000,
            per_period_limit: 100,
            period_seconds: 3600,
        }
    }

    fn test_account_id() -> AccountId {
        "Ds8q5PjLcKwwV97Zi7duhRVF9uwA2PuYMoLL7FwCzsXE"
            .parse()
            .expect("valid account id")
    }

    /// The per-period accumulator must survive a restart: a fresh agent that
    /// re-enables persistence on the same file inherits the spent amount, so
    /// restarting cannot be used to reset the period allowance.
    #[test]
    fn period_accumulator_survives_restart() {
        let path = std::env::temp_dir().join(format!("agent-period-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);

        {
            let agent = Agent::from_parts(test_account_id(), test_policy());
            agent.enable_period_persistence(path.clone()).unwrap();
            // Spend 60 of the 100 per-period allowance.
            agent.record_period_spend(60);
            assert!(agent.period_allows(40), "60 + 40 = 100 is at the limit");
            assert!(!agent.period_allows(41), "60 + 41 = 101 exceeds the limit");
        }

        // A restarted agent restores the persisted accumulator.
        let agent = Agent::from_parts(test_account_id(), test_policy());
        agent.enable_period_persistence(path.clone()).unwrap();
        assert!(
            !agent.period_allows(50),
            "60 already spent this period; 50 more must exceed 100"
        );
        assert!(agent.period_allows(40), "60 + 40 = 100 is at the limit");

        let _ = std::fs::remove_file(&path);
    }

    /// Without persistence enabled the accumulator is in-memory only, and the
    /// allowance still rolls over once the period elapses.
    #[test]
    fn period_accumulator_resets_after_period_elapses() {
        let mut policy = test_policy();
        policy.period_seconds = 1; // one-second window for the test
        let agent = Agent::from_parts(test_account_id(), policy);
        agent.record_period_spend(80);
        assert!(!agent.period_allows(30), "80 + 30 exceeds 100");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(
            agent.period_allows(80),
            "after the period elapses the allowance resets"
        );
    }
}
