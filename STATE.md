# STATE - logos-agent working handoff

Updated: 2026-09-09 (morning). Read this first when picking work up cold.

## Where this stands

LP-0008 submission repo (private until the end-game flip). PR #142 on
logos-co/lambda-prize is the vehicle, currently a **draft**; flipping it ready
is the submission act. Deadline: end of Friday 2026-09-18. Spec verified
2026-09-09 against the fetched criteria text: lane B (public-testnet paid A2A
payment) is NOT a prize requirement - the A2A payment criterion names no chain
venue (met on the standalone sequencer at DEV_MODE=0, balances 90/10, payment
tx 3d065660 block 17 in the 58-min capture), and "3 illustrative use cases on
LEZ testnet" is met by vault+notary+alerter anchors. The narrated 3-use-case
video EXISTS: user recorded it and uploaded https://youtu.be/dg6RuNw44a8
(live, public, "LP-0008 3 usecase demo"); linked in SOLUTION.md, LP-0008.md,
and PR #142 with the reviewer subtitle (CC) tip. Content itself not
re-verified from this box (YouTube bot-wall); existence/title/author verified.

#49 doc rewrites DONE (commit e9c3d99, pushed): TESTNET_EVIDENCE.md
(restructured to current epoch, evidence map + headline table), THREE_USE_CASES.md
(fresh anchor tables + lane-selection env precedence documented), 
THREE_TESTNET_AGENTS.md (self-funding prerequisite, two-run identity/mint table),
SOLUTION.md (6 stale spots fixed: YT video links, criteria 9 with live hashes,
CI checkbox flipped green on run 34293206202 / a906e22). All em dashes removed
from the four docs (de-slop sweep).

**Lapsed-hash discovery 2026-09-09 morning:** of 20 headline hashes
batch-verified live, 3 now return null (notary service anchor cb1b2b89/406,
notary re-run 83c20690/634, alerter re-run d6956ed4/635). Block 406 now serves
a 272,701B shielded block (no room for our anchor); 634/635 changed shape too:
the operator's node is serving different content for some older heights. The
other 17 are stable (87-118, 144, 148-150, 317, 338, 458, 402-405, 407, 633).
Response: fresh lane A run 2026-09-09 morning LANDED and live-verified
(/tmp/laneA-fresh.log, 1 passed 164.35s): vault tx 54bfd294... block 747
(real Codex CID zDvZRwzm4zQmJH...), notary tx 5a90dd77... block 748 (real
Codex CID zDvZRwzkxZxD..., digest 5999d285 again), alerter tx c9d8debd...
block 749. Docs cite the fresh trio as primary + the still-live older rows;
lapsed rows dropped with an honest note. Narration script rewritten 2026-09-09
as a sync-free rough read (user records at own pace; section-level alignment,
holds stretched after); kit repo aegonmyy/lp0008-video2 pushed 382197c.

## What is running (evening 2026-09-08, all automatic, logs under /tmp)

1. R recording DONE 20:12 (58 min, 1 passed in 3473.40s at DEV_MODE=0) ->
   recordings/vault-notary-real-proof.cast. Retimed to a 5:07 narratable cut
   (recordings/vault-notary-narratable.cast; retime_vault_notary.py). The
   narration script lives ONLY in the private kit repo aegonmyy/lp0008-video2
   (sync-free rough read; scripts were stripped from this repo 2026-09-09).
   User records the voiceover against the narratable mp4 at their own pace.
2. #48 F9 anchors DONE 20:16 (1 passed, 181.71s): vault block 402, notary 403,
   alerter 404, all verified LIVE.
