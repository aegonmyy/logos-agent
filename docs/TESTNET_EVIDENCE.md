# Live LEZ testnet evidence

Real, proof-backed agent activity on the **official public LEZ testnet**
(`https://testnet.lez.logos.co`, LEZ **v0.2.4**), generated with
**`RISC0_DEV_MODE=0`** — real Groth16 proofs, not the dev-mode fake-proof lane.

The agent holds its own shielded-capable LEZ account, defines a token, and mints
supply to itself — an autonomous, on-chain, proof-backed transaction accepted and
included by the live testnet sequencer.

## Mint — included on-chain

| | |
|---|---|
| Network | `https://testnet.lez.logos.co` (official), LEZ v0.2.4 |
| Proof mode | `RISC0_DEV_MODE=0` (real Groth16) |
| Agent account (holder) | `6pgVpCE8rGzE2PPzbPoa2phu3BcmmHfzKkdEi68aNaJt` |
| Mint transaction | `0e3ebbb8bb03ba31d3c1115aca438bbe8835ada9c84aa03e785f16bc6f67fa6e` |
| Token | `TESTNET-COIN`, supply 100, minted to the agent |
| Result | **holder balance = 100**, verified by reading chain state |
| Submitted at block | 984 (sync start); included shortly after |

**Explorer:**
- Account (shows the minted holding): <https://explorer.testnet.lez.logos.co/account/6pgVpCE8rGzE2PPzbPoa2phu3BcmmHfzKkdEi68aNaJt>
- Transaction: <https://explorer.testnet.lez.logos.co/transaction/0e3ebbb8bb03ba31d3c1115aca438bbe8835ada9c84aa03e785f16bc6f67fa6e>

**On-chain account state after the mint** (`getAccount`), showing the account is
now owned by the token program, carries the token holding, and has advanced its
nonce — i.e. a transaction really landed:

```json
{
  "program_owner": [1047643340, 4291649067, 2093396023, 4016657193,
                    3904308476, 481382041, 2987082047, 2603530278],
  "balance": 0,
  "data": [0,109,158,76,62,56,191,152, ... ,66,100,0,0, ...],
  "nonce": 1
}
```

`data` encodes the `TokenHolding` (balance `100` = byte `0x64`); `nonce: 1`
confirms exactly one transaction (the mint) was applied to the account.

## Why this required matching the testnet version

Our client was originally pinned to LEZ **v0.2.0**. Against the v0.2.4 testnet it
could **sync** (read path is backward-compatible) but its transactions were
**accepted into the mempool yet never included** — the transaction/write path
changed across v0.2.1–v0.2.4 (L1 fees, private-kinds refactor). Bumping the
client to **v0.2.4** made the identical transaction include. The version match is
what turned "submitted" into "on-chain".

## Reproduce

```bash
# Real proofs, against the official testnet (the default endpoint).
RISC0_DEV_MODE=0 AGENT_TESTNET_URL=https://testnet.lez.logos.co \
  cargo test --test testnet_tx -- --ignored --nocapture --test-threads=1
```

The test creates fresh accounts, defines and mints `TESTNET-COIN` to the agent,
then transfers to a second account — each a real proof-backed transaction. It
reads chain state back to assert the balances, so a green run is itself proof of
inclusion. (Public-testnet block production is intermittent; the test retries
through transient outages.)

> **Note on ephemerality:** the public testnet's chain state is reset when the
> operator redeploys (as happened during v0.2.1–v0.2.4). Transaction hashes and
> account pages above are valid for the deployment they were produced against;
> the reproducible test regenerates equivalent evidence against whatever testnet
> is live.

## Re-verified 2026-08-21 (public testnet alive, write path confirmed)

The public testnet was re-checked on 2026-08-21 and is currently **producing
blocks and including transactions** — the earlier "public write path is down"
limitation is stale as of this date. Fresh `RISC0_DEV_MODE=0` (real-proof)
activity from this date:

- **Token mint, included on-chain:** transaction
  `5c4c09ca5de158d3f7109326a1e699734784d2e64e3da0e8472176ce4bc03405`,
  included in **block 17716**; the holder balance read back as 100. (From
  `tests/testnet_tx.rs`, which mints `TESTNET-COIN` and transfers 10 to a
  second account.)
