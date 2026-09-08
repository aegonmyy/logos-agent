# Compute-unit (CU) cost of the agent's on-chain operations

CU cost on LEZ is the RISC0 **executor user-cycle count** for a program
instruction. Cycles are a property of execution and are independent of
`RISC0_DEV_MODE` (the executor runs whether or not a Groth16 proof is produced),
so these figures hold for the real-proof path.

Measured with the platform's `cycle_bench` tool:

```bash
cargo run --release -p cycle_bench -- --exec-iters 3
```

The figures are measured against a standalone stack running the same platform
release (`v0.2.4`) as the public testnet: the executor is the same software, so
its user-cycle counts are the CU the operation costs on devnet/testnet.
(Chain receipts and RPC responses do not report consumed CU, and the node's
metrics endpoint does not export per-transaction cycles, so direct executor
measurement is the method — this is also how the platform itself benchmarks.)

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

### Program calls (`program.call`)

A `program.call` dispatch executes the called program's instruction under the
same executor, so its CU cost is that program's own instruction cost — CU is
metered per executed instruction, whatever program defines it. The built-in
table above calibrates the scale (LEZ's own programs run 44k-643k CU); a
program author measures their own program with the same executor measurement
`cycle_bench` uses before relying on a budget. The sample claimer program the
agent deploys and settles through in
[`THREE_TESTNET_SETTLEMENTS.md`](THREE_TESTNET_SETTLEMENTS.md) (program id
`937554f7…f4ea471`) is a fixture of the platform's `test_programs` crate; its
instruction is a state write comparable to the initialized programs above, and
the whole settle path (three program-mediated settlements, blocks 148-150 of
the public testnet) executed within ordinary block budgets.

### Deployments (`program.deploy`)

Deploying a program is not compute-metered execution: the deployment
transaction carries the program bytecode for the state machine to register,
and no instruction executes, so there are no executor user-cycles to count.
Its on-chain cost is transaction and block **size**, not CU — observed
directly on the public testnet, where the claimer deployment was a
`ProgramDeployment` transaction carrying 343,392 bytes of bytecode. Block-size
limits (what the platform's `block_size_limit` tests exercise with this same
fixture) are the operative constraint for deployments, not the 32M compute
budget.

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
