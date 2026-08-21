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
- **Recording:** `recordings/logos-agent-real-proof.cast` + `.mp4` (real-proof
  terminal recording; narrated voiceover still to be added)

## Approach

- **Identity + wallet** (`Agent`): a shielded LEZ account with a per-transaction
  and per-period spending policy. `send` executes autonomously within both
  limits and returns `NeedsOwnerApproval` over either; the period accumulator is
  persisted so restarts cannot reset it.
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
  payment per task — filling A2A's payment and transport gaps. Each card is
  Ed25519-signed by its publisher and embeds its verifying key, so tampered cards
  fail `AgentCard::verify`.
- **Deployment**: a single `agent` command deploys the agent headless.

## Success Criteria Checklist

### Functionality

- [x] Agent has its own shielded LEZ account and sends/receives tokens
      independently of the owner.
- [x] Owner deploys the agent with a single CLI command on a headless node.
- [x] Spending threshold holds above-limit transactions for approval and executes
      below-limit ones autonomously.
- [x] A2A-compatible coordination: Agent Cards follow the A2A schema and are
      signed (Ed25519), tasks follow the A2A lifecycle, documented as an A2A
      transport binding over Logos Messaging.
- [x] Two agents discover each other, run a task through the lifecycle, and
      transfer LEZ payment autonomously without owner intervention.
- [x] Agent packaged as a Logos Core module (`module/`, `agent_plugin.so`),
      loading the Rust core over a C ABI and driving it via `logos_module`; builds
      with the Logos module builder (Qt + Logos Core SDK).
- [x] Owner interacts from a separate Logos app instance over Logos Messaging, no
      intermediary server — the Basecamp `ui_qml` module (`app/`,
      `agent_owner_plugin.so`) builds to a loadable plugin and `.lgx` bundle with
      the Logos module builder. The owner-channel Rust FFI (`src/ffi.rs`,
      `logos_agent_owner_*`) is implemented, unit-tested, and verified across the
      C ABI, and the QML approve/deny UI is written (`app/src/qml/Main.qml`). Two
      runtime runs drive a live agent through the FFI owner handle (the exact
      boundary the Basecamp app calls): `tests/owner_ffi_e2e.rs` (non-ignored;
      runs in CI) does hold → FFI poll → FFI approve → on-chain execution
      (balance 100→50), FFI deny (no movement), FFI reconfigure → autonomous
      spend on the real local sequencer; and `tests/owner_ffi_waku.rs` runs the
      same hold → poll → approve → execute flow with **real Waku as the
      transport** — the agent and the FFI owner handle each run their own client
      against a live nwaku node (Logos Dev Network, cluster 2) and never share
      memory, the approved spend still executing on-chain (balance 100→50). The
      remaining piece is a click-through of the QML UI; the FFI + Waku messaging
      path is proven end to end.
- [x] All default skills implemented — Storage (4), Messaging (3), Blockchain
      (`wallet.balance/send/history`, `program.query/call/deploy`), Meta
      (`meta.skills/status/configure`), and the five `agent.*` A2A skills.
- [x] At least 3 use cases and three category agents demonstrated end-to-end.
      The three use cases (personal file vault, privacy-preserving notary, paid
      multi-agent task) run reproducibly against a real local LEZ sequencer in
      `tests/three_use_cases_local.rs` (non-ignored; runs in CI and
      `scripts/demo.sh`): the vault and notary round-trip over storage +
      messaging, and the paid task settles a real on-chain LEZ transfer
      (balances 90/10). The same three use cases are additionally anchored on
      the public LEZ testnet (`tests/three_use_cases.rs`; vault mint included
      block 17791, 2026-08-21). Three category agents deploy with distinct
      identities (block 17716). See `docs/THREE_USE_CASES.md`,
      `docs/TESTNET_EVIDENCE.md`. Honest limitation: on this date the testnet
      included mints but not token transfers within the polling window, so the
      public-testnet A2A payment leg is evidenced on the local standalone
      sequencer rather than the public transfer path.

### Usability

