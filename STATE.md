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

#47 identity re-land PASSED 19:10 (2h20m). Evidence in the memory ledger:
messaging mint tx 328d26a7... block 317, blockchain mint tx 5fc57a20...
block 338; storage mint submitted_pending (verify on-chain later).

Master-chain v1 then hit a silent no-op bug: its three_use_cases stages used
a double `--` (`-- three_lp0008_use_cases -- --ignored`), which turns
--ignored into a filter string, so stage_48/lane_A/lane_B all "passed" in
0.00s doing nothing. Caught because 0.00s plus "1 ignored" is not a pass.
Rebuilt as /home/ubuntu/master-chain-v2.sh (pid 477945, appends to
/tmp/master-chain.log) with corrected argv and the RECORDING FIRST. Stages:

1. R: warm build, then asciinema capture of `demo.sh three_use_cases_local`
   at DEV_MODE=0 in a 120x32 tmux session ->
   recordings/vault-notary-real-proof.cast (video-1 style: zoomed out, no
   overlays). RECORDING_DONE when the tmux session ends.
2. #48 memory-anchored use cases (SERVICE_BACKEND=memory).
3. Lane A: real Codex + real Waku + public anchors (RUN_PUBLIC_USE_CASES=1).
4. Lane B: paid task with public payment (RUN_PAID_A2A=1).
5. F10 full-strength identities with services on.
6. MASTER_CHAIN_DONE -> night-autopush pushes main, CI runs overnight.

Watchdog (night-watchdog.sh, armed on pid 477945): kills a stage's cargo
tree only if BOTH queue logs and ALL r0vm CPU are frozen for 45 straight
minutes; stands down at MASTER_CHAIN_DONE. Autopush (night-autopush.sh)
waits for MASTER_CHAIN_DONE, never prints the token, runs once (marker
file).

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
