//! The agent's skill system (LP-0008, stage 2).
//!
//! A *skill* is a named, documented capability the agent can perform. Skills are
//! registered in a [`SkillRegistry`] and invoked by name with JSON arguments, so
//! new skills — including third-party ones — can be added without touching the
//! agent core: implement [`Skill`] and `register` it. Each invocation receives a
//! [`SkillContext`] giving the skill the agent's identity and wallet.
//!
//! This module ships the Blockchain skill category (`wallet.balance`,
//! `wallet.send`) plus the reflective meta skills (`meta.skills`, `meta.status`).
//! The Storage and Messaging categories plug into the same registry.

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use lee::AccountId;
use lee::program::Program;
use serde::Serialize;
use serde_json::{Value, json};
use wallet::cli::Command;
use wallet::{AccountIdentity, WalletCore};

use crate::messaging::Messaging;
use crate::storage::Storage;
use crate::{Agent, SpendOutcome};

/// Parse a 64-hex-char program id into the platform's `[u32; 8]` form.
fn program_id_from_hex(hex_str: &str) -> Result<[u32; 8]> {
    let bytes = hex::decode(hex_str).map_err(|_| anyhow!("program_id is not valid hex"))?;
    if bytes.len() != 32 {
        bail!("program_id must be 32 bytes (64 hex chars)");
    }
    let mut id = [0u32; 8];
    for (word, chunk) in id.iter_mut().zip(bytes.chunks_exact(4)) {
        *word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    Ok(id)
}

/// Render a `[u32; 8]` program id as a 64-hex-char string.
fn program_id_to_hex(id: [u32; 8]) -> String {
    id.iter()
        .flat_map(|word| word.to_le_bytes())
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// One documented parameter a skill accepts.
#[derive(Debug, Clone, Serialize)]
pub struct ParamSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

impl ParamSpec {
    const fn required(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            required: true,
        }
    }
}

/// Everything a skill needs at call time: the agent, and — for skills that act
/// on-chain — its wallet. Storage and Messaging skills leave `wallet` as `None`.
pub struct SkillContext<'a> {
    pub wallet: Option<&'a mut WalletCore>,
    pub agent: &'a Agent,
}

