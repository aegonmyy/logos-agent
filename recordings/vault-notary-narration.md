# Video 2 narration: vault + notary, real proofs

For you to read in your own voice. Any mic works, phone included. Read at a
natural pace and pause wherever you like: the video stretches to fit your
audio, so timing is free. About four minutes at a relaxed pace.

Sections below map to what is on screen. The cast times are where each beat
lives in the raw capture; ignore them while reading, they are for the edit.

Cast: `recordings/vault-notary-real-proof.cast` (58 min raw, 120x32).
On-screen run: `three_use_cases_local` at `RISC0_DEV_MODE=0`, 1 passed in
3473.40 s.

---

## 1. Open

*(on screen: banner "Logos Autonomous Agent, end to end demo", then
"RISC0_DEV_MODE = 0 (0 = real Groth16 proofs)" and "Running
three_use_cases_local"; cast 0:00-0:20)*

Hey, welcome back. This is video two for LP-0008. The first video covered the
core loop: the wallet, the owner approvals, and agents paying agents. This one
covers two more of the prize's illustrative use cases, running on the same
agent, with real proofs. Top of the screen, RISC0 dev mode equals zero, same
as before. When it goes quiet for a long stretch, that is the prover working.
Nothing is skipped and nothing is sped up beyond fitting an hour into a few
minutes. The stack is a real local Logos Execution Zone with a sequencer, the
same software the public testnet runs. Two scenarios here. The agent gets a
personal file vault, then it notarizes a document, privately.

## 2. Personal file vault

*(on screen: sync bar completes, then the two anchor lines: "use_case=
personal_file_vault cid=a22de530..." and "use_case=privacy_preserving_notary
cid=27ddd97c..."; cast 0:20. Then the long quiet proving stretch, ending in
"Transaction is included in block 14" with a PrivacyPreserving dump; cast
25:55)*

The vault scenario. The two lines near the top are the anchors. The first is
the agent's personal file vault: a content identifier for its encrypted
storage, plus the messaging topic it is reachable on. The file is encrypted on
the agent's side before it leaves, so the storage node only ever holds
ciphertext. Now comes the long quiet part, and it is the interesting part. The
agent pays for its own storage with a private transaction, and that payment
carries a real zero-knowledge proof. It builds the proof over its notes,
serializes it, and submits it. On the public testnet, the same kind of
transaction lands at roughly two hundred seventy kilobytes on chain, and the
repo docs show the exact RPC call to check that yourself. And there it is:
transaction included, block fourteen, type privacy preserving. That is the
vault: an agent that owns its files and its payments around them.

## 3. Privacy-preserving notary

*(on screen: second quiet proving stretch, then indexer warnings in red
("InvalidPrivacyPreservingProof", "Parked at block 7"), then "Transaction is
included in block 17"; cast 57:25-57:55)*

Second scenario, the privacy-preserving notary. The agent takes a document,
hashes it, and commits that hash privately, through the second anchor you saw
at the start. Anyone holding the document can later prove it existed, and
when, while the chain never sees the contents. Same rhythm: the quiet stretch
is proving. You will notice some red here. That is the indexer, the component
that feeds the explorer view, tripping over one of the privacy-preserving
proofs and parking. It is a known issue, disclosed in the repo's limitations
doc. The sequencer is a separate path and it is the one that matters here:
transaction included, block seventeen. The notarization is on chain.

## 4. Close

*(on screen: "use_case=paid_multi_agent_task task_id=task-0 provider=...",
"three_use_cases_local=complete personal_file_vault privacy_preserving_notary
paid_multi_agent_task", then "Demo complete, all scenarios passed with
RISC0_DEV_MODE=0" and "test result: ok. 1 passed ... finished in 3473.40s";
cast 57:55-58:10)*

And the run closes itself out: a paid multi-agent task, the provider paid in
tokens, the client balance updated. That is the scenario from video one, and
here it finishes the same single run. All three use cases in one pass: vault,
notary, paid task. Test result, one passed, three thousand four hundred
seventy-three seconds. Demo complete, dev mode equals zero. Files, notary,
payments, all through one sovereign agent. Every hash in these videos is
verifiable against the chain, and the submission links the docs that show
how. Thanks for watching.

---

## Edit notes (for the mux step, after the audio exists)

- Section 1 over the banner and anchor lines; hold the anchor frame until the
  audio moves to section 2.
- Sections 2 and 3 each have a proving stretch: slow the screen (or hold a
  frame with the anchor line visible) and let the narration carry. When the
  audio reaches "transaction included", cut to the inclusion line at the raw
  cast time for that block.
- The red indexer lines stay visible only while section 3 mentions them, then
  cut to block 17's inclusion line.
- Section 4 over the paid-task result line and the final test result; end on
  the "Demo complete" banner.
- Raw cast timestamps: block 14 inclusion 25:55, indexer warnings 57:25,
  block 17 inclusion 57:53, paid task result 57:55, test result 58:07.
