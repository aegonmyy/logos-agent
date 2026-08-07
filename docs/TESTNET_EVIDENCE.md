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
