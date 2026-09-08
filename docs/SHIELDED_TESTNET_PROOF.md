# Shielded testnet proof (LP-0008)

The agent's own shielded account receiving and spending tokens on the public
LEZ testnet at `RISC0_DEV_MODE=0`, with every transaction carrying a real
Groth16 proof in its on-chain body. This is the proof-bearing on-chain
evidence for the submission.

## The run (2026-09-08, after the operator's chain reset)

The public testnet was reset by its operators on 2026-09-08 (serving from
genesis again), which killed every previously cited hash. This run re-landed
the evidence on the fresh chain, agent-driven end to end.

- RPC: `https://testnet.lez.logos.co` (official), LEZ v0.2.4
- Proof mode: `RISC0_DEV_MODE=0` (real Groth16, not the dev-mode fake-proof lane)
- Test: `tests/testnet_agent_spend.rs` (run duration 3320 s; real proving is
  ~10-15 min per shielded transaction)

What the run asserted, in order:

1. **Receives into its own shielded account**: 100 `AGENT-TESTNET` tokens
   minted to the agent's private account (proof-bearing mint, block 87).
2. **Spends autonomously below the limit**: a 10-token send under the per-tx
   limit of 50 executes with no owner involved; the shielded spend phase
   produced three further `PrivacyPreserving` transactions (blocks 88, 111,
   118; a shielded spend restructures notes, and each transaction carries its
   own proof), and the agent's balance reads 100 → 90 after sync, with the 10
   at the private recipient.
3. **Holds above the limit**: a 75-token request over the limit of 50 returns
   `NeedsOwnerApproval`, submits nothing, and the balance stays 90.

### Transactions

| | Mint (to shielded) | Spend tx 1 | Spend tx 2 | Spend tx 3 |
|---|---|---|---|---|
| Kind | **PrivacyPreserving** | **PrivacyPreserving** | **PrivacyPreserving** | **PrivacyPreserving** |
| Type byte | `0x01` | `0x01` | `0x01` | `0x01` |
| On-chain size | 270,810 B | 272,778 B | 273,066 B | 270,480 B |
| Tx hash | `a178944818bc67ee776bcc9ca4e8bf5a798b4d1cf1aab8ce5a84650c079799c6` | `a3eaeb3a773f35a48935944ca1b15bed683265dbded90633c094e4ef56aa4f4b` | `2a283d3c8887ef552bf56f415bbd6b534a4424e0765b590f3b44f6f6215a0aaa` | `eab567f88e164945769c342d35540c7e7ae610c267c4247d877868ae51af8b93` |
| Block | 87 | 88 | 111 | 118 |
| Carries ZK proof | Yes | Yes | Yes | Yes |

Verify on chain (the explorer does not index `PrivacyPreserving`
transactions, so it returns "not found" for these hashes; the raw RPC returns
them):

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":["a3eaeb3a773f35a48935944ca1b15bed683265dbded90633c094e4ef56aa4f4b"]}'
```

To check the on-chain size of any of the four, base64-decode the returned
body and count the bytes — this is the exact derivation of the table's size
row (substitute the other three hashes from the table above):

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":["a3eaeb3a773f35a48935944ca1b15bed683265dbded90633c094e4ef56aa4f4b"]}' \
  | jq -r '.result[0]' | base64 -d | wc -c
# -> 272778  (spend-01; mint-01 -> 270810, spend-02 -> 273066, spend-03 -> 270480)
```

The committed RPC snapshots for all four transactions are in
[`testnet-evidence/v0.2.0/rpc/`](testnet-evidence/v0.2.0) with the manifest at
[`testnet-evidence/v0.2.0/manifest.json`](testnet-evidence/v0.2.0/manifest.json).

### Accounts

Note on verifying balances: these are shielded accounts, so a plain
`getAccount` returns the **public** state view and reads `balance: 0` for
both private accounts — that is by design, not a contradiction. The holdings
live in shielded notes; the proof of the 100-token mint and the 10-token
spend is the `PrivacyPreserving` transactions themselves (mint-01 carries the
100 tokens into the agent's account, spend-01 moves 10 to the recipient).
The `100 -> 90` balance figures above are the agent wallet's own synced
note balances, reproduced by running the test below.

- Definition (public): `2fZi7k8cWTWEyKbZ9yhe6iSCE9excNQvg1W1DaY44KEz`
- Agent (private): `FxccS3PJDSpKsD5iTWYeiUWuZs4KqBfi7NPSL6Hnuht1`
- Recipient (private): `B8dyBNFa8WtiYcRkvGB67N4CcJDUttcggWtAgUBiLNmG`

The size is the proof. A `Public` transaction (type `0x00`) is a few hundred
bytes: a message and signatures, no proof. A `PrivacyPreserving` transaction
(type `0x01`) carries the ZK proof in its witness set, so it is hundreds of
kilobytes. The ~271 KB on-chain bodies above are the proofs, serialized.

## Superseded run (pre-reset, archived)

The first proof-bearing run (2026-09-08, earlier that day) used
`tests/testnet_shielded_probe.rs`: a public 377-byte mint (`2a57150d…cb019`,
block 42680) and a 271,076-byte shielded send (`606bb9b5…72434`, block 42709).
The operator's reset the same day invalidated those hashes on the live chain;
the committed snapshots remain archived at
[`testnet-evidence/v0.1.0/`](testnet-evidence/v0.1.0). The v0.2.0 run above is
the same invariant regenerated on the fresh chain, now agent-driven.

## Why this works now (and did not before)

The prior "private transactions silently dropped" finding
(`TESTNET_EVIDENCE.md`, 2026-08-22) was a transient sequencer condition. The
public testnet exposes `getProofsAndRoot` (verified 2026-09-08), the RPC the
wallet needs to construct a `PrivacyPreserving` transaction's membership
proofs. With that RPC available, the wallet builds the shielded tx, the
sequencer includes it, and it lands with the proof on chain.

## Reproduce

```bash
RISC0_DEV_MODE=0 cargo test --test testnet_agent_spend -- --ignored --nocapture --test-threads=1
```

The test mints 100 tokens into the agent's own shielded account, spends 10
autonomously (asserting the balance 100 → 90), requests 75 over the limit
(asserting `NeedsOwnerApproval` and an unchanged balance), then scans the
chain for the run's proof-bearing transactions and fetches one back over RPC
asserting it is `PrivacyPreserving` (type `0x01`) and over 100 KB. A green
run is itself proof that proof-bearing transactions landed on the public
testnet at real-proof mode.

> **Note on ephemerality:** a chain reset by the operator invalidates the
> hashes and snapshots above; the reproducible test regenerates equivalent
> evidence against whatever testnet is live (exactly what happened between
> v0.1.0 and v0.2.0 here). The invariant is the on-chain size and type byte:
> a `PrivacyPreserving` transaction at `DEV_MODE=0` is hundreds of kilobytes
> and carries the proof; a `Public` transaction is a few hundred bytes and
> does not.

## CI

The `real-proof-e2e` workflow (`.github/workflows/real-proof.yml`) runs at
`RISC0_DEV_MODE=0`: the agent spending flow on a local sequencer (required,
deterministic) and the shielded-send probe against the public testnet
(best-effort with `continue-on-error`, since the public testnet flaps; the
durable evidence is the committed RPC snapshot above). It is dispatched on
demand (`workflow_dispatch`) on the commit that needs real-proof evidence:
real proving is hours per run, so it is not bound to every push. The fast
`e2e` job in `ci.yml` remains at `DEV_MODE=1` on every push for quick
feedback.
