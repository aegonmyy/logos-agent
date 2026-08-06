# Solution: LP-0008 — Autonomous AI Agent Module

**Submitted by:** aegonmyy

## Summary

A Logos-native autonomous agent: it holds its own shielded LEZ account, spends
under an owner-set policy, exposes composable skills for Storage, Messaging, and
Blockchain, and coordinates with other agents through an A2A-compatible protocol
that settles payment in LEZ. Small spends happen autonomously; larger ones are
held for the owner's approval over an encrypted Logos Messaging channel. The full
flow is proven end-to-end against a real local sequencer with `RISC0_DEV_MODE=0`
(real Groth16 proofs).

The agent is built directly on the Logos Execution Zone wallet, so its shielded
identity, private transactions, and proving are the platform's — not
reimplemented — and the agent is indistinguishable on-chain from any other holder.

## Repository

- **Repo:** https://github.com/aegonmyy/logos-agent
- **License:** MIT or Apache-2.0
- **Demo:** `scripts/demo.sh` (runs against a local sequencer at `RISC0_DEV_MODE=0`)
- **Video:** _(link to be added)_

## Approach

- **Identity + wallet** (`Agent`): a shielded LEZ account with a per-transaction
  spending policy. `send` executes autonomously under the limit and returns
  `NeedsOwnerApproval` over it.
- **Skill interface** (`Skill` / `SkillRegistry`): capabilities are plug-ins
  invoked by name with JSON arguments; new skills register without touching the
  core. Default skills cover Storage, Messaging, Blockchain, and reflective
  `meta.*`.
- **Storage / Messaging**: `Storage` and `Messaging` traits with real backends —
  `CodexStorage` (client-side AES-256-GCM before upload) and `WakuMessaging`
  (nwaku REST) — plus in-memory backends for deterministic tests.
- **Owner control** (`OwnerChannel` / `AgentRuntime`): an encrypted two-way
  channel; the approval workflow holds over-limit spends until the owner decides,
  and the owner can reconfigure the limit at runtime.
- **A2A coordination** (`a2a`): A2A-schema Agent Cards published to a discovery
  topic, the A2A task lifecycle over Logos Messaging as transport, and LEZ
  payment per task — filling A2A's payment and transport gaps.
- **Deployment**: a single `agent` command deploys the agent headless.

## Success Criteria Checklist

### Functionality

- [x] Agent has its own shielded LEZ account and sends/receives tokens
      independently of the owner.
- [x] Owner deploys the agent with a single CLI command on a headless node.
- [x] Spending threshold holds above-limit transactions for approval and executes
      below-limit ones autonomously.
- [x] A2A-compatible coordination: Agent Cards follow the A2A schema, tasks follow
      the A2A lifecycle, documented as an A2A transport binding over Logos Messaging.
- [x] Two agents discover each other, run a task through the lifecycle, and
      transfer LEZ payment autonomously without owner intervention.
- [x] Agent packaged as a Logos Core module (`module/`, `agent_plugin.so`),
      loading the Rust core over a C ABI and driving it via `logos_module`; builds
      with the Logos module builder (Qt + Logos Core SDK).
- [x] Owner interacts from a separate Logos app instance — a Basecamp `ui_qml`
      module (`app/`, `agent_owner_plugin.so`) talks to the agent module over
      RemoteObjects and shows status and skills.
- [x] All default skills implemented — Storage (4), Messaging (3), Blockchain
      (`wallet.balance/send/history`, `program.query/call/deploy`), Meta
      (`meta.skills/status/configure`), and the five `agent.*` A2A skills.
- [x] At least 3 use cases and three category agents demonstrated end-to-end —
      `tests/three_category_agents.rs` deploys one agent per category (Storage,
      Messaging, Blockchain) against the live local sequencer; the file-vault,
      marketplace, and on-chain use cases are covered. *Local sequencer, as the
      public testnet was reset for v0.2.0 (see note below).*

### Usability

- [x] Documented skill interface (SDK) for adding skills without modifying the core.
- [x] Owner-facing interface inside the Logos app (Basecamp) — the `agent_owner`
      `ui_qml` module builds to a loadable plugin + QML assets.

### Reliability

- [x] Above-threshold spends that are not approved are never executed.
- [x] Skill failures are isolated — a failing skill returns an error and does not
      crash the agent or other skills; A2A surfaces it as a `failed` task.
- [x] Recovers pending approvals across restarts — the runtime persists pending
      spends to disk (`AgentRuntime::with_state`) and restores them on start; an
      owner-notification that cannot be delivered is retried and the spend is not
      held or executed. *A2A task-state persistence is a further refinement.*

### Performance

- [x] CU cost of on-chain operations documented from measurement (`docs/CU_COSTS.md`):
      a token transfer is 127,726 user-cycles (~0.4% of the 32M public budget).

### Supportability

- [x] End-to-end integration tests run against a LEZ sequencer (in-process,
      standalone) covering identity, skills, approvals, and A2A payment.
- [x] Reproducible demo script runs against a real local sequencer with
      `RISC0_DEV_MODE=0`; the real-proof run is recorded in
      `docs/DEV_MODE_0_EVIDENCE.md`.
- [x] README documents end-to-end usage and deployment.
- [ ] CI green on the default branch. *CI workflow is included; a green run
      depends on the repository being stood up.*
- [ ] Recorded narrated video showing terminal output including proof generation.
      *To be recorded with `scripts/demo.sh`.*

> **Testnet note:** the public LEZ testnet was reset for v0.2.0. As with our
> LP-0017 submission, deployment and the demo are shown against a real local
> sequencer at `RISC0_DEV_MODE=0`; the deployment steps are identical for the
> public testnet once its endpoint is configured.

## FURPS Self-Assessment

### Functionality

The agent is complete and proven: a shielded identity with policy-gated spending,
the full default skill set across Storage, Messaging, Blockchain, and Meta, an
owner-approval workflow, and A2A coordination that discovers peers and settles LEZ
payment per task autonomously. It is packaged as a Logos Core module and paired
with a Basecamp owner app, both of which build against the Logos Core SDK.

### Usability

Skills are added by implementing one trait and registering it — no core changes —
and `meta.skills` lists the catalogue for discovery. Deployment is a single
command, and the owner interacts both over the encrypted channel and through the
Basecamp owner app.

### Reliability

Unapproved over-limit spends never execute, and skill failures are isolated rather
than fatal (A2A reports them as `failed`). Pending approvals are persisted and
restored across restarts, and an owner-notification that cannot be delivered is
retried before the spend is dropped.

### Performance

On-chain operation costs are measured, not estimated: a token transfer (the
agent's spend and A2A payment path) is 127,726 compute units, well within the 32M
public-execution budget. Real-proof generation for a private transaction is on the
order of minutes on commodity hardware.

### Supportability

The work is covered by end-to-end integration tests against a real local sequencer,
a reproducible demo script that runs at `RISC0_DEV_MODE=0`, documented CU costs,
a README, and a CI workflow. Retained evidence of the real-proof run is included.

## Terms & Conditions

I agree to the Terms & Conditions in TERMS.md. This submission is original work,
licensed under MIT or Apache-2.0.
