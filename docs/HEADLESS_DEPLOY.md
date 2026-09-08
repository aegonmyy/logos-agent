# Headless deploy: one command, any machine (LP-0008)

LP-0008: *"The owner can deploy the agent and configure it with a single CLI
command on any machine using Logos Core headless."*

This is a recorded run of exactly that, against the **official public LEZ
testnet** (`https://testnet.lez.logos.co`) on 2026-09-08. The `agent` binary is
headless (no GUI, no Logos app); it runs anywhere the crate builds and a wallet
directory exists.

## The commands

```bash
# 0. Build (any machine with the Rust + RISC0 toolchains):
cargo build --bins          # -> agent, agent-wallet-init

# 1. Once per machine: create the wallet home the agent deploys from
#    (owners who already have an LEE wallet skip this step):
export LEE_WALLET_HOME_DIR=/tmp/agent-demo/wallet
agent-wallet-init --sequencer https://testnet.lez.logos.co

# 2. THE single deploy + configure command:
agent --owner demo-owner \
      --spending-limit 50 \
      --period-limit 500 \
      --period-seconds 86400 \
      --messaging-url http://127.0.0.1:8645 \
      --state-file /tmp/agent-demo/agent-state.json
```

Every flag of step 2 also reads from an environment variable
(`AGENT_OWNER`, `AGENT_SPENDING_LIMIT`, `AGENT_PERIOD_LIMIT`,
`AGENT_PERIOD_SECONDS`, `AGENT_MESSAGING_URL`, `AGENT_STATE_FILE`,
`AGENT_ACCOUNT_ID`), so configuration is one line in either form. The
messaging URL is any nwaku REST endpoint on the Logos Dev Network (cluster 2);
this run used a local `wakuorg/nwaku:v0.38.0` container.

## Transcript 1: wallet init (step 1)

```
$ agent-wallet-init --sequencer https://testnet.lez.logos.co
Config not found, setting up default config
Created configs dir at path /tmp/agent-demo/wallet
Configs set up
Statistics not found, choosing empty
Stored persistent accounts at /tmp/agent-demo/wallet/storage.json
wallet home: /tmp/agent-demo/wallet
sequencer:   https://testnet.lez.logos.co

Recovery phrase (store securely; the agent's keys live in this wallet):
  <24 words>

Wallet ready. Deploy an agent from it with:
  agent --owner <owner-id> --spending-limit 50 \
        --messaging-url http://127.0.0.1:8645
```

## Transcript 2: the deploy command (step 2, first start)

```
$ agent --owner demo-owner --spending-limit 50 --period-limit 500 \
        --messaging-url http://127.0.0.1:8645 \
        --state-file /tmp/agent-demo/agent-state.json
Statistics not found, choosing empty
Generated new account with account_id Private/ALcBJ9yFMbiQy8goauUrvyPyqi9W6iJBQhmMVUw375Fz at path /0
Stored persistent accounts at /tmp/agent-demo/wallet/storage.json
agent account: ALcBJ9yFMbiQy8goauUrvyPyqi9W6iJBQhmMVUw375Fz
agent deployed; awaiting owner instructions (tx limit 50, period limit 500, state /tmp/agent-demo/agent-state.json).
Latest block is 42
Syncing to block 42. Blocks to sync: 42
Synced to block 42 in ...
[Ctrl-C]      # we stopped it after 30 s; it runs until stopped
```

One command produced: the agent's **own shielded account** (`ALcBJ9…`), the
**owner configuration** (`demo-owner`), the **spending policy** (per-tx 50,
per-period 500 per 86400 s), the **owner channel** over Logos Messaging
(nwaku), and the running event loop against the public testnet.

## Transcript 3: restart is the same agent (identity + state persist)

Stop the process, rerun the identical command:

```
$ agent --owner demo-owner --spending-limit 50 --period-limit 500 \
        --messaging-url http://127.0.0.1:8645 \
        --state-file /tmp/agent-demo/agent-state.json
restoring agent identity from /tmp/agent-demo/agent-state.json: ALcBJ9yFMbiQy8goauUrvyPyqi9W6iJBQhmMVUw375Fz
agent account: ALcBJ9yFMbiQy8goauUrvyPyqi9W6iJBQhmMVUw375Fz
agent deployed; awaiting owner instructions (tx limit 50, period limit 500, state /tmp/agent-demo/agent-state.json).
Latest block is 42
...
```

The identity is durable: the account id is persisted in the state file and
restored on start (an explicit `--account <id>` pins it instead). The owner
channel topics derive from the account id, so a restarted agent listens where
the owner already sends; pending approvals and the per-period accumulator are
restored from the same file. A restart mints a new identity only when the
state file is absent.

## What the event loop survives

The loop treats a transient sequencer or nwaku failure as recoverable: wallet
sync errors are already tolerated per cycle, and an owner-channel error is
reported and retried on the next poll instead of exiting. The messaging layer
subscribes to a topic on demand before reading it (nwaku's REST relay only
buffers subscribed content topics), so a fresh deploy that polls before any
message was sent reads an empty inbox rather than erroring.

## Logos Core headless, the other reading

Deploying via the Logos Core headless runtime also works: the module form of
the agent loads in `logoscore` alongside the stock modules, with the runtime
evidence in [`LOGOS_CORE_LOADED.md`](LOGOS_CORE_LOADED.md). The `agent` binary
is the same Rust core linked headless, for owners who want the agent without a
Logos Core instance.
