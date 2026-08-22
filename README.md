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
- **Blockchain** — `wallet.balance`, `wallet.send`, `wallet.history`,
  `program.query`, `program.call`, `program.deploy`
- **Meta** — `meta.skills`, `meta.status`, `meta.configure`
- **A2A** — `agent.card`, `agent.discover`, `agent.task`, `agent.subscribe`,
  `agent.cancel`; these are stateful coordination operations backed by
  `A2aProvider`/`A2aClient`, rather than stateless registry calls. `agent.card`
  returns a signed card (see A2A coordination).

### Spending threshold

`Agent::send` checks two owner-set limits. The per-transaction limit gates a
single transfer; the per-period limit gates the aggregate spent within a rolling
window (`per_period_limit` tokens over `period_seconds`). Within both it submits
the transfer. Over either it returns `NeedsOwnerApproval` and moves nothing. The
`AgentRuntime` turns that into a message on the owner channel and holds the spend
in a pending map until the owner replies `approve` or `deny`; an approval releases
the transfer, a denial drops it. The owner can also raise or lower either limit at
runtime over the same channel (`configure_limit`, `configure_period`).

The per-period accumulator is persisted, so restarting the agent does not reset
the period allowance: the headless binary writes it next to its state file, and
`A2aClient::with_state` does the same so task payments count across restarts.

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

Each card is a **signed** document: the publisher signs the card's canonical JSON
with an Ed25519 key and embeds the verifying key alongside the signature, and
`AgentCard::verify` rejects tampered cards (a changed price, a swapped payment
account). Shielded LEZ accounts cannot produce message signatures (their
nullifier/viewing keys generate ZK proofs; the wallet's `sign_message` covers
public accounts and keycards), so the card key is self-certifying: pin it once
over a channel you trust (the owner channel) and later cards under the same
`signingPubkey` verify. Deployments that need a stable key across restarts use
`A2aProvider::new_with_signing_key` with a persisted key.

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

- The Agent Card signature is self-certifying: it proves a card was not tampered
  and that repeat cards come from the same key, but it does not bind the signing
  key to the shielded LEZ account (shielded keys cannot message-sign). Pin the
  key once over a trusted channel before relying on it.
- The Logos Core module loads an offline session: reflective, storage, and
  messaging skills run through the runtime, but the wallet-backed on-chain
  skills are not yet driven through the module session.
- The Basecamp owner app's owner-channel FFI is implemented, unit-tested, and
  verified across the C ABI (a standalone harness dlopens `liblogos_agent.so`
  and exercises every owner-channel symbol), and the Qt module builds with the
  Logos module builder (`nix build .#owner-install`). A runtime approve/deny
  pass against a live agent + Waku node is still needed for full evidence.
- In the recorded real-proof run, the standalone stack's indexer rejects a
  privacy-preserving proof while re-applying an early block and stops indexing
  for the rest of the run. Transaction inclusion and the agent's confirmed state
  are unaffected (the agent reads the sequencer, not the indexer); see
  `docs/DEV_MODE_0_EVIDENCE.md` for what is and is not established.
- No AI model is bundled; the skill interface is model-agnostic and inference is
  left to the deployer (out of scope per the prize).

## Running

### 1. Deploy an agent (headless, one command)

The wallet and sequencer connection come from the standard LEE wallet
environment. A single command gives the agent a shielded identity, a spending
limit, and an owner channel, then runs its event loop:

```bash
export LEE_WALLET_HOME_DIR=/path/to/wallet     # wallet + sequencer connection
agent --owner <owner-identity> \               # who approves over-limit spends
      --spending-limit 50 \                     # autonomous per-tx limit (tokens)
      --period-limit 500 \                      # aggregate spend per period (0 = off)
      --period-seconds 86400 \                  # period length (default: a day)
      --messaging-url http://127.0.0.1:8645 \   # nwaku REST (Logos Messaging)
      --state-file agent-state.json             # pending approvals + period state persist here
```

On start it prints the agent's shielded account id (fund it from any wallet) and
then waits for owner instructions. All flags also read from env vars
(`AGENT_OWNER`, `AGENT_SPENDING_LIMIT`, `AGENT_PERIOD_LIMIT`,
`AGENT_PERIOD_SECONDS`, `AGENT_MESSAGING_URL`, `AGENT_STATE_FILE`). The
`--state-file` is what makes pending approvals survive a restart: on relaunch the
agent reloads any spends still awaiting the owner, and the per-period
accumulator alongside it so restarts cannot reset the period allowance.

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

