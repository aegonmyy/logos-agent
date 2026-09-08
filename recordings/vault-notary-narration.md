# Video 2 narration: vault + notary + paid task, real proofs

Read this in your own voice, in sync with `recordings/vault-notary-narratable.mp4`
(5 min 7 s). Play the video, start each section when its on-screen moment
arrives, and if you finish a line early just pause for a beat. If a section
runs long, no problem: say the word and I stretch that window.

The raw 58-minute capture (`vault-notary-real-proof.cast`) is compressed to
this 5:07 cut; the proving dead air is collapsed but nothing is cut. Raw-cast
times are noted per section for the edit; ignore them while reading.

This video narrates all three illustrative use cases on its own: the file
vault, the privacy-preserving notary, and the paid multi-agent task. Video 1
stays as the core-loop walkthrough.

---

## 1. Open — 0:00 to 0:49

*(on screen: banner "Logos Autonomous Agent, end to end demo", then
"RISC0_DEV_MODE = 0 (0 = real Groth16 proofs)" and "Running
three_use_cases_local"; the stack boots behind it)*

Hey, welcome back. This is video two for LP-0008. The first video covered the
core loop: the wallet, the owner approvals, and agents paying agents. This one
covers the prize's illustrative use cases, running on the same agent, with
real proofs. Top of the screen, RISC0 dev mode equals zero, same as before.
When it goes quiet for a long stretch, that is the prover working. Nothing is
skipped and nothing is sped up beyond fitting an hour into a few minutes. The
stack is a real local Logos Execution Zone with a sequencer, the same software
the public testnet runs. Three scenarios run here in one pass: the agent gets
a personal file vault, it notarizes a document privately, and it completes a
paid task for another agent.

## 2. Personal file vault — 0:50 to 2:10

*(on screen: the two anchor lines print at 0:50: "use_case=
personal_file_vault cid=a22de530..." and "use_case=privacy_preserving_notary
cid=27ddd97c...", then the screen holds on the anchor while proving would run;
"Transaction is included in block 14" with a PrivacyPreserving dump arrives at
2:10, and the "there it is" line lands right on it)*

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

## 3. Privacy-preserving notary — 2:10 to 3:35

*(on screen: the block-14 PrivacyPreserving dump scrolls out and the screen
settles, then at ~3:04 the indexer warnings in red:
"InvalidPrivacyPreservingProof", "Parked at block 7"; "Transaction is
included in block 17" arrives at 3:35, timed to the last sentence)*

Second scenario, the privacy-preserving notary. The agent takes a document,
hashes it, and commits that hash privately, through the second anchor you saw
at the start. Anyone holding the document can later prove it existed, and
when, while the chain never sees the contents. Same rhythm: the quiet stretch
is proving. You will notice some red here. That is the indexer, the component
that feeds the explorer view, tripping over one of the privacy-preserving
proofs and parking. It is a known issue, disclosed in the repo's limitations
doc. The sequencer is a separate path and it is the one that matters here:
transaction included, block seventeen. The notarization is on chain.

## 4. Paid multi-agent task — 3:35 to 5:00

*(on screen: the block-17 dump scrolls out at 3:35, then the payment itself
prints as a PrivacyPreserving transaction; at 4:15 the task lines land:
"use_case=paid_multi_agent_task task_id=task-0 provider=EE72BU1g..." and
"three_use_cases_local=complete personal_file_vault privacy_preserving_notary
paid_multi_agent_task"; the screen holds on them until 5:00)*

Third scenario, and it is the one from video one: a paid task between two
agents. The client agent posts the task, a provider agent picks it up, and
what is scrolling past now is the payment itself, printing in that same
privacy-preserving format you just saw twice. The proving happened in the
quiet stretch; this is the submission. And here is the landing, right on cue:
task zero, the provider's account right there on screen, and the token the
provider was paid in. Two sovereign agents, one marketplace transaction, zero
human keystrokes. So, three for three in a single run: the vault, the notary,
and the paid task. Files, notary, payments, all through one agent that owns
its keys, its data, and its money. Every hash in this video is verifiable
against the chain, and the submission links the docs that show exactly how.

## 5. Close — 5:00 to end

*(on screen: "test result: ok. 1 passed ... finished in 3473.40s" at 5:00,
then the "Demo complete" banner at ~5:05 to end on)*

Test result: one passed, three thousand four hundred seventy-three seconds.
Demo complete, dev mode equals zero. Thanks for watching.

---

## Edit notes (for the mux step, after the audio exists)

- The narratable mp4 carries the beat timing: vault anchor 0:50, block 14 at
  2:10, indexer red at ~3:04, block 17 at 3:35, paid-task lines at 4:15, test
  result 5:00, "Demo complete" ~5:05, end 5:07. Mux the voiceover straight
  onto it.
- If a recorded section runs long, extend that window's hold in
  `retime_vault_notary.py` (the BEATS targets) and re-render; the holds are
  single numbers, the retimer is deterministic.
- Raw-cast landmarks, if needed for a caption or follow-along: block 14
  inclusion 25:55, indexer warnings 57:25, block 17 inclusion 57:53, payment
  dump 57:55, paid task result 57:55, test result 58:07 of the 58-minute
  original.
