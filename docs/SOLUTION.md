# Solution: LP-0008: Autonomous AI Agent Module

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
identity, private transactions, and proving come from the platform itself, and
the agent is indistinguishable on-chain from any other holder.

## Repository

- **Repo:** https://github.com/aegonmyy/logos-agent
- **License:** MIT or Apache-2.0
- **Demo:** `scripts/demo.sh` (runs against a local sequencer at `RISC0_DEV_MODE=0`)
- **Demo videos:** narrated CLI walkthrough (real proofs, `RISC0_DEV_MODE=0`):
  <https://youtu.be/HS6i3ucrKeE>; narrated three-use-case demo, builder's
  voice, cut from the 58-minute real-proof capture:
  <https://youtu.be/dg6RuNw44a8>; Basecamp GUI walkthrough:
  <https://youtu.be/1Ck_0keFXek>. Raw real-proof terminal recordings under
  `recordings/`. Reviewer tip: turn on YouTube subtitles (CC) for the
  narrated videos; the GUI demo's captions are burned into the picture.

## Approach

- **Identity + wallet** (`Agent`): a shielded LEZ account with a per-transaction
  and per-period spending policy. `send` executes autonomously within both
  limits and returns `NeedsOwnerApproval` over either; the period accumulator is
  persisted so restarts cannot reset it.
- **Skill interface** (`Skill` / `SkillRegistry`): capabilities are plug-ins
  invoked by name with JSON arguments; new skills register without touching the
  core. Default skills cover Storage, Messaging, Blockchain, and reflective
  `meta.*`.
- **Storage / Messaging**: `Storage` and `Messaging` traits with real backends
  (`CodexStorage`, client-side AES-256-GCM before upload; `WakuMessaging`,
  nwaku REST) plus in-memory backends for deterministic tests.
- **Owner control** (`OwnerChannel` / `AgentRuntime`): an encrypted two-way
  channel; the approval workflow holds over-limit spends until the owner decides,
  and the owner can reconfigure the limit at runtime.
