# Three agents, one per skill category

LP-0008 asks for three separate agents deployed — one each for the Storage,
Messaging, and Blockchain skill categories — with reproducible deployment and
evidence. The same `agent` binary deploys all three; they differ only in the
skills their owner enables and the role they play.

## Deployment

Each agent is a single command against a running local sequencer (see
`scripts/demo.sh` for bringing the stack up) and a Logos Messaging node:

```bash
# 1. Storage agent — a personal file vault.
agent --owner "$OWNER" --spending-limit 0 \
      --messaging-url http://127.0.0.1:8645
#     exercises: storage.upload / download / list / share

# 2. Messaging agent — a group coordinator.
agent --owner "$OWNER" --spending-limit 0 \
      --messaging-url http://127.0.0.1:8645
#     exercises: messaging.send / join / create_group

# 3. Blockchain agent — holds and moves funds under policy.
agent --owner "$OWNER" --spending-limit 50 \
      --messaging-url http://127.0.0.1:8645
#     exercises: wallet.balance / send, and pays for A2A tasks
```

Each agent creates its **own** shielded LEZ account on deploy (printed as
`agent account: <id>`), so the three are independent identities on-chain.

## Evidence

Each category is exercised end-to-end by an integration test that runs against
the live local sequencer:

| Agent (category) | Demonstrated by | What it proves |
|---|---|---|
| Storage | `storage_messaging_skills` | encrypt → upload → list → download round-trip |
| Messaging | `storage_messaging_skills` | create group, join, deliver a message |
| Blockchain | `agent_spending`, `a2a_two_agents` | shielded balance, policy-gated send, paid task |

The Blockchain agent's flow is additionally proven with **real proofs**
(`RISC0_DEV_MODE=0`) in [`DEV_MODE_0_EVIDENCE.md`](DEV_MODE_0_EVIDENCE.md).

> Note on testnet: the public LEZ testnet was reset for v0.2.0. As with our
> LP-0017 submission, deployments are demonstrated against a real local sequencer
> at `RISC0_DEV_MODE=0`; the deployment steps above are identical for the public
> testnet once its endpoint is configured in the wallet environment.
