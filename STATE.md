# STATE — logos-agent working handoff

Updated: 2026-09-08 (evening). Read this first when picking work up cold.

## Where this stands

LP-0008 submission repo (private until the end-game flip). PR #142 on
logos-co/lambda-prize is the vehicle, currently a **draft**; flipping it ready
is the submission act. Deadline: end of Friday 2026-09-18. Criteria walkthrough
complete: 22/23 MET, sole gap = narrated video covering 3 illustrative use
cases (close in progress, see below). Last commits on main: e3bf57a (weboko
audit doc fixes), dcb4a0d (video 2 cast + script), bd24c09 (narratable cut);
PR #142 body already patched with the live v0.2.0 evidence set (pin e3bf57a,
blocks 87-118 + 148-150, GUI video no longer claimed narrated).

Weboko audit (#51) closed: every published hash walked via RPC, 8 live
headline hashes verified minutes before publishing, v0.1.0 dir carries a
SUPERSEDED marker, dead surfaces scheduled for rewrite in #49.

## What is running (evening 2026-09-08, all automatic, logs under /tmp)

1. R recording DONE 20:12 (58 min, 1 passed in 3473.40s at DEV_MODE=0) ->
   recordings/vault-notary-real-proof.cast. Retimed to a 4:26 narratable cut
   (recordings/vault-notary-narratable.cast, beats pinned so each paragraph
   lands on its on-screen moment; retime_vault_notary.py). Narration script
   with sync windows: recordings/vault-notary-narration.md. User records the
   voiceover TONIGHT against the narratable mp4 (render pending, see below).
2. #48 F9 anchors DONE 20:16 (1 passed, 181.71s): vault block 402, notary 403,
   alerter 404, all verified LIVE.
3. Lane A DONE 20:19 (1 passed, 171.01s): real Codex + Waku anchors verified
   LIVE: vault 405 (cid zDvZRwzm2qMNXP9...), notary 406 (sha256 5999d285...,
   same doc hash as #48), alerter 407.
4. Lane B RUNNING since 20:19: payer funded (8K5JHkmr..., balance 100),
   payment tx e4f759ed8e9b5e338f585ba6ff8d7221f9b6fcd78e967db0cf93cce7f680
   89f1 submitted, proving.
5. F10 full-strength identities queued after lane B.
6. MASTER_CHAIN_DONE -> night-autopush pushes main, CI runs overnight.

Watchdog (night-watchdog.sh, armed on pid 477945): kills a stage's cargo
tree only if BOTH queue logs and ALL r0vm CPU are frozen for 45 straight
minutes; stands down at MASTER_CHAIN_DONE. Autopush (night-autopush.sh)
waits for MASTER_CHAIN_DONE, never prints the token, runs once (marker
file).

Video-2 render tooling: asciinema's agg is NOT in nixpkgs (that agg is the
graphics lib; the flake build wants rustc-from-source, too heavy mid-prove);
installing via `cargo install --git https://github.com/asciinema/agg`, then
`agg <narratable.cast> <mp4>` at 120x32.

Casualty log: a stray parallel `three_use_cases` launch (pre-serialize
experiment) was OOM-SIGKILLed when load hit 39; it had already landed a
public Vault anchor (block 199, tx e5fc9eac12ebbc14c61cf524ebc018916a5fd18
409a72d7624e13af39f346b1e) which stays valid on-chain. The serialized #48
stage landed the full anchor set fresh (402-404), superseding it.

Monitor bi333kn2l tails both logs for stage markers and failures.

## After the queue drains (tonight/tomorrow)

- Watch lane B -> F10-full -> autopush; verify CI green on the pushed commit.
- Video 2: render the narratable cast to mp4, user reads
  recordings/vault-notary-narration.md in sync TONIGHT, mux audio, upload,
  link in PR. Videos already on YT are the only video source (demo release
  assets deleted 2026-09-08).
- #49 doc rewrites with fresh hashes: TESTNET_EVIDENCE.md, THREE_USE_CASES.md,
  THREE_TESTNET_AGENTS.md (cite F10-full mints once landed), SOLUTION.md F9/F10
  lines, README dead-hash check.
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
