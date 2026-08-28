# What does not work

The honest limits of this submission, stated so a reviewer knows exactly what
the agent does and does not guarantee. Each item points at the evidence where
the limit is described in full.

## 1. Agent Card keys are self-certifying, not account-bound

The signing key in an Agent Card proves the card was not tampered with and that
repeat cards come from the same key. It does not bind that key to the agent's
shielded LEZ account, because shielded accounts cannot produce message
signatures (their nullifier and viewing keys generate ZK proofs; the wallet's
`sign_message` covers public accounts and keycards). Deployments should pin the
signing key once over a trusted channel, the owner channel, before relying on
it. See the security model in `README.md`.

## 2. The Basecamp module loads an offline session

The Logos Core module executes reflective, storage, and messaging skills
through the runtime. The wallet-backed on-chain skills are not yet driven
through the module session; they run from the headless `agent` binary and the
CLI. See `docs/LOGOS_CORE_LOADED.md`.

## 3. The owner GUI click-through is not yet recorded

The owner-channel FFI is implemented, unit-tested, and verified across the C
ABI (a standalone harness dlopens `liblogos_agent.so` and exercises every
owner-channel symbol), and the Qt module builds with the Logos module builder.
The hold -> approve -> execute flow itself is proven by automated tests
(`owner_ffi_e2e.rs` and `owner_ffi_waku.rs`, the latter with real Waku as the
transport against a live nwaku node). What is still needed for full evidence is
a recorded click-through of the QML approve/deny UI against a live agent and a
Waku node.

## 4. The public testnet drops token-program transfers

On 2026-08-22 the public sequencer included token-program mints but silently
dropped token-program transfers: a submitted transfer hash returns `null` from
`getTransaction` while a sibling mint from the same run is queryable (null
control included). Settlement is therefore evidenced through the program-call
path, which the sequencer does include, and the token `Send` leg is evidenced
on a local standalone sequencer with real proofs. See `docs/TESTNET_EVIDENCE.md`
for the null-control diagnosis.

## 5. The standalone stack's indexer stalls on a privacy-preserving proof

In the recorded real-proof run, the standalone stack's indexer rejected a
privacy-preserving proof while re-applying an early block and stopped indexing
for the rest of the run. Transaction inclusion and the agent's confirmed state
are unaffected, because the agent reads the sequencer rather than the indexer.
`docs/DEV_MODE_0_EVIDENCE.md` records what is and is not established.

## 6. An unreachable owner holds over-limit spends

A spend above the limit is retried and never auto-approved. If the owner is
permanently unreachable, the spend stays held until the owner returns.

## 7. Key loss is permanent

The agent's shielded keys and the signing key live on the agent's node. Loss of
that key material means loss of the agent's identity and any funds it holds;
this submission has no recovery path.

## 8. No AI model is bundled

The skill interface is model-agnostic and inference is left to the deployer.
This is out of scope per the prize.
