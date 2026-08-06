# Logos Autonomous Agent

A Logos-native autonomous agent: a module that gives an AI agent its own shielded
LEZ wallet, private file storage, encrypted messaging, an owner-controlled
spending policy, and an A2A-compatible way to hire and pay other agents — all over
Logos infrastructure, with no custodian and no central server.

Built for [λ-Prize LP-0008](https://github.com/logos-co/lambda-prize/blob/master/prizes/LP-0008.md).

## What it does

- **Sovereign identity and funds.** The agent holds its own shielded LEZ account.
  Its transactions are privacy-preserving — indistinguishable on-chain from any
  other account — and it can send and receive tokens independently of its owner.
- **Owner-controlled spending.** The owner sets a per-transaction limit. Below it
  the agent spends autonomously; above it the agent asks the owner over an
  encrypted channel and waits for a decision before moving any funds.
- **Composable skills.** Capabilities are discrete skills invoked by name with
  JSON arguments. New skills — including third-party ones — register without
  touching the agent core.
- **Agent-to-agent coordination.** Agents publish A2A Agent Cards, discover each
  other on a shared topic, and run tasks through the A2A lifecycle, paying the
  declared price in LEZ per task.

## Architecture

The agent is a library (`logos_agent`) plus a headless binary (`agent`). It builds
directly on the Logos Execution Zone wallet, so the shielded account, private
transactions, and RISC0 proving are the platform's, not reimplemented here.

| Module | Responsibility |
|---|---|
| `lib.rs` (`Agent`) | Shielded identity, balance, policy-gated `send` |
| `skills.rs` | `Skill` trait, `SkillRegistry`, dispatch by name, the Blockchain and reflective `meta.*` skills |
| `storage.rs` | `Storage` trait; `CodexStorage` (real) and `InMemoryStorage`; client-side AES-256-GCM encryption |
| `messaging.rs` | `Messaging` trait; `WakuMessaging` (real nwaku REST) and `InMemoryMessaging` |
| `owner.rs` | `OwnerChannel` + `AgentRuntime`: the approval workflow and configuration |
| `a2a.rs` | `AgentCard`, task lifecycle, `A2aProvider` and `A2aClient` |
| `bin/agent.rs` | Single-command headless deployment |

### Skill interface

A skill implements one trait:

```rust
#[async_trait]
pub trait Skill: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn params(&self) -> Vec<ParamSpec> { Vec::new() }
    async fn invoke(&self, ctx: &mut SkillContext<'_>, args: Value) -> Result<Value>;
}
```

Register it and it is dispatchable by name and listed by `meta.skills`. No core
change is needed to add a capability. The default skills are:

- **Storage** — `storage.upload`, `storage.download`, `storage.list`, `storage.share`
- **Messaging** — `messaging.send`, `messaging.join`, `messaging.create_group`
- **Blockchain** — `wallet.balance`, `wallet.send`
- **Meta** — `meta.skills`, `meta.status`

### Spending threshold

`Agent::send` compares the amount against the owner's per-transaction limit.
Within the limit it submits the transfer. Over the limit it returns
`NeedsOwnerApproval` and moves nothing. The `AgentRuntime` turns that into a
message on the owner channel and holds the spend in a pending map until the owner
replies `approve` or `deny`; an approval releases the transfer, a denial drops it.
The owner can also raise or lower the limit at runtime over the same channel.

An owner-approved over-limit spend is the only path that bypasses the limit, via
`Agent::send_approved`, which callers other than the approval flow should not use.

### A2A coordination

Agent Cards follow the A2A schema (`protocolVersion`, `name`, `capabilities`,
`skills`), with two Logos-native substitutions:

- **Transport** is Logos Messaging. The card's `address` is a Waku topic instead
  of an HTTP URL; task requests and status updates are messages on derived topics.
- **Payment** is a LEZ transfer. Each advertised skill carries a `priceLez`, and
  the client pays that price to the provider's `lezAccount` on request — the gap
  A2A leaves to the implementer.

Tasks move through the A2A lifecycle (`submitted → working → completed | failed`).
A provider serves tasks by dispatching the requested skill through its own
`SkillRegistry`, so any registered skill can be sold.

## Security model

- The agent signs with its own shielded key; the owner's keys are never on the
  agent's node.
- Funds above the per-transaction limit cannot move without an owner decision
  delivered over the encrypted owner channel.
- Files are encrypted client-side before upload; the storage node holds only
  ciphertext.
- The agent cannot change its own spending limit; only the owner can, over the
  owner channel.

### Known limitations

- Task cancellation currently notifies the provider but does not yet automate the
  refund.
- `program.query`, `program.call`, `program.deploy`, and `wallet.history` are on
  the roadmap on top of the same wallet primitives.

## Running

### Deploy an agent (headless)

The wallet and sequencer connection come from the standard LEE wallet environment.

```bash
export LEE_WALLET_HOME_DIR=/path/to/wallet
agent --owner <owner-identity> --spending-limit 50 \
      --messaging-url http://127.0.0.1:8645
```

### End-to-end demo

`scripts/demo.sh` runs the full flow against a local sequencer with real proofs
(`RISC0_DEV_MODE=0`). See the script header for what it exercises.

## Tests

Integration tests run against an in-process LEZ stack (bedrock, sequencer,
indexer, wallet), brought up automatically via Docker:

```bash
RISC0_DEV_MODE=1 cargo test -p logos_agent   # fast, for iteration
RISC0_DEV_MODE=0 cargo test -p logos_agent   # real proofs, as evaluated
```

| Test | Proves |
|---|---|
| `agent_spending` | shielded identity, balance, autonomous vs. held spend |
| `skills_dispatch` | skills invoked by name through the registry |
| `storage_messaging_skills` | file encryption round-trip and message delivery |
| `owner_approval_flow` | approve / deny / reconfigure over the owner channel |
| `a2a_two_agents` | discovery, task lifecycle, autonomous LEZ payment |

## License

Dual-licensed under MIT or Apache-2.0.
