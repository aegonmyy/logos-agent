# Live LEZ testnet evidence

Real, proof-backed agent activity on the **official public LEZ testnet**
(`https://testnet.lez.logos.co`, LEZ **v0.2.4**), generated with
**`RISC0_DEV_MODE=0`**, so every transaction carries a real Groth16 proof.

The testnet's chain state is reset by its operators from time to time; a reset
on **2026-09-08** invalidated every hash cited before that date. This document
leads with the current chain epoch, and **every transaction below was
re-verified live over RPC on 2026-09-09**. Prior epochs are summarized at the
end and archived under [`testnet-evidence/`](../testnet-evidence/).

## Current epoch evidence map

| Evidence | Transactions | Blocks | Full detail |
|---|---|---|---|
| Agent receives into its own shielded account and spends autonomously | 4 × `PrivacyPreserving`, 270,480–273,066 B | 87–118 | [`SHIELDED_TESTNET_PROOF.md`](SHIELDED_TESTNET_PROOF.md) |
| Program deployment driven by the agent's `program.deploy` skill | 343,397 B `ProgramDeployment` | 144 | [`THREE_TESTNET_SETTLEMENTS.md`](THREE_TESTNET_SETTLEMENTS.md) |
| Three program-mediated settlements, one per category agent | 3 × 193 B | 148–150 | [`THREE_TESTNET_SETTLEMENTS.md`](THREE_TESTNET_SETTLEMENTS.md) |
| Three category-agent token mints (shielded, agent-held, balance 100) | see [`THREE_TESTNET_AGENTS.md`](THREE_TESTNET_AGENTS.md) | 317, 338, 458 | [`THREE_TESTNET_AGENTS.md`](THREE_TESTNET_AGENTS.md) |
| Use-case anchors with real Codex + real Waku services | vault / notary / alerter | 747–749 (also 405, 407) | [`THREE_USE_CASES.md`](THREE_USE_CASES.md) |
| Use-case anchors over memory backends | vault / notary / alerter | 402–404, 633 | [`THREE_USE_CASES.md`](THREE_USE_CASES.md) |
| Real-service evidence: Codex content address + Waku message id | n/a | 432 | [`THREE_TESTNET_AGENTS.md`](THREE_TESTNET_AGENTS.md) |

## Headline: agent mints to its own shielded account

The strongest single proof on the current chain (full-strength category-agent
run, completed 2026-09-09 00:01 after 11,753 s of real proving):

| | |
|---|---|
| Network | `https://testnet.lez.logos.co` (official), LEZ v0.2.4 |
| Proof mode | `RISC0_DEV_MODE=0` (real Groth16) |
| Test | `tests/three_testnet_agents.rs` |
| Agent account (holder) | `33hs4jhpyRtenW72TggmNFsALSwBWBp1KUZyS7HRWusd` (shielded) |
| Token | `BBbuRmpb5idxPfHnFjgEiLxZXrfex3YWgmzTzg1SJmz4`, supply 100 |
| Mint transaction | `9b510890642f95a10ac4b75ffa6a121c262c065343c44906297347f5ef4266ce` |
| Result | **`PrivacyPreserving`, 270,845-byte on-chain body, included in block 458; holder balance 100** |

Verify (the explorer does not index `PrivacyPreserving` transactions; the raw
RPC returns them):

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":["9b510890642f95a10ac4b75ffa6a121c262c065343c44906297347f5ef4266ce"]}' \
  | jq -r '.result[0]' | base64 -d | wc -c
