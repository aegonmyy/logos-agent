# Three LP-0008 Use Cases

`tests/three_use_cases.rs` provides reproducible workflows that map to the
LP-0008 illustrative use cases: the personal file vault, the
privacy-preserving notary, and the paid skill marketplace, plus the on-chain
event alerter exercised in the public-anchor lane. The first two use the real
Codex and nwaku REST adapters when configured. The third uses the A2A
provider/client path and performs a paid task on a real LEZ sequencer.

## Workflows

### Personal File Vault

The agent encrypts a document client-side, uploads it to Codex, sends the returned
CID over Logos Messaging, downloads the object, and verifies the original bytes.

### Privacy-Preserving Notary

The agent encrypts and uploads a document, computes its SHA-256 digest, sends the
CID and digest over Messaging, then downloads the document and verifies the digest.
The CID and digest are the private storage proof; a public LEZ transaction can be
added by a deployment-specific notary program when one is configured.

### Paid Multi-Agent Task

The provider publishes an A2A Agent Card, the client discovers it, submits a task,
the provider executes the advertised skill, the client pays the declared price in
LEZ, and the provider's balance is verified. The payment settles through the
wallet as a proof-bearing shielded transaction.

The reproducible A2A payment proof runs on a real local LEZ sequencer brought up
via Docker, with `RISC0_DEV_MODE=0` (real proofs), verifying the client/provider
token balances after payment. Public-testnet `Send` inclusion varies by window
(see [`TESTNET_EVIDENCE.md`](TESTNET_EVIDENCE.md)); the paid lane therefore runs
against the sequencer it controls end to end.

## Run

### Reproducible, no external services (local sequencer)

`tests/three_use_cases_local.rs` runs all three use cases against a real local
LEZ sequencer brought up via Docker (`TestContext`); it is non-ignored, so it runs in
CI and in `scripts/demo.sh`:

```bash
RISC0_DEV_MODE=0 cargo test --test three_use_cases_local -- --nocapture --test-threads=1
```

### With real services (Codex + nwaku)

Start Logos Storage v0.3.0 and nwaku, then run:

```bash
RISC0_DEV_MODE=0 \
  AGENT_CODEX_URL=http://127.0.0.1:8080 \
  AGENT_MESSAGING_URL=http://127.0.0.1:8645 \
  cargo test --test three_use_cases -- --ignored --nocapture
```

### Anchored on the public testnet

```bash
RISC0_DEV_MODE=0 SERVICE_BACKEND=memory \
  cargo test --test three_use_cases -- three_lp0008_use_cases --ignored --nocapture --test-threads=1
```

The storage/messaging round-trips run over in-memory backends while every
on-chain anchor is a live public-testnet transaction (a per-use-case token
mint for the vault, notary, and event-alerter use cases). To run the paid
lane instead, set `RUN_PAID_A2A=1` **without** `SERVICE_BACKEND`: the test
checks `SERVICE_BACKEND=memory` first, so setting both re-runs the anchor
lane rather than the paid one. The paid lane self-funds (it mints the payer
supply first) and sends the payment with a bounded retry.

The test prints CIDs, Messaging topics, SHA-256 digests, task IDs, provider
accounts, and final task states. It must not be described as
public-testnet payment evidence unless its output includes a successful LEZ
transaction and block reference.

Public LEZ transaction inclusion is polled by transaction hash with periodic
progress output; a mint that is submitted but not included within the bounded
window is reported as `submitted_pending` rather than failing the use case. A
newly observed block is not treated as proof that the requested transaction was
included.

## Evidence Standard

For a final LP-0008 submission, preserve the complete test output and record:

- Codex CIDs and verified downloaded-file digests.
- Messaging topics and message IDs or the documented standalone-node fallback.
- A2A Agent Card, discovery topic, task ID, lifecycle states, payment transaction,
  and LEZ block.
- The exact public LEZ endpoint and service versions used.

## A2A Real-Proof Evidence

Run the paid A2A criterion independently from the public service workflow:

```bash
RISC0_DEV_MODE=0 cargo test --test a2a_two_agents \
  two_agents_discover_run_task_and_settle_payment -- --nocapture
```

Verified locally against the standalone LEZ sequencer:

```text
test two_agents_discover_run_task_and_settle_payment ... ok
test result: ok. 1 passed; 0 failed
finished in 152.43s
```

The test verifies Agent Card publication and discovery, a real LEZ payment,
`submitted` and `completed` task states, the returned task result, and client and
provider balances of 90 and 10 respectively. The proof run used
`RISC0_DEV_MODE=0`.

The full three-use-case flow was additionally recorded end to end at
`RISC0_DEV_MODE=0` on 2026-09-08 (`recordings/vault-notary-real-proof.cast`,
test passed in 3473.40 s). The payment settled on chain as transaction
`3d065660d4f7d97ddaae0f4f78bff0d59f5995c32377ad0dfa35c5e199456c96`,
included in **block 17** of the run's sequencer, with the closing line:

