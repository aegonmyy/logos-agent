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

## Run

Start Logos Storage v0.3.0 and nwaku, then run:

```bash
RISC0_DEV_MODE=0 \
  AGENT_CODEX_URL=http://127.0.0.1:8080 \
  AGENT_MESSAGING_URL=http://127.0.0.1:8645 \
  cargo test --test three_use_cases -- --ignored --nocapture
```

The test prints CIDs, Messaging topics, SHA-256 digests, task IDs, provider
accounts, and final task states. The first two workflows are service round trips.
Set `RUN_PAID_A2A=1` only with a funded, correctly configured wallet; otherwise
the paid workflow is reported as skipped rather than silently claiming payment
evidence. It must not be described as public-testnet payment evidence unless its
output includes a successful LEZ transaction and block reference.

## Evidence Standard

For a final LP-0008 submission, preserve the complete test output and record:

- Codex CIDs and verified downloaded-file digests.
- Messaging topics and message IDs or the documented standalone-node fallback.
- A2A Agent Card, discovery topic, task ID, lifecycle states, payment transaction,
  and LEZ block.
- The exact public LEZ endpoint and service versions used.