// Approve or deny by request id, or change the limits — all over Logos Messaging:
channel.decide("req-0", true).await?;             // approve
channel.decide("req-1", false).await?;            // deny
channel.configure_limit(75).await?;               // raise the per-tx limit
channel.configure_period(500, 86_400).await?;     // set the per-period limit + window
```

The agent applies these on its next poll and prints the resolution
(`Executed` / `Denied` / `Reconfigured`).

### 3. Owner interaction (Basecamp app)

The same owner channel is surfaced as a Basecamp mini-app (`app/`, package
`agent_owner`) so the owner can act from any Logos app instance holding their
keys — no server. It shows the agent's status and skills, lists pending spend
requests with approve / deny controls, and lets the owner raise the per-tx and
per-period limits at runtime.

The owner app holds the **owner end** of the owner channel directly: it loads
the Rust core (`liblogos_agent.so`) and opens the channel over Logos Messaging
(Waku) — the same two topics the agent derives — so approve/deny and limit
changes go straight to the agent over messaging, with no intermediary server.
The agent side runs in the headless `agent` binary. Configure the owner app with
the agent it owns and the messaging node both sides share:

```bash
export LOGOS_AGENT_FFI_PATH=/abs/path/to/liblogos_agent.so
export LOGOS_AGENT_ACCOUNT_ID=<the agent's shielded account id>
export LOGOS_AGENT_OWNER_ID=<owner-identity>
export AGENT_MESSAGING_URL=http://127.0.0.1:8645   # the same Waku REST node the agent uses
```

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

The owner-channel Rust FFI (`logos_agent_owner_channel_new` /
`_poll` / `_decide` / `_configure_limit` / `_configure_period` / `_free` in
`src/ffi.rs`) is unit-tested in-process and verified across the C ABI; the QML
approve/deny UI builds with the Logos module builder. A runtime approve/deny
pass against a live agent + Waku node is still needed for full evidence.

### 4. End-to-end demo

`scripts/demo.sh` runs the full flow against a local sequencer with real proofs
(`RISC0_DEV_MODE=0`) — the on-screen proof generation is the evidence dev mode is
off. See the script header for what it exercises.

## Compute-unit costs

On-chain operation costs are measured, not estimated: a token transfer (the
agent's spend and A2A payment path) is **127,726 compute units**, ~0.4% of LEZ's
32M public-execution budget; a mint is 116,862 CU. Full table and the
measurement method are in [`docs/CU_COSTS.md`](docs/CU_COSTS.md).

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
| `owner_ffi_e2e` | the same over the FFI owner handle (the Basecamp app boundary), with on-chain execution |
| `a2a_two_agents` | discovery, task lifecycle, autonomous LEZ payment |
| `three_use_cases_local` | the three use cases end-to-end (vault, notary, paid task) |
| `three_category_agents` | three agents, one per skill category, each with its own identity |
| `owner_ffi_waku` | the FFI owner run with real Waku as the transport (agent and owner each run their own nwaku client), spend still executing on-chain; ignored, needs an nwaku node |

Further suites are ignored by default because they need external services:
`three_use_cases` (Codex/nwaku, or `SERVICE_BACKEND=memory` for public-testnet
anchors) and the `*_live` tests (running Codex/Waku nodes, including
`owner_ffi_waku`, which needs an nwaku node on `127.0.0.1:8645` with
`--cluster-id=2` plus the local sequencer). See
`docs/THREE_USE_CASES.md`.

Public-testnet suites (also ignored, pointed at
`https://testnet.lez.logos.co` by default): `three_testnet_agents` gives each
category agent an included mint to its own account,
`three_testnet_settlements` deploys a program and settles once per agent
through it, and `testnet_tx` mints and transfers `TESTNET-COIN`. The evidence
they produced — verified transaction hashes, blocks, and the diagnosis of why
the sequencer drops token transfers — is in
[`docs/TESTNET_EVIDENCE.md`](docs/TESTNET_EVIDENCE.md),
[`docs/THREE_TESTNET_AGENTS.md`](docs/THREE_TESTNET_AGENTS.md), and
[`docs/THREE_TESTNET_SETTLEMENTS.md`](docs/THREE_TESTNET_SETTLEMENTS.md).

## License

Dual-licensed under MIT or Apache-2.0.