/// A named, documented capability. Implement this trait and register it to add a
/// new skill without modifying the agent core.
#[async_trait(?Send)]
pub trait Skill: Send + Sync {
    /// Stable identifier, e.g. `"wallet.send"`.
    fn name(&self) -> &'static str;
    /// One-line human description.
    fn description(&self) -> &'static str;
    /// The parameters this skill accepts (surfaced by `meta.skills`).
    fn params(&self) -> Vec<ParamSpec> {
        Vec::new()
    }
    /// Run the skill with JSON `args`, returning a JSON result.
    async fn invoke(&self, ctx: &mut SkillContext<'_>, args: Value) -> Result<Value>;
}

/// The set of skills an agent can perform. Third-party skills are `register`ed
/// here; the reflective `meta.*` skills are always built in.
#[derive(Default)]
pub struct SkillRegistry {
    skills: Vec<Box<dyn Skill>>,
}

impl SkillRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry preloaded with the default Blockchain skills.
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry
            .register(Box::new(WalletBalance))
            .register(Box::new(WalletSend))
            .register(Box::new(WalletHistory))
            .register(Box::new(ProgramQuery))
            .register(Box::new(ProgramCall))
            .register(Box::new(ProgramDeploy));
        registry
    }

    /// Add a skill. A later registration of the same name shadows earlier ones.
    pub fn register(&mut self, skill: Box<dyn Skill>) -> &mut Self {
        self.skills.push(skill);
        self
    }

    /// Register the Storage skill category, backed by `storage`.
    pub fn register_storage(&mut self, storage: Arc<dyn Storage>) -> &mut Self {
        self.register(Box::new(StorageUpload(Arc::clone(&storage))));
        self.register(Box::new(StorageDownload(Arc::clone(&storage))));
        self.register(Box::new(StorageList(Arc::clone(&storage))));
        self.register(Box::new(StorageShare(storage)));
        self
    }

    /// Register the Messaging skill category, backed by `messaging`.
    pub fn register_messaging(&mut self, messaging: Arc<dyn Messaging>) -> &mut Self {
        self.register(Box::new(MessagingSend(Arc::clone(&messaging))));
        self.register(Box::new(MessagingJoin(Arc::clone(&messaging))));
        self.register(Box::new(MessagingCreateGroup(messaging)));
        self
    }

    /// Registered skill names, in registration order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.skills.iter().map(|skill| skill.name()).collect()
    }

    /// A JSON catalogue of every skill and its parameters — the data behind
    /// `meta.skills`. Includes the built-in reflective skills.
    #[must_use]
    pub fn catalogue(&self) -> Value {
        let mut items: Vec<Value> = self
            .skills
            .iter()
            .map(|skill| {
                json!({
                    "name": skill.name(),
                    "description": skill.description(),
                    "params": skill.params(),
                })
            })
            .collect();
        items.push(json!({
            "name": "meta.skills",
            "description": "List all available skills and their parameters.",
            "params": [],
        }));
        items.push(json!({
            "name": "meta.status",
            "description": "Report the agent's identity, spending limit, and skill count.",
            "params": [],
        }));
        items.push(json!({
            "name": "meta.configure",
            "description": "Update runtime configuration, e.g. the spending limit.",
            "params": [
                { "name": "key", "description": "Config key (e.g. per_tx_limit).", "required": true },
                { "name": "value", "description": "New value.", "required": true },
            ],
        }));
        Value::Array(items)
    }

    fn find(&self, name: &str) -> Option<&dyn Skill> {
        // Reverse iteration so a re-registered name shadows the earlier one.
        self.skills
            .iter()
            .rev()
            .find(|skill| skill.name() == name)
            .map(AsRef::as_ref)
    }

    /// Invoke skill `name` with JSON `args`. The reflective `meta.*` skills are
    /// handled here; everything else dispatches to a registered [`Skill`].
    pub async fn dispatch(
        &self,
        name: &str,
        ctx: &mut SkillContext<'_>,
        args: Value,
    ) -> Result<Value> {
        match name {
            "meta.skills" => Ok(self.catalogue()),
            "meta.status" => Ok(json!({
                "account_id": ctx.agent.account_id().to_string(),
                "per_tx_limit": ctx.agent.policy_limit().to_string(),
                "skill_count": self.skills.len() + 3,
            })),
            "meta.configure" => {
                let key = arg_str(&args, "key")?;
                match key.as_str() {
                    "per_tx_limit" => {
                        let limit = arg_amount(&args, "value")?;
                        ctx.agent.set_policy_limit(limit);
                        Ok(json!({ "status": "configured", "per_tx_limit": limit.to_string() }))
                    }
                    other => bail!("unknown configuration key: {other}"),
                }
            }
            _ => {
                let skill = self
                    .find(name)
                    .with_context(|| format!("unknown skill: {name}"))?;
                skill.invoke(ctx, args).await
            }
        }
    }
}

// --- argument helpers -------------------------------------------------------

fn arg_str(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("missing string argument `{key}`"))
}

fn arg_account(args: &Value, key: &str) -> Result<AccountId> {
    arg_str(args, key)?
        .parse::<AccountId>()
        .map_err(|_| anyhow!("`{key}` is not a valid account id"))
}

fn arg_amount(args: &Value, key: &str) -> Result<u128> {
    if let Some(number) = args.get(key).and_then(Value::as_u64) {
        return Ok(u128::from(number));
    }
    arg_str(args, key)?
        .parse::<u128>()
        .map_err(|_| anyhow!("`{key}` is not a valid amount"))
}

