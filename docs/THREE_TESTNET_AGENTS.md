# Three Public-Testnet Agents

This document is the reproducible evidence procedure for LP-0008's requirement
for three independent agents on LEZ testnet: Storage, Messaging, and Blockchain.
Unlike `docs/THREE_AGENTS.md`, this procedure does not use the local Docker LEZ
fixture.

## Prerequisites

The test self-funds: it creates fresh accounts, defines a token (supply 100)
per agent, and mints to each agent's own account, so no pre-funded wallet is
needed. The endpoint defaults to:

```text
https://testnet.lez.logos.co
```

For the real Logos services, set:

```bash
export AGENT_MESSAGING_URL=http://127.0.0.1:8645
export AGENT_CODEX_URL=http://127.0.0.1:8080
```

The service endpoints are optional for identity-only evidence, but are required
to exercise the Storage and Messaging categories against real backends.

## Run

Run with real proofs against the public testnet:

```bash
RISC0_DEV_MODE=0 \
  cargo test --test three_testnet_agents \
  -- --ignored --nocapture --test-threads=1
```

To select another sequencer:

```bash
AGENT_TESTNET_URL=https://testnet.lez.logos.co \
  RISC0_DEV_MODE=0 \
  cargo test --test three_testnet_agents \
  -- --ignored --nocapture --test-threads=1
```

## Expected Evidence

The test prints three distinct account identities:

```text
testnet agent category=storage account=<account-id>
testnet agent category=messaging account=<account-id>
testnet agent category=blockchain account=<account-id>
```

It also prints the public testnet block used for the run, and per-agent mint
state (`included_block=<n> balance=100` on inclusion, `submitted_pending`
when the bounded mint wait expires). When the real service endpoints are
configured, it prints a Waku message id and a Codex content address. Copy the
complete output into this document or a dated evidence file, together with
the testnet transaction hashes and explorer links returned by the wallet.

## Verified Runs (2026-09-08/09, current chain epoch, re-verified live 2026-09-09)

The public testnet was reset by its operators on 2026-09-08; the runs below
re-landed the evidence on the fresh chain. Each category agent has an
**included, verified on-chain token mint** it holds itself, at `RISC0_DEV_MODE=0`.
The storage row comes from the full-strength run (real Codex + real Waku,
11,753 s of real proving); the messaging and blockchain rows come from the
same-day re-land run.

| Category | Agent account | Token | Mint transaction | Block | State |
|---|---|---|---|---|---|
| storage | `33hs4jhpyRtenW72TggmNFsALSwBWBp1KUZyS7HRWusd` | `BBbuRmpb5idxPfHnFjgEiLxZXrfex3YWgmzTzg1SJmz4` | `9b510890642f95a10ac4b75ffa6a121c262c065343c44906297347f5ef4266ce` | **458** | `PrivacyPreserving`, 270,845 B, balance 100 |
| messaging | `5t9amMTzqMtyJUgbR6TeiXkAf4nuNNpvu2a9kGy99ADr` | `GCC4uhLA66V4amdonQd8gD2uZJDAMequTCV5tgCGUXNd` | `328d26a709447c40805fa64eba407a2aa2d91473068434351617a519cb62732e` | **317** | included, balance 100 |
| blockchain | `GHJcKi85GywQvH2gKevuJYQ98XGnxG8imb9EJkZffBGq` | `CW5wW7aSigdPNpa4EeV1NXPKRmdeB1MopnPeLZNJ3B8s` | `5fc57a20966a637f860e3139dbd2003f9f35ba556d7974156cffc291e3cb560b` | **338** | included, balance 100 |

The full-strength run's messaging and blockchain mint attempts reported
`submitted_pending` when their inclusion waits hit parked-connection timeouts,
but the transactions broadcast and landed: oversized shielded blocks 463
(271,270 B), 508 (272,900 B), 511 (271,514 B), and 558 (544,688 B, two mints)
appear in the 459–560 range, corroborating the attempts. The verified rows
above remain the primary citations.

**Real-service evidence from the full-strength run:**

```text
messaging evidence topic=/logos-agent/testnet/evidence/3jPB2RCP7AX3VA9jUUmX51cJZJXPZFm6qTDh3bs8LRQK message_id=3bb660a10984114c4245be5e098d9ee1fdba0c74625f2c4f3298f2dc3ef22ad7
storage evidence address=zDvZRwzm9WDaKLBS2eg6DWCz4Ww22SbHYzuXFAfbLx4xyRVudMvE
testnet evidence block=432
```

The storage address is a real Codex content address (encrypted upload verified
by download), and the messaging message id is a real Waku message id read back
from the node's store.

## Per-agent on-chain settlements

Each of the three category agents also settles through the agent's own
deployed program on the public testnet: program deployment at block 144, then
per-agent `program.call` settlements at blocks **148–150**, every settlement's
account ownership change re-read from chain state. Full evidence and RPC
re-verification: [`THREE_TESTNET_SETTLEMENTS.md`](THREE_TESTNET_SETTLEMENTS.md).

## History (superseded epochs)

Kept for the record; all hashes below are dead on the live chain after the
2026-09-08 operator reset.

- **2026-08-18 run** (real services, `RISC0_DEV_MODE=0`): identities
  `CJ3u1hzC…` / `DM63x4x9…` / `G9sAkVHZ…`, evidence block 12902.
- **2026-08-21 run** (identity-only): identities `2vcrKfra…` /
  `2ne7UEvY…` / `HaGPWVZV…`, evidence block 17716.
- **2026-08-22 settlements**: blocks 18599–18601, superseded by the
  current-epoch settlements at 148–150.
- The 2026-08-22 observation that private-supply mints never include was
  epoch-specific: on the current chain the shielded mints above included
  (blocks 317, 338, 458).

## Current Repository State

The checked-in `tests/three_category_agents.rs` test is local-sequencer evidence
only. It must not be presented as public-testnet evidence. Public-testnet
evidence is this document (identities + per-agent mints) plus
[`THREE_TESTNET_SETTLEMENTS.md`](THREE_TESTNET_SETTLEMENTS.md) (per-agent
settled transactions), both reproducible via the ignored public-testnet suites.
