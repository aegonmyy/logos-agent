# Logos Autonomous Agent

[![CI](https://github.com/aegonmyy/logos-agent/actions/workflows/ci.yml/badge.svg)](https://github.com/aegonmyy/logos-agent/actions/workflows/ci.yml)

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

### 1. Deploy an agent (headless, one command)

The wallet and sequencer connection come from the standard LEE wallet
environment. A single command gives the agent a shielded identity, a spending
limit, and an owner channel, then runs its event loop:

```bash
export LEE_WALLET_HOME_DIR=/path/to/wallet     # wallet + sequencer connection
agent --owner <owner-identity> \               # who approves over-limit spends
      --spending-limit 50 \                     # autonomous per-tx limit (tokens)
      --messaging-url http://127.0.0.1:8645 \   # nwaku REST (Logos Messaging)
      --state-file agent-state.json             # pending approvals persist here
```

On start it prints the agent's shielded account id (fund it from any wallet) and
then waits for owner instructions. All flags also read from env vars
(`AGENT_OWNER`, `AGENT_SPENDING_LIMIT`, `AGENT_MESSAGING_URL`,
`AGENT_STATE_FILE`). The `--state-file` is what makes pending approvals survive a
restart: on relaunch the agent reloads any spends still awaiting the owner.

### 2. Owner interaction (CLI)

Below the limit the agent spends on its own. Above it, the agent posts an
approval request to the owner channel and holds the spend. The owner side of the
channel (the same two Waku topics the agent derives) is driven programmatically
via `OwnerChannel`:

```rust
use logos_agent::owner::OwnerChannel;

let channel = OwnerChannel::open(messaging, &agent_account_id, "<owner-identity>");

// See what the agent is asking for:
for req in channel.poll_agent_requests().await? {
    println!("agent requests: {req}");            // { id, skill, to, amount, limit }
}

// Approve or deny by request id, or change the limit — all over Logos Messaging:
channel.decide("req-0", true).await?;             // approve
channel.decide("req-1", false).await?;            // deny
channel.configure_limit(75).await?;               // raise the autonomous limit
```

The agent applies these on its next poll and prints the resolution
(`Executed` / `Denied` / `Reconfigured`).

### 3. Owner interaction (Basecamp app)

The same owner channel is surfaced as a Basecamp mini-app (`app/`, package
`agent_owner`) so the owner can act from any Logos app instance holding their
keys — no server. It shows the agent's status and skills and lists pending
spend requests with approve / deny controls.

Build the loadable assets and distributable bundles:

```bash
nix build .#owner-install        # the owner app module
nix build .#agent-install        # the core agent module
./scripts/build-ffi.sh           # the Rust core (liblogos_agent.so)

# …or produce standalone, side-loadable .lgx bundles in dist/:
./scripts/package-basecamp.sh    # -> dist/agent.lgx, dist/agent_owner.lgx, liblogos_agent.so
```

Load them into `logoscore` / Basecamp (see `docs/LOGOS_CORE_LOADED.md` for a
verified end-to-end run, and `dist/README.txt` after packaging):

```bash
export LOGOS_AGENT_FFI_PATH=/abs/path/to/liblogos_agent.so
logoscore --config-dir "$LC" load-module agent
logoscore --config-dir "$LC" load-module agent_owner
```

### 4. End-to-end demo

`scripts/demo.sh` runs the full flow against a local sequencer with real proofs
(`RISC0_DEV_MODE=0`) — the on-screen proof generation is the evidence dev mode is
off. See the script header for what it exercises.

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
