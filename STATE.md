# STATE — logos-agent working handoff

Updated: 2026-09-08 (evening). Read this first when picking work up cold.

## Where this stands

LP-0008 submission repo (private until the end-game flip). PR #142 on
logos-co/lambda-prize is the vehicle, currently a **draft**; flipping it ready
is the submission act. Deadline: end of Friday 2026-09-18. Criteria walkthrough
complete: 22/23 MET, sole gap = narrated video covering 3 illustrative use
cases (close in progress, see below). Last commit on main: 8103f87 (not yet
pushed; pushing fires CI on this same box, so it waits for the proving queue
to drain).

## What is running (evening 2026-09-08, all automatic, logs under /tmp)

Serialized proving queue (one cargo at a time; the box is 8 cores / 15 GB and
saturates):

1. #47 identity re-land: `three_testnet_agents` (pid 466364, log
   /tmp/reland-f9-f10.log) — three category-agent identities on the public
   testnet, real proofs. Started 16:50. Hit two mint failures ("Circuit
   proving error", then a dead-connection timeout) and is on a retry with
   fresh accounts; an r0vm op has been proving ~27 min as of 18:24. Slow but
   alive. If it exhausts its retries and fails, the master chain still moves
   on to the next stage (it waits on pid exit, not success).
2. #48 use-case anchors (`three_use_cases` memory lane) — queued in
   /home/ubuntu/master-chain.sh (pid 466444, log /tmp/master-chain.log).
3. F9 lane A (real Codex + real Waku + public anchors) — queued.
4. F9 lane B (public payment, RUN_PAID_A2A=1) — queued.
5. F10 full-strength identity run with services — queued.
6. #50 recording: /home/ubuntu/record-vault-notary.sh (pid 475400) waits for
   MASTER_CHAIN_DONE, then records `demo.sh three_use_cases_local` at
   DEV_MODE=0 via asciinema in a 120x32 tmux session →
   recordings/vault-notary-real-proof.cast. Style: zoomed-out terminal, no
   overlays (matches video 1).

Casualty log: a stray parallel `three_use_cases` launch (pre-serialize
experiment) was OOM-SIGKILLed when load hit 39; it had already landed a
public Vault anchor (block 199, tx e5fc9eac12ebbc14c61cf524ebc018916a5fd18
409a72d7624e13af39f346b1e) which stays valid on-chain. The serialized #48
stage will land the full anchor set fresh.

Monitor bi333kn2l tails both logs for stage markers and failures.

## After the queue drains (tomorrow)

- Push main (commits through 8103f87), watch CI go green on it.
- Video 2: retime the cast (recordings/retime_cast.py pattern), write the
  narration script against the actual chapters, **user records the voiceover
  themselves** (~3-4 min reading), mux, upload, link in PR. Videos already on
  YT are the only video source (demo release assets deleted 2026-09-08;
  both YT videos are the user's own voice).
- #49 doc rewrites with fresh hashes: TESTNET_EVIDENCE.md, THREE_USE_CASES.md,
  THREE_TESTNET_AGENTS.md, SOLUTION.md F9/F10 lines, README dead-hash check.
- End-game: re-cut Basecamp bundles on the final commit + tag a release;
  dispatch real-proof.yml on the final commit; pin green run + hashes in
  PR #142; rebuild solutions/LP-0008.md on the fork; flip the PR ready.

## Blocked / external

- #40 waku+chat module builds (~/core4mod): ON HOLD by user decision.
  Corrected diagnosis (old "crates.io 403" label was wrong): chat built fine
  at 13:30 but its out-link was eaten by the broken retry script (wrong
  success check + nix removing the link at each failed re-run); waku's flake
  has no `install` output (real ones: default / lib). Retry loop killed; do
  not restart the old script. Fixed-script plan ready, needs user go-ahead.
- Video upload needs the user (their YouTube channel).

## Verified facts worth keeping

- Public testnet RPC: getTransaction / getAccount / getBlockRange /
  getProofsAndRoot exist; getLatestBlock / getBlockchainStatus do not.
- Dead-connection signature: bytes stuck in socket Send-Q, 0% CPU, no client
  read timeout. Wallet HTTP client has none (platform crate); our own clients
  now do (10s connect / 30s read).
- Proving ops on this box run ~11 min each at DEV_MODE=0; a 6-op test ≈ 70 min.
- Claimer program id 937554f71c96d8ace11298d2d3342e0b4ffa7d61fb394c1706a78c088f4ea471.
- gh CLI is not on PATH; use curl with the token from
  ~/.secrets/lambda-prize-token.env (never print it). Converting a PR to draft:
  REST PATCH silently ignores `draft` on fork PRs; use GraphQL
  convertPullRequestToDraft.