/// A trivial demo skill used by examples and the A2A tests: echoes `text` back.
/// It is a stand-in for any specialist capability an agent might sell.
pub struct EchoSkill;

#[async_trait(?Send)]
impl Skill for EchoSkill {
    fn name(&self) -> &'static str {
        "demo.echo"
    }
    fn description(&self) -> &'static str {
        "Echo back the provided text."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::required("text", "Text to echo back.")]
    }
    async fn invoke(&self, _ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let text = arg_str(&args, "text")?;
        Ok(json!({ "echo": text }))
    }
}

// --- Blockchain skills ------------------------------------------------------

/// `wallet.balance` — the agent's holding of a given token.
struct WalletBalance;

#[async_trait(?Send)]
impl Skill for WalletBalance {
    fn name(&self) -> &'static str {
        "wallet.balance"
    }
    fn description(&self) -> &'static str {
        "Return the agent's current balance of a given token."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::required(
            "token",
            "Account id of the token definition to check.",
        )]
    }
    async fn invoke(&self, ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let token = arg_account(&args, "token")?;
        let wallet = ctx
            .wallet
            .as_deref_mut()
            .context("wallet.balance requires a wallet")?;
        wallet.sync_to_latest_block().await?;
        let balance = ctx.agent.balance(wallet, token);
        Ok(json!({
            "token": token.to_string(),
            "balance": balance.to_string(),
        }))
    }
}

/// `wallet.send` — send tokens, enforcing the owner's spending policy.
struct WalletSend;

#[async_trait(?Send)]
impl Skill for WalletSend {
    fn name(&self) -> &'static str {
        "wallet.send"
    }
    fn description(&self) -> &'static str {
        "Send tokens to a recipient; holds for owner approval above the limit."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::required("to", "Recipient account id."),
            ParamSpec::required("amount", "Amount of tokens to send."),
        ]
    }
    async fn invoke(&self, ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let to = arg_account(&args, "to")?;
        let amount = arg_amount(&args, "amount")?;
        let wallet = ctx
            .wallet
            .as_deref_mut()
            .context("wallet.send requires a wallet")?;
        let agent = ctx.agent;
        Ok(match agent.send(wallet, to, amount).await? {
            SpendOutcome::Executed { amount, to } => json!({
                "status": "executed",
                "amount": amount.to_string(),
                "to": to.to_string(),
            }),
            SpendOutcome::NeedsOwnerApproval { amount, to, limit } => json!({
                "status": "needs_owner_approval",
                "amount": amount.to_string(),
                "to": to.to_string(),
                "limit": limit.to_string(),
            }),
        })
    }
}

// --- Storage skills ---------------------------------------------------------

/// `storage.upload` — encrypt a file and store it; returns its content address.
struct StorageUpload(Arc<dyn Storage>);

#[async_trait(?Send)]
impl Skill for StorageUpload {
    fn name(&self) -> &'static str {
        "storage.upload"
    }
    fn description(&self) -> &'static str {
        "Encrypt and store a file on Logos Storage; returns a content address."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::required("label", "A human label for the stored file."),
            ParamSpec::required("data", "The file contents to store."),
        ]
    }
    async fn invoke(&self, _ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let label = arg_str(&args, "label")?;
        let data = arg_str(&args, "data")?;
        let address = self.0.upload(&label, data.as_bytes()).await?;
        Ok(json!({ "address": address }))
    }
}

/// `storage.download` — retrieve and decrypt a stored file.
struct StorageDownload(Arc<dyn Storage>);

#[async_trait(?Send)]
impl Skill for StorageDownload {
    fn name(&self) -> &'static str {
        "storage.download"
    }
    fn description(&self) -> &'static str {
        "Retrieve and decrypt a file from Logos Storage by content address."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::required(
            "address",
            "Content address of the file to retrieve.",
        )]
    }
    async fn invoke(&self, _ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let address = arg_str(&args, "address")?;
        let bytes = self.0.download(&address).await?;
        Ok(json!({ "data": String::from_utf8_lossy(&bytes) }))
    }
}

