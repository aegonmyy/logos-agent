# Real-node and real-testnet evidence

Beyond the in-memory backends used for fast deterministic tests, the Messaging
backend and the wallet are exercised against real infrastructure.

## Messaging against a real nwaku node

`tests/waku_live.rs` (run with `--ignored`) publishes a message to a running
nwaku node and reads it back:

```
topic: /logos-agent/1/group-8bb490f308f8cb06/proto
published message id: c699fb6a...
received 1 message(s)
test result: ok. 1 passed
```

Bring up nwaku (e.g. the delivery docker-compose) on `127.0.0.1:8645`, then:
`cargo test --test waku_live -- --ignored --nocapture`.

## Wallet against the live public LEZ testnet

`tests/testnet_live.rs` points an agent wallet at the public LEZ testnet
sequencer and reads its chain height:

```
Latest block is 3
Synced to block 3 in 108ms
live testnet latest block id: 3
test result: ok. 1 passed
```

The sequencer endpoint used is `https://seq-testnet.paradox.computer` (LEZ
v0.2.0). The public testnet is intermittently available; when it is up, the agent
wallet reaches it and reads real chain state.