- **A2A coordination** (`a2a`): A2A-schema Agent Cards published to a discovery
  topic, the A2A task lifecycle over Logos Messaging as transport, and LEZ
  payment per task, filling A2A's payment and transport gaps. Each card is
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
      intermediary server: the Basecamp `ui_qml` module (`app/`,
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
      transport**: the agent and the FFI owner handle each run their own client
      against a live nwaku node (Logos Dev Network, cluster 2) and never share
      memory, the approved spend still executing on-chain (balance 100→50). The
      remaining piece is a click-through of the QML UI; the FFI + Waku messaging
      path is proven end to end.
- [x] All default skills implemented: Storage (4), Messaging (3), Blockchain
      (`wallet.balance/send/history`, `program.query/call/deploy`), Meta
      (`meta.skills/status/configure`), and the five `agent.*` A2A skills.
- [x] At least 3 use cases and three category agents demonstrated end-to-end.
      The three use cases (personal file vault, privacy-preserving notary, paid
      multi-agent task) run reproducibly against a real local LEZ sequencer in
      `tests/three_use_cases_local.rs` (non-ignored; runs in CI and
      `scripts/demo.sh`): the vault and notary round-trip over storage +
      messaging, and the paid task settles a real on-chain LEZ transfer
      (payment tx `3d065660…`, block 17 of the run's sequencer, balances
      90/10; full real-proof capture at `RISC0_DEV_MODE=0` in
      `recordings/vault-notary-real-proof.cast`, passed in 3473 s). The vault,
      notary, and event-alerter use cases are additionally anchored on the
      public LEZ testnet with included mint transactions at blocks 402–404 and
      633, and at blocks 747–749 (2026-09-09, plus 405 and 407) with real
      Codex storage and real Waku messaging in the loop
      (`tests/three_use_cases.rs`). Three category
      agents hold their own verified on-chain mints (blocks 317, 338, 458).
      See `docs/THREE_USE_CASES.md`, `docs/TESTNET_EVIDENCE.md`,
      `docs/THREE_TESTNET_AGENTS.md`. Honest limitation: token `Send`
      inclusion on the public testnet varies by window (five payments in a
      four-hour window on 2026-09-08/09 were accepted and never included,
      listed in `docs/TESTNET_EVIDENCE.md`), so the public-testnet
      send/settlement evidence is carried by the agent-driven shielded spend
      (blocks 87–118) and the program-mediated settlements (blocks 148–150),
      and the A2A payment itself is evidenced on the local standalone
      sequencer.

### Usability

- [x] Documented skill interface (SDK) for adding skills without modifying the core.
- [x] Owner-facing interface inside the Logos app (Basecamp): the `agent_owner`
      `ui_qml` module builds to a loadable plugin + QML assets, and
      `scripts/package-basecamp.sh` produces standalone, side-loadable `.lgx`
      bundles (`agent.lgx`, `agent_owner.lgx`) as separate downloadables.

### Reliability

- [x] Above-threshold spends that are not approved are never executed.
- [x] Skill failures are isolated: a failing skill returns an error and does not
      crash the agent or other skills; A2A surfaces it as a `failed` task. Proven
      by a dedicated test (`a2a::tests::failing_skill_is_isolated_and_does_not_affect_other_tasks`): a failing
      skill and a working skill served in the same round; the failing task
      surfaces as `failed` with its error, the neighbouring task completes.
- [x] Recovers pending approvals across restarts: the runtime persists pending
      spends to disk (`AgentRuntime::with_state`) and restores them on start, and
      the deployed `agent` binary uses this by default (`--state-file`). A2A task
      state is likewise persisted (`A2aClient::with_state`). An owner-notification
      that cannot be delivered is retried and the spend is not held or executed.

### Performance

- [x] CU cost of on-chain operations documented from measurement (`docs/CU_COSTS.md`):
      a token transfer is 127,726 user-cycles (~0.4% of the 32M public budget).

### Supportability

- [x] End-to-end integration tests run against a LEZ sequencer (standalone) and
      are included in CI: the `e2e` job runs `agent_spending`,
      `owner_approval_flow`, `owner_ffi_e2e`, `a2a_two_agents`,
      `three_use_cases_local`, and `three_category_agents` against a local
      sequencer brought up via Docker on every push.
- [x] Reproducible demo script runs against a real local sequencer with
      `RISC0_DEV_MODE=0`; the curated path includes the three use cases, and
      the real-proof run is recorded in `docs/DEV_MODE_0_EVIDENCE.md`.
- [x] README documents end-to-end usage and deployment (CLI + Basecamp owner
      walkthrough).
- [x] CI green on the default branch: both lanes pass on `main`
      (GitHub Actions run 34293206202 on commit `a906e22`, 2026-09-09,
      self-hosted runner; the fast + e2e jobs run on every push).
- [x] Recorded demo showing terminal output including proof generation at
      `RISC0_DEV_MODE=0`: the narrated CLI walkthrough
      (<https://youtu.be/HS6i3ucrKeE>) shows dev-mode off, Groth16 proof
      generation, and the settled transactions; the raw cast is
      `recordings/logos-agent-real-proof.cast`. Turn on YouTube subtitles
      (CC) for the narration. The narrated three-use-case walkthrough
      (<https://youtu.be/dg6RuNw44a8>), cut from the 58-minute real-proof
      capture (`recordings/vault-notary-real-proof.cast`), covers the
      personal file vault, the privacy-preserving notary, and the paid
      multi-agent task in the builder's own voice; turn on YouTube
      subtitles (CC) for its narration as well.

> **Testnet evidence:** the agent has real, proof-backed activity on the
> **official public LEZ testnet** (`testnet.lez.logos.co`, v0.2.4) at
> `RISC0_DEV_MODE=0`, all re-verified live on 2026-09-09: shielded mint and
> spend from the agent's own account (blocks 87–118, four `PrivacyPreserving`
> transactions of 270–273 KB), a program deployment (block 144), three
> program-mediated settlements (blocks 148–150), three category-agent mints
> (blocks 317, 338, 458), and use-case anchors with real Codex and Waku in the
> loop (blocks 405–407). See **`docs/TESTNET_EVIDENCE.md`** for the full map,
> hashes, and verify commands. The multi-agent and multi-use-case flows are
> additionally shown against a real local sequencer at `RISC0_DEV_MODE=0`
> (`docs/DEV_MODE_0_EVIDENCE.md`, `docs/THREE_USE_CASES.md`), since
> public-testnet state is reset on operator redeploys.

## FURPS Self-Assessment

### Functionality

The agent is complete and proven: a shielded identity with policy-gated spending
(per-transaction and per-period), the full default skill set across Storage,
Messaging, Blockchain, and Meta, an owner-approval workflow, and A2A coordination
that discovers peers and settles LEZ payment per task autonomously. It is packaged
as a Logos Core module and paired with a Basecamp owner app, both of which build
against the Logos Core SDK.

### Usability

Skills are added by implementing one trait and registering it, with no core changes,
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
(wired into CI as a dedicated `e2e` job), a reproducible demo script that runs at
`RISC0_DEV_MODE=0`, documented CU costs, a README, and retained evidence of the
real-proof run. CI is green on the default branch (run 34293206202 on
`a906e22`).

## Terms & Conditions

I agree to the Terms & Conditions in TERMS.md. This submission is original work,
licensed under MIT or Apache-2.0.
