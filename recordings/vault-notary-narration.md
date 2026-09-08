# Video 2 narration: vault + notary, real proofs

Read this in your own voice, in sync with `recordings/vault-notary-narratable.mp4`
(4 min 26 s). Play the video, start each section when its on-screen moment
arrives, and if you finish a line early just pause for a beat. If a section
runs long, no problem: say the word and I stretch that window.

The raw 58-minute capture (`vault-notary-real-proof.cast`) is compressed to
this 4:26 cut; the proving dead air is collapsed but nothing is cut. Raw-cast
times are noted per section for the edit; ignore them while reading.

---

## 1. Open — 0:00 to 0:49

*(on screen: banner "Logos Autonomous Agent, end to end demo", then
"RISC0_DEV_MODE = 0 (0 = real Groth16 proofs)" and "Running
three_use_cases_local"; the stack boots behind it)*

Hey, welcome back. This is video two for LP-0008. The first video covered the
core loop: the wallet, the owner approvals, and agents paying agents. This one
covers two more of the prize's illustrative use cases, running on the same
agent, with real proofs. Top of the screen, RISC0 dev mode equals zero, same
as before. When it goes quiet for a long stretch, that is the prover working.
Nothing is skipped and nothing is sped up beyond fitting an hour into a few
minutes. The stack is a real local Logos Execution Zone with a sequencer, the
same software the public testnet runs. Two scenarios here. The agent gets a
personal file vault, then it notarizes a document, privately.

## 2. Personal file vault — 0:49 to 2:01

*(on screen: the two anchor lines print at 0:49: "use_case=
personal_file_vault cid=a22de530..." and "use_case=privacy_preserving_notary
cid=27ddd97c...", then the screen holds on the anchor while proving would run;
"Transaction is included in block 14" with a PrivacyPreserving dump arrives at
2:01, and the "there it is" line lands right on it)*

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

## 3. Privacy-preserving notary — 2:01 to 3:28

*(on screen: the block-14 PrivacyPreserving dump scrolls out, then at ~2:55
the indexer warnings in red: "InvalidPrivacyPreservingProof", "Parked at
block 7"; "Transaction is included in block 17" arrives at 3:28, timed to the
last sentence)*

Second scenario, the privacy-preserving notary. The agent takes a document,
hashes it, and commits that hash privately, through the second anchor you saw
at the start. Anyone holding the document can later prove it existed, and
when, while the chain never sees the contents. Same rhythm: the quiet stretch
is proving. You will notice some red here. That is the indexer, the component
that feeds the explorer view, tripping over one of the privacy-preserving
proofs and parking. It is a known issue, disclosed in the repo's limitations
doc. The sequencer is a separate path and it is the one that matters here:
transaction included, block seventeen. The notarization is on chain.

## 4. Close — 3:28 to 4:26

*(on screen: "use_case=paid_multi_agent_task task_id=task-0 provider=..." and
"three_use_cases_local=complete personal_file_vault privacy_preserving_notary
paid_multi_agent_task", then "test result: ok. 1 passed ... finished in
3473.40s" at 4:06, then the "Demo complete" banner at 4:21 to end on)*

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