/// `storage.list` — list the files the agent has stored.
struct StorageList(Arc<dyn Storage>);

#[async_trait(?Send)]
impl Skill for StorageList {
    fn name(&self) -> &'static str {
        "storage.list"
    }
    fn description(&self) -> &'static str {
        "List files the agent has stored, with labels and content addresses."
    }
    async fn invoke(&self, _ctx: &mut SkillContext<'_>, _args: Value) -> Result<Value> {
        let objects: Vec<Value> = self
            .0
            .list()
            .await?
            .into_iter()
            .map(|(label, address)| json!({ "label": label, "address": address }))
            .collect();
        Ok(json!({ "objects": objects }))
    }
}

/// `storage.share` — grant another identity access to a stored file.
struct StorageShare(Arc<dyn Storage>);

#[async_trait(?Send)]
impl Skill for StorageShare {
    fn name(&self) -> &'static str {
        "storage.share"
    }
    fn description(&self) -> &'static str {
        "Share access to a stored file with another Logos identity."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::required("address", "Content address of the file to share."),
            ParamSpec::required("recipient", "Logos identity to share with."),
        ]
    }
    async fn invoke(&self, _ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let address = arg_str(&args, "address")?;
        let recipient = arg_str(&args, "recipient")?;
        self.0.share(&address, &recipient).await?;
        Ok(json!({ "status": "shared", "address": address, "recipient": recipient }))
    }
}

// --- Messaging skills -------------------------------------------------------

/// `messaging.send` — send a message to a user or agent address.
struct MessagingSend(Arc<dyn Messaging>);

#[async_trait(?Send)]
impl Skill for MessagingSend {
    fn name(&self) -> &'static str {
        "messaging.send"
    }
    fn description(&self) -> &'static str {
        "Send a message to a Logos Messaging address (user or agent)."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::required("to", "Recipient address or group topic."),
            ParamSpec::required("message", "The message to send."),
        ]
    }
    async fn invoke(&self, _ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let to = arg_str(&args, "to")?;
        let message = arg_str(&args, "message")?;
        let message_id = self.0.send(&to, message.as_bytes()).await?;
        Ok(json!({ "message_id": message_id }))
    }
}

/// `messaging.join` — join a group topic.
struct MessagingJoin(Arc<dyn Messaging>);

#[async_trait(?Send)]
impl Skill for MessagingJoin {
    fn name(&self) -> &'static str {
        "messaging.join"
    }
    fn description(&self) -> &'static str {
        "Join a Logos Messaging group topic."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::required("group_id", "The group topic to join.")]
    }
    async fn invoke(&self, _ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let group_id = arg_str(&args, "group_id")?;
        self.0.join(&group_id).await?;
        Ok(json!({ "status": "joined", "group_id": group_id }))
    }
}

/// `messaging.create_group` — create a group topic and invite members.
struct MessagingCreateGroup(Arc<dyn Messaging>);

#[async_trait(?Send)]
impl Skill for MessagingCreateGroup {
    fn name(&self) -> &'static str {
        "messaging.create_group"
    }
    fn description(&self) -> &'static str {
        "Create a new group topic and invite the given members."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::required(
            "members",
            "Array of member identities to invite.",
        )]
    }
    async fn invoke(&self, _ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let members: Vec<String> = args
            .get("members")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let group_id = self.0.create_group(&members).await?;
        Ok(json!({ "group_id": group_id }))
    }
}

/// `wallet.history` — a summary of the agent's recent transactions.
struct WalletHistory;

