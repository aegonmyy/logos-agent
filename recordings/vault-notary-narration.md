# Video 2 narration: three use cases, real proofs (sync-free read)

How to use this: play `vault-notary-narratable.mp4` and read the sections in
order, at your own pace. You never need to hit an exact moment. Every line is
written so it stays true whenever you say it within its section. If you drift
ahead of the screen or fall behind it, keep reading. Record in one take or
section by section, and if you flub a line just repeat it and keep going. I
stretch the video to fit your audio afterward.

Context while you read: the raw capture is 58 minutes, compressed to a 5:07
cut. The proving dead air is collapsed and nothing is cut. Whenever the
screen sits quiet for a long stretch, that is the prover building a real
Groth16 proof. The parenthetical notes under each header describe roughly
what is on screen; they are for you, not to be read aloud.

---

## 1. Open

(banner, then "RISC0_DEV_MODE = 0" and "Running three_use_cases_local" near
the top of the screen, the stack booting)

Hey, welcome back. This is video two for LP-0008. Video one covered the core
loop: the agent's own wallet, owner approvals, and agents paying agents. This
one walks the prize's illustrative use cases on the same agent. The why of
this build: A2A gives agents a common protocol, and it ships with no payment
rail and no privacy. The Logos agent closes both gaps. It holds its own
shielded account on the Logos Execution Zone, so its identity, its private
transactions, and its proving come from the platform itself. Skills are
plug-ins: storage, messaging, blockchain, and the A2A marketplace, and the
repo documents the interface for adding new skills without touching the
core. Top of the screen, RISC0 dev mode equals zero, so every quiet stretch
you see is a real proof being built. Nothing here runs in dev mode. This run
is against a real local sequencer, the same software the public testnet
runs, and three scenarios complete in a single pass: a personal file vault,
a privacy-preserving notary, and a paid task between two agents.

## 2. Personal file vault

(two "use_case=" anchor lines print, a long quiet stretch, then a large
PrivacyPreserving transaction dump ending in "Transaction is included in
block 14")

First scenario, the personal file vault. Each use case prints an anchor
line: a content identifier and the messaging topic the agent is reachable
on. The vault flow is: the agent encrypts the file on its own side, uploads
it, and the storage node only ever holds ciphertext. That content identifier
is the address of the ciphertext. Anyone holding the key can fetch the file
back from any device and check the bytes. Then the agent anchors the vault
on chain, and that is what the long quiet stretch is: a private transaction
carrying a real zero-knowledge proof, built over the agent's notes. When it
lands, the chain shows it included in a block, type privacy-preserving. The
same kind of transaction on the public testnet lands at roughly two hundred
seventy kilobytes on chain, and the repo docs give the exact RPC call to
check that yourself. That is the vault end to end: an agent that owns its
files and the payments around them.

## 3. Privacy-preserving notary

(red indexer warnings appear somewhere in this stretch:
"InvalidPrivacyPreservingProof", "Parked at block 7"; later a second
inclusion line, "Transaction is included in block 17")

Second scenario, the privacy-preserving notary. The agent hashes a document
and commits that hash privately through its second anchor. Anyone holding
the document can later prove it existed, and roughly when, while the chain
never sees the contents. Same rhythm as the vault: the quiet stretch is
proving, and the notarization lands as another privacy-preserving
transaction. At one point you will see red on screen. That is the indexer,
the component that feeds the explorer view, tripping over one of these
proofs and parking. It is a known issue and it is disclosed in the repo's
limitations doc. The sequencer is a separate path, and it is the one that
matters here: the inclusion line prints, and the notarization is on chain.

## 4. Paid multi-agent task

(another PrivacyPreserving dump, this one the payment; then the task lines:
"use_case=paid_multi_agent_task task_id=task-0 provider=..." and
"three_use_cases_local=complete ...")

Third scenario, the paid skill marketplace. Two agents, no human in the
loop. The provider publishes an Agent Card on a discovery topic with a price
in LEZ. The client discovers the card, sends the task, and pays the declared
price. What scrolls past is the payment itself, in the same
privacy-preserving format you have seen twice already: a shielded LEZ
transfer with its own proof. The closing line is the verification. It shows
the task completed, the provider's account, the token it was paid in, and
the balances: ninety for the client, ten for the provider. Two sovereign
agents, one marketplace transaction, zero keystrokes. So, three for three in
a single run. Files, notarization, and payment, through one agent that owns
its keys, its data, and its money. The vault and the notary also anchor on
the public testnet with included transactions, and the submission links the
docs carrying every hash plus the RPC commands to verify them.

## 5. Close

("test result: ok. 1 passed ... finished in 3473.40s", then the "Demo
complete" banner)

The run closes green: one test, all three use cases, three thousand four
hundred seventy-three seconds of it, dev mode equals zero throughout. Thanks
for watching.

---

## Edit notes (for the mux step, after the audio exists)

- This script is sync-free on purpose: align audio to video by section
  boundaries only (open, vault, notary, paid task, close), then stretch each
  section's hold to match the recorded section length in
  `retime_vault_notary.py` (the BEATS targets) and re-render. The retimer is
  deterministic.
- Narratable-cut landmarks for the alignment: vault anchor 0:50, block 14
  inclusion 2:10, indexer red ~3:04, block 17 inclusion 3:35, paid-task
  lines 4:15, test result 5:00, "Demo complete" ~5:05, end 5:07.
- Raw-cast landmarks, if needed for a caption or follow-along: block 14
  inclusion 25:55, indexer warnings 57:25, block 17 inclusion 57:53, payment
  dump 57:55, paid task result 57:55, test result 58:07 of the 58-minute
  original.
