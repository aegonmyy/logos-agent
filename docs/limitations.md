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

## 3. The standalone stack's indexer stalls on a privacy-preserving proof

In the recorded real-proof run, the standalone stack's indexer rejected a
privacy-preserving proof while re-applying an early block and stopped indexing
for the rest of the run. Transaction inclusion and the agent's confirmed state
are unaffected, because the agent reads the sequencer rather than the indexer.
`docs/DEV_MODE_0_EVIDENCE.md` records what is and is not established.

## 4. An unreachable owner holds over-limit spends

A spend above the limit is retried and never auto-approved. If the owner is
permanently unreachable, the spend stays held until the owner returns.

## 5. Key loss is permanent

The agent's shielded keys and the signing key live on the agent's node. Loss of
that key material means loss of the agent's identity and any funds it holds;
this submission has no recovery path.

## 6. No AI model is bundled

The skill interface is model-agnostic and inference is left to the deployer.
This is out of scope per the prize.

## Resolved since the first submission

Two items previously listed here are resolved and now carry their own evidence:

- **The owner GUI click-through** is recorded: `recordings/basecamp-gui-demo.mp4`
  (https://youtu.be/1Ck_0keFXek), a human-driven Basecamp session approving,
  denying, and reconfiguring a live agent over real Waku, every decision
  settled on chain; narrated, with subtitles via YouTube CC. See
  `docs/BASECAMP_GUI_DEMO.md`.
- **The public testnet dropping token-program transfers** (2026-08-22) was a
  transient sequencer condition, no longer reproducible: on 2026-09-08 the
  agent's shielded send landed on the public testnet as proof-bearing
  `PrivacyPreserving` transactions (270-273 KB, blocks 88-118 after the
  operator's same-day chain reset). See `docs/SHIELDED_TESTNET_PROOF.md`.