# -> 270845
```

The size is the proof: a `Public` transaction (type `0x00`) is a few hundred
bytes; a `PrivacyPreserving` transaction (type `0x01`) carries the ZK proof in
its witness set and runs to hundreds of kilobytes.

## Transfers and settlements on the public testnet

Observed behavior of the current epoch, stated precisely:

- **Shielded transactions include.** The 2026-09-08 morning run landed a
  shielded mint (block 87) and three shielded spend transactions (blocks 88,
  111, 118), moving the agent's balance 100 → 90 with 10 at a private
  recipient; all re-verified live ([`SHIELDED_TESTNET_PROOF.md`](SHIELDED_TESTNET_PROOF.md)).
- **Public transactions include.** Account setup, program deployment
  (block 144), and program calls (blocks 148–150) all included within their
  polling windows.
- **Token `Send` inclusion varies by window.** During a four-hour window on
  2026-09-08/09 (20:19 through 00:53 UTC), five shielded token payments were
  accepted with a printed hash and never included. Every hash still returns
  `null` at re-check, while mints submitted by the very same runs included
  within seconds (payer-funding mints at blocks 408, 641, 670):

| Payment attempt | Transaction hash | Outcome at re-check |
|---|---|---|
| Lane B, run 1 | `e4f759ed8e9b5e338f585ba6ff8d7221f9b6fcd78e967db0cf93cce7f68089f1` | `null` |
| Lane B, retry 1 | `905a91c22b4dd0f6bc1920a1f3881e42281d180e8524ab0a18b5f4c9fe1501cd` | `null` |
| Lane B, retry 2, attempt 1 | `918735891baf43bc0ba32c790b69ffaa19bda1e5736bcbd63bec721ab62ef2e4` | `null` |
| Lane B, retry 2, attempt 2 | `68b9c1bea71a925a41b9ffdcc7c3a201d53ba010bd10d8a74fdcea4c048425ad` | `null` |
| Lane B, retry 2, attempt 3 | `e9cc8dc7bd04e7eeed2821aa7c4cb5dedf1c7d9b76714c98b19c5cc09713c01d` | `null` |

The wallet, proofs, and submission path were healthy throughout (sibling
transactions landed), so the discriminator was the transfer instruction in
that window, and the window is the variable: the morning of the same day,
shielded spends included. The harness therefore never treats submission as
inclusion: a transfer that is accepted but absent within its bounded poll is
reported as `submitted_not_included`, and the payment leg is retried with a
bounded timeout.

**Consequence for the evidence set:** the public-testnet send/settlement leg
is carried by the agent-driven shielded spend (blocks 87–118) and the three
program-mediated settlements (blocks 148–150, one claiming a shielded
account). The A2A payment criterion is evidenced on the local standalone
sequencer at `RISC0_DEV_MODE=0` (real proofs, client/provider balances 90/10;
see [`THREE_USE_CASES.md`](THREE_USE_CASES.md)). No document in this
repository claims a public-testnet A2A payment.

## Why this required matching the testnet version

Our client was originally pinned to LEZ **v0.2.0**. Against the v0.2.4 testnet it
could **sync** (read path is backward-compatible) but its transactions were
**accepted into the mempool yet never included**: the transaction/write path
changed across v0.2.1–v0.2.4 (L1 fees, private-kinds refactor). Bumping the
client to **v0.2.4** made the identical transaction include. The version match is
what turned "submitted" into "on-chain".

## Reproduce

```bash
# Real proofs, against the official testnet (the default endpoint).
RISC0_DEV_MODE=0 AGENT_TESTNET_URL=https://testnet.lez.logos.co \
  cargo test --test testnet_tx -- --ignored --nocapture --test-threads=1
```

The test creates fresh accounts, defines and mints `TESTNET-COIN` to the agent,
then transfers to a second account, each a real proof-backed transaction. It
reads chain state back to assert the balances, so a green run is itself proof of
inclusion. (Public-testnet block production is intermittent; the test retries
through transient outages. Inclusion of token `Send`s depends on the window, as
documented above; the transfer leg is bounded and reported as
`submitted_not_included` when the window withholds it.)

> **Note on ephemerality:** the public testnet's chain state is reset when the
> operator redeploys, and the node has also served different content for a few
> older heights after the fact (three of our 2026-09-08 anchors at blocks 406,
> 634, and 635 verified live when landed and returned `null` later, while every
> neighbouring hash stayed queryable). Transaction hashes and account pages
> above are valid for the deployment they were produced against; the
> reproducible tests regenerate equivalent evidence against whatever testnet is
> live, which is exactly how the current set was produced after the 2026-09-08
> reset.

## Prior epochs (superseded)

Kept for the record; all hashes below are dead on the live chain.

- **v0.1.0 epoch** (pre-2026-09-08): agent mint at block 984
  (`0e3ebbb8bb03ba31d3c1115aca438bbe8835ada9c84aa03e785f16bc6f67fa6e`); public
  mint `2a57150d…` (block 42680) and shielded send `606bb9b5…` (block 42709).
  Snapshots archived at [`testnet-evidence/v0.1.0/`](../testnet-evidence/v0.1.0).
- **2026-08-18/22 epoch**: category-agent identities (block 12902), use-case
  mint anchors (blocks 17716, 17791–17793), program-mediated settlements
  (blocks 18599–18601). The 2026-08-22 "private transactions silently dropped"
  diagnosis held for that epoch and was later resolved; on the current epoch
  shielded transactions include (blocks 87–118) while token `Send` inclusion
  varies by window, as documented above.