```text
use_case=paid_multi_agent_task task_id=task-0 provider=EE72BU1gSTxRzhPLfyUzg7o43iTfhhVN2daTZoafehjb \
  token=Cvp1AYmcxjxjwiWp5rooP13ZrjVMPQ1f4JtsaZMwFpEk client_balance=90 provider_balance=10 state=completed
```

## Public-Testnet Anchors (re-verified live 2026-09-09)

The vault, notary, and event-alerter use cases anchor on the public testnet
with included mint transactions. The primary set below was produced on
2026-09-09 with real Codex storage and real Waku messaging in the loop. The
notary digest is
`5999d285f64e95b7d4f1246a112d45535e852676d06f14530900876f4638a42a` in every
run, including the real-service runs where the document round-tripped through
real Codex and real Waku.

| Use case | Run | Transaction | Block | Artifact |
|---|---|---|---|---|
| Personal file vault | **real Codex + real Waku, 2026-09-09** | `54bfd294619753f13edcfd136694e7530e6afe9b2d7130f26e677b29e87fb35d` | 747 | real Codex CID `zDvZRwzm4zQmJH7UbbL7MAUJDhhmy2ZwqaT7nGgedzJxhfCyXS7y` |
| Privacy-preserving notary | **real Codex + real Waku, 2026-09-09** | `5a90dd7740cecec6a6bd35738d02d913426796bd5cd1acdb5741fdbf8bc9f966` | 748 | real Codex CID `zDvZRwzkxZxD2q6h9Tv5isQuMaJWJYMQQFoD2d29NQEMVGTx9SZgW`, digest as above |
| On-chain event alerter | **real Codex + real Waku, 2026-09-09** | `c9d8debda7f930354d3d1f1e764a3880277581123aa68056383d11933a0c2200` | 749 | anchor only |
| Personal file vault | memory backends, 2026-09-08 | `0dcc608725d948f527484440509551835d11793c4e1729cf4445176e4ab45983` | 402 | CID `1bf2de773802172a6cc6407ded7ea37d57aab1614705a10ca8aff56eff0017fe` |
| Privacy-preserving notary | memory backends, 2026-09-08 | `72ae9ae11ccf12aa89abdc26a4865480fb147ce6d9a6f38bc648103f7a4ade2f` | 403 | CID `676b4700e4656c6cdfd66ffa948b294fb490057cc89f15f7fc828cbc381b1d75`, digest as above |
| On-chain event alerter | memory backends, 2026-09-08 | `51979b2f432c27da9699a125ef48fd2430b2edb996f742bf9b5b2be86f053b91` | 404 | anchor only |
| Personal file vault | real services, 2026-09-08 | `a9dd993018a1caad6cb8487dddd56827c8591303646bbc2d7db86051afd63794` | 405 | real Codex CID `zDvZRwzm2qMNXP9eenAGVE51uidLTvHWh2E3CRX3MbcYC4vps5Y8` |
| On-chain event alerter | real services, 2026-09-08 | `58eeed1c42c878cac07f10a53a03c49dd05dc37d0569db9b3043adaa74ccfedc` | 407 | anchor only |
| Personal file vault | memory backends re-run, 2026-09-09 | `bb30088704c420b7b697a0c51b1bc3df25833cfdfb18a3168722d308f11b2ec5` | 633 | CID `80fea81dcfa0c1ec948ab5d7c223a83e067805cfd453237b749fed2af0e2f95e` |

Earlier anchors at blocks 406, 634, and 635 verified live when they landed and
later returned `null` when the operator's node served different content for
those heights; the table cites only hashes that verify live as of 2026-09-09,
and the reproducible run regenerates the evidence at will.

Verify any of them:

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":["54bfd294619753f13edcfd136694e7530e6afe9b2d7130f26e677b29e87fb35d"]}'
```

## Payment on the Public Testnet

The paid lane's on-testnet payment leg is window-dependent: five payment
transactions submitted across a four-hour window on 2026-09-08/09 were
accepted with a hash and never included, while sibling mints from the same
runs included within seconds. The full list and analysis are in
[`TESTNET_EVIDENCE.md`](TESTNET_EVIDENCE.md). The payment itself is therefore
evidenced on the local sequencer with real proofs (section above), and the
public-testnet send/settlement evidence is carried by the agent-driven
shielded spend and the program-mediated settlements
([`SHIELDED_TESTNET_PROOF.md`](SHIELDED_TESTNET_PROOF.md),
[`THREE_TESTNET_SETTLEMENTS.md`](THREE_TESTNET_SETTLEMENTS.md)).
