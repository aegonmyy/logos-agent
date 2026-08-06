# Compute-unit (CU) cost of the agent's on-chain operations

CU cost on LEZ is the RISC0 **executor user-cycle count** for a program
instruction. Cycles are a property of execution and are independent of
`RISC0_DEV_MODE` (the executor runs whether or not a Groth16 proof is produced),
so these figures hold for the real-proof path.

Measured with the platform's `cycle_bench` tool:

```bash
cargo run --release -p cycle_bench -- --exec-iters 3
```

## Operations the agent performs

| Operation | Program · instruction | User-cycles (CU) |
|---|---|---:|
| Pay for a task / `wallet.send` | `token` · Transfer | 127,726 |
| Fund an account (token issuance) | `token` · Mint | 116,862 |
| Burn tokens | `token` · Burn | 116,546 |
| Native authorized transfer | `authenticated_transfer` · Transfer | 79,958 |
| Account initialization | `authenticated_transfer` · Initialize | 43,818 |
| Create associated token account | `associated_token_account` · Create | 174,995 |

For reference, other built-in programs measured on the same run: `amm`
SwapExactInput 508,679 and AddLiquidity 643,059; `clock` Tick 137,022.

## Budget context

The public-execution compute budget is **33,554,432 cycles** (32M,
`MAX_NUM_CYCLES_PUBLIC_EXECUTION` in `lee/state_machine`). A token Transfer at
~128k cycles uses ~0.4% of that budget, so an agent's spend and A2A payment
operations sit comfortably within limits.

## Notes

- Figures are the **public-execution** cost. When an operation runs inside a
  privacy-preserving transaction (the agent's default, for shielded accounts),
  the executor cost is composed with the PPE circuit; the transfer instruction
  cost above is the dominant application-level term.
- Per the prize spec, LEZ's per-transaction compute budget may change during
  testnet; re-run `cycle_bench` against the target version to refresh these
  numbers.
- Full machine-readable output: `target/cycle_bench.json`.
