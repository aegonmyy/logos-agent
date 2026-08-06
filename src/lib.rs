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

use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, bail};
use lee::AccountId;
use token_core::TokenHolding;
use wallet::{
    WalletCore,
    account::AccountIdWithPrivacy,
    cli::{
        Command, CliAccountMention, SubcommandReturnValue,
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
}

impl SpendingPolicy {
    /// Whether a spend of `amount` is within the autonomous limit.
    #[must_use]
    pub const fn allows(&self, amount: u128) -> bool {
        amount <= self.per_tx_limit
    }
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
        self.policy.lock().expect("policy lock poisoned").per_tx_limit
    }

    /// Update the autonomous spending limit. Owner-driven (via `meta.configure`
    /// over the owner channel); never changed by the agent itself.
    pub fn set_policy_limit(&self, per_tx_limit: u128) {
        self.policy.lock().expect("policy lock poisoned").per_tx_limit = per_tx_limit;
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
        let limit = self.policy_limit();
        if amount > limit {
            return Ok(SpendOutcome::NeedsOwnerApproval {
                amount,
                to: recipient,
                limit,
            });
        }
        self.execute_send(wallet, recipient, amount).await?;
        Ok(SpendOutcome::Executed {
            amount,
            to: recipient,
        })
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
            .push(TxRecord { to: recipient, amount });
        Ok(())
    }
}

/// Refer to a private account in a wallet CLI command.
fn private_mention(account_id: AccountId) -> CliAccountMention {
    CliAccountMention::Id(AccountIdWithPrivacy::Private(account_id))
}