3. Lane A DONE 20:19 (1 passed, 171.01s): real Codex + Waku anchors verified
   LIVE: vault 405 (cid zDvZRwzm2qMNXP9...), notary 406 (sha256 5999d285...,
   same doc hash as #48), alerter 407.
4. Lane B saga (paid task with public payment, RUN_PAID_A2A=1):
   run 1 FAILED 20:44 (0 passed, exit=101, 1520s) - payer funded, payment
   e4f759ed submitted then "All pollers failed" (wallet multi_poll all-
   connections-error, poller.rs:109 = the flap); public payer tx 4914b317
   LANDED block 408 (373B), shielded payment never broadcast (getTransaction
   null). RETRY "PASS" 00:07 WAS A FALSE LANE: my laneb-retry.sh carried
   SERVICE_BACKEND=memory, and the test checks SERVICE_BACKEND=memory FIRST
   and early-returns after the public anchors (three_use_cases.rs:470-480),
   so run 2 was another #48-style anchor pass (bonus anchors blocks 633-638
   incl. probe 1's), NOT the paid leg. Correct invocation = master-chain-v2.sh
   line 45 verbatim: RISC0_DEV_MODE=0 RUN_PAID_A2A=1, no SERVICE_BACKEND.
   laneb-retry.sh fixed; REAL lane B attempt 1 FAILED again 00:36 (0 passed,
   exit=101, 1512.78s): setup mint 64a49b4c landed block 641 (373B public),
   payment 905a91c2 hash printed then "All pollers failed"; BOTH payments
   (905a91c2 + run 1's e4f759ed) confirmed ABSENT from chain while same-run
   public txs land - root cause: the Send submit rides the wallet's pooled
   HTTP connection, dropped by the testnet after short idle stretches (hash
   prints, tx never reaches sequencer, command's internal wait dies);
   wait_for_transaction_bounded tolerates poll errors but the Send's INTERNAL
   wait propagates the error. FIX (00:45): tests/three_use_cases.rs Send leg
   wrapped in 3-attempt x 120s timeout retry (PAYMENT_ATTEMPT_TIMEOUT_SECS,
   mirrors the mint retry in three_testnet_agents.rs), compiled clean, NOT
   yet committed. ATTEMPT 2 (with fix) FAILED 00:53 (0 passed, exit=101,
   429.28s): setup mint d1c5045e landed block 670 instantly, then 3 payment
   attempts (91873589, 68b9c1be, e9cc8dc7) each printed a hash and timed out
   at the 120s bound; ALL payment txs confirmed null. NET: 5 payments
   submitted tonight across 4h (e4f759ed, 905a91c2, 91873589, 68b9c1be,
   e9cc8dc7), zero included, while same-run mints land in seconds - this is
   the DOCUMENTED testnet behavior ("includes mints but leaves transfers
   un-included for long stretches", TRANSFER_POLL_ATTEMPTS comment), not a
   code bug. Hardening committed LOCAL e758cf8 (not pushed): turns the 25-min
   hang into a bounded 7-min failure. LANE B CLOSED for tonight - morning
   choice: (a) single re-run in a healthier chain window (command ready in
   laneb-retry.sh), or (b) accept posture: F9 anchors fully covered by
   #48/lane A (+ bonus 633-638), paid task evidenced at DEV_MODE=0 on the
   local sequencer (58-min cast, video 2) + video 1; docs never claimed
   public-testnet A2A payment. LESSON: copy stage invocations verbatim from
   the chain script; env precedence in the test (memory backend
   early-return) silently picks a different lane.
5. F10 full-strength identities DONE 00:01 (1 passed, 11753.33s, exit 0):
   storage mint VERIFIED LIVE (tx 9b510890..., block 458, 270,845B, balance
   100); messaging + blockchain mints reported submitted_pending after all
   retries hit parked-connection timeouts - BUT the timed-out attempts
   broadcast and landed: oversized shielded blocks 463 (271,270B), 508
   (272,900B), 511 (271,514B), 558 (544,688B = two mints) in 459-560; no
   hashes captured (timeout killed the command before its hash print), so
   #47's verified mints stay primary for messaging (block 317) / blockchain
   (block 338), these blocks corroborate. Real-service evidence landed:
   storage address zDvZRwzm9WDaKLBS2eg6DWCz4Ww22SbHYzuXFAfbLx4xyRVudMvE (real
   Codex CID), messaging topic /logos-agent/testnet/evidence/3jPB2RCP7...
   message_id 3bb660a10984114c4245be5e098d9ee1fdba0c74625f2c4f3298f2dc3ef22ad7
   (real Waku), evidence block 432.
6. MASTER_CHAIN_DONE 00:01 -> autopush DONE: origin/main = a906e22 (verified
   via ls-remote), CI on it COMPLETED SUCCESS (run 34293206202) at ~01:00.
   e758cf8 (payment hardening) and the morning doc batch rode e9c3d99, pushed
   to origin/main.

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
- Video 2: DONE 2026-09-09. The user recorded the narration and uploaded
  the demo themselves: https://youtu.be/dg6RuNw44a8. Link + reviewer CC tip
  pinned in SOLUTION.md, submission/LP-0008.md, PR #142 body. The
  stretch/mux path (kit repo, retime_vault_notary.py) stays available if a
  re-cut is ever wanted. All narration scripts were stripped from this repo
  2026-09-09; the kit repo aegonmyy/lp0008-video2 is their only home.
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