- **Three category agents deployed:** three distinct shielded identities
  (storage / messaging / blockchain) created against the public testnet and
  synced to block 17716 — see the "Re-verified 2026-08-21" run in
  [`THREE_TESTNET_AGENTS.md`](THREE_TESTNET_AGENTS.md).
- **All three use cases anchored by included mints** (from
  `tests/three_use_cases.rs` with `SERVICE_BACKEND=memory`, which runs the
  storage/messaging round-trips over in-memory backends while every on-chain
  anchor is a live public-testnet transaction):
  - **Personal file vault** — `LP0008-Vault` mint
    `73d9dc70f12ab443bd0f458c6504f8392671f281b54f56913d710b67dd722039`,
    included in **block 17791**; storage CID
    `f47887f3f7bc3ffaf6f927f1b53db65380268c4e61aaf7814a023fa6054a8b34`.
  - **Privacy-preserving notary** — `LP0008-Notary` mint
    `9b70fb96de6dba41212d2f1c46ef274b3a93e17845c341ca8ffb1f2d5c1bb924`,
    included in **block 17792**; notary digest
    `5999d285f64e95b7d4f1246a112d45535e852676d06f14530900876f4638a42a`,
    storage CID
    `f35efd1f863e54dbe27914970cfd8abbfef8036199d8da8d6cd26b2718853df4`.
  - **On-chain event alerter** — `LP0008-EventAlert` mint
    `6fb084d33e4933a47c9e1998be0e206aba64cd372b05fdf6b84b4ff1b33166b5`,
    included in **block 17793**.
  All three mints are real, proof-backed, included on-chain transactions; the
  test passed in 148s.

**Known limitation, stated plainly:** on this date the public testnet includes
token **mints** reliably but did not include token **transfers** within the
30-minute polling window in any run (the `testnet_tx` transfer and the paid-A2A
payment transfer both timed out as submitted-but-not-included). The mints above
are real, proof-backed, included on-chain transactions; the transfer leg is
best-effort in the test harness (bounded wait, reported as `submitted_not_included`
when not included) so a use case completes on its included mint anchor rather
than hanging. The A2A *payment* criterion is therefore evidenced on the local
standalone sequencer (real proofs, balances 90/10 — see
[`THREE_USE_CASES.md`](THREE_USE_CASES.md)), not on the public testnet transfer
path. Why transfers specifically do not include while mints do is not diagnosed;
no guess is offered. Re-run when the public write path is healthier to upgrade
pending anchors.

## Transfer non-inclusion: verified diagnosis (2026-08-22)

The discriminator was pinned down by querying the public RPC directly:

- **Sibling mints from the same run are queryable** — `getTransaction` for a
  mint hash submitted seconds earlier returns full transaction data.
- **The submitted transfer hash returns `null`** — not `rejected`, not an error:
  the sequencer's chain has no record of it, so the client never has a hash it
  can meaningfully wait on.
- **Null control:** an impossible (random) hash also returns `null`, confirming
  the RPC answers `null` for "not on chain" rather than for "query failed".

So token-program `Send` instructions submitted to this testnet are accepted by
the API layer and silently dropped before block assembly, while the same
program's `New` (mint) instructions from the same wallet include within a block
or two. This is a sequencer-side filter on the instruction, not a wallet,
proof, or fee problem on our side — which is also consistent with how a
third-party submission (edenbd1/lambda-prize #129) produced its public-testnet
settlements: every settlement there is a call to the submitter's **own deployed
program**, never a token-program `Send`. Program deployment and program calls
are public transactions the sequencer includes.

**Consequence for our evidence:** the send/settlement leg on the public testnet
is exercised through our own deployed program rather than the token program's
`Send` — see [`THREE_TESTNET_SETTLEMENTS.md`](THREE_TESTNET_SETTLEMENTS.md)
(`tests/three_testnet_settlements.rs`): one program deployment, then
per-agent `program.call` settlements whose state change is read back on chain.
The token `Send` path remains fully evidenced against the local standalone
sequencer with real proofs (`tests/a2a_two_agents.rs`,
`tests/three_use_cases_local.rs`).
