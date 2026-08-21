# RISC0_DEV_MODE=0 real-proof evidence — logos_agent

Test: agent_spending::agent_spends_within_limit_and_holds_above_it
Date: 2026-08-06
Command: RISC0_DEV_MODE=0 cargo test -p logos_agent --test agent_spending

## Result
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2756.40s

## Proof mode confirmation
dev-mode warnings in log (must be 0): 0
real privacy-preserving transactions proved: 6

## Transaction hashes (on the local sequencer)
Transaction hash is 142fbca9c229f8510cd512e7d26f5bc692f0e63e807cbe6cb85c232a41f42a5b
Transaction hash is b912f14276180f255ac9c55b8ee86302676c639568f5d015f0fd8a6fbac4ef2a
Transaction hash is d190e57994226fc1661223f7c54071f63da2a70217dd60d488bec0af53d2023a
Transaction hash is e27231ba76529db994f860a79ff616226c6656ce94f020e27f1030dabccd21e7
Transaction hash is 05c0834cc2f7ff26bdbb1820d58b1c226ac9283becd7e2b03eae78fd16b9cbb4
Transaction hash is 005753262954ed34186b884dbf781409ba7ed30d1ec8158fea5419f800569907

## Indexer errors in the real-proof runs (known issue — read before watching the recording)

The real-proof recording (`recordings/logos-agent-real-proof.cast`) and the runs
above show the standalone stack's **indexer** logging errors while the scenarios
run:

```
WARN  common::transaction] Error at transition InvalidPrivacyPreservingProof
ERROR indexer_core] Failed to apply block 7 (attempt 1/3), will retry: State transition failed at transaction 0: Invalid...
ERROR indexer_core] Parked at block 7 after 3 failed apply attempts
ERROR indexer_core] Parked at block 8: Unexpected block id: expected 7, got 8
... (one "Parked at block N" line per later block, for the rest of the run)
```

What is verified from the runs themselves:

- The **sequencer accepts the agent's transactions and includes them** —
  confirmations appear on screen with hash and block number (for example
  `660c17e1…` included in block 14 and `931132b2…` in block 18 in the recording).
- The agent's **wallet syncs and reads confirmed state from the sequencer**
  (balances, received accounts), and every scenario passes. The agent's read path
  is the sequencer, not the indexer.
- The indexer is a separate indexing subsystem of the standalone Docker stack. It
  rejects a privacy-preserving proof while re-applying block 7, exhausts its
  three retries, parks at block 7, and then refuses every later block because it
  expects the parked one — the `Unexpected block id` lines are that stall
  repeating, not new failures.

So the indexer stall does not affect transaction inclusion, confirmation, or any
result claimed in these runs. What it does mean is that the stack's index is not
usable for the rest of the run, and no claim in this submission depends on it.

What is **not** established from the recording: why the indexer rejects a proof
the sequencer accepted (for example, whether its proof verifier is stubbed or
mismatched in this standalone stack under real proving). That has not been
diagnosed, and we do not guess at it here.