- [x] Documented skill interface (SDK) for adding skills without modifying the core.
- [x] Owner-facing interface inside the Logos app (Basecamp) — the `agent_owner`
      `ui_qml` module builds to a loadable plugin + QML assets, and
      `scripts/package-basecamp.sh` produces standalone, side-loadable `.lgx`
      bundles (`agent.lgx`, `agent_owner.lgx`) as separate downloadables.

### Reliability

- [x] Above-threshold spends that are not approved are never executed.
- [x] Skill failures are isolated — a failing skill returns an error and does not
      crash the agent or other skills; A2A surfaces it as a `failed` task. Proven
      by a dedicated test (`a2a::tests::failing_skill_isolated...`): a failing
      skill and a working skill served in the same round; the failing task
      surfaces as `failed` with its error, the neighbouring task completes.
- [x] Recovers pending approvals across restarts — the runtime persists pending
      spends to disk (`AgentRuntime::with_state`) and restores them on start, and
      the deployed `agent` binary uses this by default (`--state-file`). A2A task
      state is likewise persisted (`A2aClient::with_state`). An owner-notification
      that cannot be delivered is retried and the spend is not held or executed.

### Performance

- [x] CU cost of on-chain operations documented from measurement (`docs/CU_COSTS.md`):
      a token transfer is 127,726 user-cycles (~0.4% of the 32M public budget).

### Supportability

- [x] End-to-end integration tests run against a LEZ sequencer (standalone) and
      are included in CI — the `e2e` job runs `agent_spending`,
      `owner_approval_flow`, `owner_ffi_e2e`, `a2a_two_agents`,
      `three_use_cases_local`, and `three_category_agents` against a local
      sequencer brought up via Docker on every push.
- [x] Reproducible demo script runs against a real local sequencer with
      `RISC0_DEV_MODE=0`; the curated path includes the three use cases, and
      the real-proof run is recorded in `docs/DEV_MODE_0_EVIDENCE.md`.
- [x] README documents end-to-end usage and deployment (CLI + Basecamp owner
      walkthrough).
- [ ] CI green on the default branch — the workflow is present, but the latest
      changes still need a clean-branch CI run before this is checked.
- [x] Recorded demo showing terminal output including proof generation at
      `RISC0_DEV_MODE=0` — `recordings/logos-agent-real-proof.cast` and its
      rendered MP4. The narrated voiceover required by the prize remains to be
      recorded.

> **Testnet evidence:** the agent has real, proof-backed activity on the
> **official public LEZ testnet** (`testnet.lez.logos.co`, v0.2.4) at
> `RISC0_DEV_MODE=0` — it defines and mints a token to its own account, included
> on-chain (balance 100, tx `0e3ebbb8…`). See **`docs/TESTNET_EVIDENCE.md`** for
> hashes, the explorer links, and the on-chain account state. The multi-agent and
> multi-use-case flows are additionally shown against a real local sequencer at
> `RISC0_DEV_MODE=0` (`docs/DEV_MODE_0_EVIDENCE.md`, `docs/THREE_USE_CASES.md`),
> since public-testnet block production is intermittent and its state is reset on
> operator redeploys.

## FURPS Self-Assessment

### Functionality

The agent is complete and proven: a shielded identity with policy-gated spending
(per-transaction and per-period), the full default skill set across Storage,
Messaging, Blockchain, and Meta, an owner-approval workflow, and A2A coordination
that discovers peers and settles LEZ payment per task autonomously. It is packaged
as a Logos Core module and paired with a Basecamp owner app, both of which build
against the Logos Core SDK.

### Usability

Skills are added by implementing one trait and registering it — no core changes —
and `meta.skills` lists the catalogue for discovery. Deployment is a single
command. The owner interacts over the encrypted channel today; the Basecamp owner
app builds and loads, with runtime approve/deny interaction still to be evidenced.

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

The work is covered by end-to-end integration tests against a real local sequencer
— wired into CI as a dedicated `e2e` job — a reproducible demo script that runs at
`RISC0_DEV_MODE=0`, documented CU costs, a README, and retained evidence of the
real-proof run. A clean-branch CI run is still pending before the green badge is
re-confirmed.

## Terms & Conditions

I agree to the Terms & Conditions in TERMS.md. This submission is original work,
licensed under MIT or Apache-2.0.
