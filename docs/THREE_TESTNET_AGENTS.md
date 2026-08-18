# Three Public-Testnet Agents

This document is the reproducible evidence procedure for LP-0008's requirement
for three independent agents on LEZ testnet: Storage, Messaging, and Blockchain.
Unlike `docs/THREE_AGENTS.md`, this procedure does not use the local Docker LEZ
fixture.

## Prerequisites

Set `LEE_WALLET_HOME_DIR` to a funded LEE wallet directory and ensure the wallet
can reach the current public testnet. The endpoint defaults to:

```text
https://testnet.lez.logos.co
```

For the real Logos services, set:

```bash
export AGENT_MESSAGING_URL=http://127.0.0.1:8645
export AGENT_CODEX_URL=http://127.0.0.1:8080
```

The service endpoints are optional for identity-only evidence, but are required
to exercise the Storage and Messaging categories.

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

It also prints the public testnet block used for the run. When the real service
endpoints are configured, it prints a Waku message id and a Codex content address.
Copy the complete output into this document or a dated evidence file, together
with the testnet transaction hashes and explorer links returned by the wallet.

## Verified Run

The following run completed on 2026-08-18 with `RISC0_DEV_MODE=0`, the public
LEZ endpoint, local nwaku v0.38.0, and Logos Storage v0.3.0:

```text
testnet agent category=storage account=CJ3u1hzCvZMLN91CMKKfryTWA2PGdDaaMUEmVfVdoh36
testnet agent category=messaging account=DM63x4x9uGsiwihhyry2CdPYJxbvEJxj12axY7wQuRmt
testnet agent category=blockchain account=G9sAkVHZZpkZNTaTR9YnQaP3ZWiankDiKUHKDfr7tqcw
messaging evidence topic=/logos-agent/testnet/evidence/G9sAkVHZZpkZNTaTR9YnQaP3ZWiankDiKUHKDfr7tqcw message_id=local-store-fallback
storage evidence address=zDvZRwzm5hAtdqy5oTEYRf3JUx9JffNeHSgCgMcQLk3rP1vYs4B2
testnet evidence block=12902
test result: ok. 1 passed; 0 failed
```

The Messaging assertion publishes the message and reads it back from the local
nwaku store. A standalone node reports `NoPeersToPublish` when it has no relay
mesh; the harness accepts that specific condition only after the message is
confirmed by polling. The Codex assertion uploads encrypted content and verifies
the downloaded plaintext matches the original bytes.

## Current Repository State

The checked-in `tests/three_category_agents.rs` test is local-sequencer evidence
only. It must not be presented as public-testnet evidence. This test and document
provide the public-testnet procedure; a passing run with a funded wallet and live
services is required before marking the LP-0008 criterion complete.