#[async_trait(?Send)]
impl Skill for WalletHistory {
    fn name(&self) -> &'static str {
        "wallet.history"
    }
    fn description(&self) -> &'static str {
        "Return a summary of the agent's recent transactions."
    }
    async fn invoke(&self, ctx: &mut SkillContext<'_>, _args: Value) -> Result<Value> {
        let transactions: Vec<Value> = ctx
            .agent
            .history()
            .into_iter()
            .map(|record| {
                json!({ "to": record.to.to_string(), "amount": record.amount.to_string() })
            })
            .collect();
        Ok(json!({ "transactions": transactions }))
    }
}

/// `program.query` — read the state of an account owned by a LEZ program.
struct ProgramQuery;

#[async_trait(?Send)]
impl Skill for ProgramQuery {
    fn name(&self) -> &'static str {
        "program.query"
    }
    fn description(&self) -> &'static str {
        "Read state from a LEZ program by reading a program-owned account."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::required("account", "Account id whose state to read.")]
    }
    async fn invoke(&self, ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let account_id = arg_account(&args, "account")?;
        let wallet = ctx
            .wallet
            .as_deref_mut()
            .context("program.query requires a wallet")?;
        let account = wallet.get_account_public(account_id).await?;
        Ok(json!({
            "account": account_id.to_string(),
            "state": serde_json::to_value(&account).context("serializing account state")?,
        }))
    }
}

/// `program.call` — submit a transaction to a LEZ program.
struct ProgramCall;

#[async_trait(?Send)]
impl Skill for ProgramCall {
    fn name(&self) -> &'static str {
        "program.call"
    }
    fn description(&self) -> &'static str {
        "Submit a transaction to a LEZ program (public accounts + instruction words)."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::required("program_id", "64-hex-char program id."),
            ParamSpec::required("accounts", "Array of public account ids the call touches."),
            ParamSpec::required("instruction", "Instruction as an array of u32 words."),
        ]
    }
    async fn invoke(&self, ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let program_id = program_id_from_hex(&arg_str(&args, "program_id")?)?;

        let accounts = args
            .get("accounts")
            .and_then(Value::as_array)
            .context("`accounts` must be an array")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .context("account id must be a string")?
                    .parse::<AccountId>()
                    .map(AccountIdentity::Public)
                    .map_err(|_| anyhow!("invalid account id"))
            })
            .collect::<Result<Vec<_>>>()?;

        let instruction: Vec<u32> = args
            .get("instruction")
            .and_then(Value::as_array)
            .context("`instruction` must be an array of numbers")?
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .context("instruction words must be u32")
            })
            .collect::<Result<Vec<_>>>()?;

        let wallet = ctx
            .wallet
            .as_deref_mut()
            .context("program.call requires a wallet")?;
        let hash = wallet
            .send_pub_tx(accounts, instruction, program_id)
            .await
            .map_err(|err| anyhow!("program call failed: {err:?}"))?;
        Ok(json!({ "tx_hash": format!("{hash}") }))
    }
}

/// `program.deploy` — deploy a compiled LEZ program binary; returns its id.
struct ProgramDeploy;

#[async_trait(?Send)]
impl Skill for ProgramDeploy {
    fn name(&self) -> &'static str {
        "program.deploy"
    }
    fn description(&self) -> &'static str {
        "Deploy a compiled LEZ program binary and return its program id."
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::required("binary_path", "Path to the compiled program ELF.")]
    }
    async fn invoke(&self, ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let path = arg_str(&args, "binary_path")?;
        let bytes = std::fs::read(&path).with_context(|| format!("reading program binary {path}"))?;
        let program = Program::new(bytes.into()).map_err(|err| anyhow!("invalid program: {err}"))?;
        let program_id = program.id();

        let wallet = ctx
            .wallet
            .as_deref_mut()
            .context("program.deploy requires a wallet")?;
        wallet::cli::execute_subcommand(
            wallet,
            Command::DeployProgram {
                binary_filepath: path.into(),
            },
        )
        .await
        .context("deploying program")?;

        Ok(json!({ "program_id": program_id_to_hex(program_id) }))
    }
}
