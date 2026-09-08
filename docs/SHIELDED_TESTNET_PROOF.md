# Shielded testnet proof (LP-0008)

A shielded (PrivacyPreserving) token Send on the public LEZ testnet at
`RISC0_DEV_MODE=0`, whose on-chain transaction body carries a real Groth16
proof. This is the proof-bearing on-chain evidence for the submission.

## The run (2026-09-08)

- RPC: `https://testnet.lez.logos.co` (official), LEZ v0.2.4
- Proof mode: `RISC0_DEV_MODE=0` (real Groth16, not the dev-mode fake-proof lane)
- Test: `tests/testnet_shielded_probe.rs`
- Run duration: 2214 s (real proving is ~10-15 min per shielded tx)

### Transactions

| | Mint | Shielded send |
|---|---|---|
| Kind | Public | **PrivacyPreserving** |
| Type byte | `0x00` | **`0x01`** |
| On-chain size | 377 bytes | **271,076 bytes** |
| Carries ZK proof | No | **Yes** (in the witness set) |
| Tx hash | `2a57150d…cb019` | `606bb9b5…72434` |
| Block | 42680 | **42709** |
| Effect | holder balance = 100 | holder 100 -> 90 (10 to private recipient) |

Verify on chain (the explorer does not index `PrivacyPreserving`
transactions, so it returns "not found" for this hash; the raw RPC returns
it):

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":["606bb9b50f0e15152281faf41d4c331f9eaa6997019a899db6d8d1683a722434"]}'
```

The committed RPC snapshot is at
`docs/testnet-evidence/v0.1.0/rpc/shielded-send-01.json` (decodes to
271,076 bytes, type byte `0x01`).

The size difference is the proof. A `Public` transaction (type `0x00`) is a
few hundred bytes: a message and signatures, no proof. A
`PrivacyPreserving` transaction (type `0x01`) carries the ZK proof in its
witness set, so it is hundreds of kilobytes. The 271,076-byte on-chain body
of the shielded send is the proof, serialized.

### Accounts

- Definition (public): `6fxDP2NjawxxHxetnniKLUBNnNqaRZcUuXFJ9uKEhB2L`
- Holder (public): `9xKyTYCqg7FnMzT5Lzxrnc9rvQM24TVfToR7b58iZnuU`
- Recipient (private): `5y3JTqGGaGMHN4R5xod6xa7K28D1tgLMuxYUb1NQaRmn`

## Why this works now (and did not before)

The prior "private transactions silently dropped" finding
(`TESTNET_EVIDENCE.md`, 2026-08-22) was a transient sequencer condition. The
public testnet exposes `getProofsAndRoot` (verified 2026-09-08), the RPC the
wallet needs to construct a `PrivacyPreserving` transaction's membership
proofs. With that RPC available, the wallet builds the shielded tx, the
sequencer includes it, and it lands with the proof on chain.

## Reproduce

```bash
RISC0_DEV_MODE=0 cargo test --test testnet_shielded_probe -- --ignored --nocapture --test-threads=1
```

The test mints 100 tokens to a public account, sends 10 to a private
recipient (the `PrivacyPreserving` tx), asserts the holder balance moves
100 -> 90, then fetches the on-chain transaction and asserts it is
`PrivacyPreserving` (type `0x01`) and over 100 KB (the proof). A green run
is itself proof that a proof-bearing transaction landed on the public
testnet at real-proof mode.

## Machine-readable evidence

- Manifest: [`v0.1.0/manifest.json`](testnet-evidence/v0.1.0/manifest.json)
- RPC snapshots: [`v0.1.0/rpc/`](testnet-evidence/v0.1.0/rpc)
  - `mint-01.json` (377 bytes, Public)
  - `shielded-send-01.json` (271,076 bytes, PrivacyPreserving, carries the proof)

> **Note on ephemerality:** a chain reset by the operator invalidates the
> hashes and snapshots above; the reproducible test regenerates equivalent
> evidence against whatever testnet is live. The invariant is the on-chain
> size and type byte: a `PrivacyPreserving` transaction at `DEV_MODE=0` is
> hundreds of kilobytes and carries the proof; a `Public` transaction is a
> few hundred bytes and does not.

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
