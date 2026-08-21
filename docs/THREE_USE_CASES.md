# Three LP-0008 Use Cases

`tests/three_use_cases.rs` provides three reproducible workflows that map to the
LP-0008 illustrative use cases. The first two use the real Codex and nwaku REST
adapters. The third uses the A2A provider/client path and performs a paid task when
the configured LEZ wallet can reach the testnet.

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
the provider executes the advertised skill, and the client observes the completed
result. The task price is declared in the card and is settled through the client
wallet when a funded testnet wallet is available.

The reproducible A2A payment proof is currently the local standalone-sequencer
run, not the flaky public payment path. It uses real proofs with
`RISC0_DEV_MODE=0` and verifies the client/provider token balances after payment.

## Run

### Reproducible, no external services (local sequencer)

`tests/three_use_cases_local.rs` runs all three use cases against a real local
LEZ sequencer brought up via Docker (`TestContext`) — non-ignored, so it runs in
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
mint).

The test prints CIDs, Messaging topics, SHA-256 digests, task IDs, provider
accounts, and final task states. Set `RUN_PAID_A2A=1` only with a funded,
correctly configured wallet; otherwise the paid workflow is reported as skipped
rather than silently claiming payment evidence. It must not be described as
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

## Re-verified 2026-08-21 (all three use cases anchored on-chain)

A `RISC0_DEV_MODE=0 SERVICE_BACKEND=memory` run against the public testnet
included all three use-case mint anchors:

| Use case | Mint transaction | Block |
|---|---|---|
| Personal file vault | `73d9dc70f12ab443bd0f458c6504f8392671f281b54f56913d710b67dd722039` | 17791 |
| Privacy-preserving notary | `9b70fb96de6dba41212d2f1c46ef274b3a93e17845c341ca8ffb1f2d5c1bb924` | 17792 |
| On-chain event alerter | `6fb084d33e4933a47c9e1998be0e206aba64cd372b05fdf6b84b4ff1b33166b5` | 17793 |

The vault storage CID is
`f47887f3f7bc3ffaf6f927f1b53db65380268c4e61aaf7814a023fa6054a8b34`; the notary
document digest is
`5999d285f64e95b7d4f1246a112d45535e852676d06f14530900876f4638a42a` with storage
CID `f35efd1f863e54dbe27914970cfd8abbfef8036199d8da8d6cd26b2718853df4`. The
test passed in 148s. See [`TESTNET_EVIDENCE.md`](TESTNET_EVIDENCE.md) for
explorer links.

## Public-Testnet Limitation

The public LEZ sequencer includes token **mints** reliably (all three use-case
mints above) but on this date did not include token **transfers** within the
polling window. The harness therefore never treats submission as inclusion and
reports a non-included transfer as `submitted_not_included` rather than hanging.
The A2A **payment** criterion is consequently evidenced on the local standalone
sequencer (real proofs, balances 90/10 — see "A2A Real-Proof Evidence" above
and `tests/three_use_cases_local.rs`), not on the public transfer path. Re-run
with `RUN_TESTNET_TRANSFERS=1` when the public write path is healthier to
upgrade the transfer anchors.
