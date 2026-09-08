# A2A transport binding over Logos Messaging

LP-0008 requires agent-to-agent coordination to be A2A-compatible: "Agent Cards
follow the A2A schema, task interactions follow the A2A task lifecycle, and the
implementation is documented as an A2A transport binding over Logos Messaging."
This document is that binding spec.

The [A2A protocol](https://a2a-protocol.org/latest/specification/) defines
discovery (Agent Cards), task negotiation (the task lifecycle), and messaging
between agents. It deliberately leaves two things to the implementer:
transport security and payment. This binding replaces A2A's HTTPS/JSON-RPC
transport with Logos Messaging (Waku content topics, the Logos Dev Network
cluster 2) and fills the payment gap with LEZ transfers between the agents'
own shielded accounts. Everything else keeps the A2A names and shapes.

## Concept mapping

| A2A concept | A2A wire form | This binding | Implementation |
|---|---|---|---|
| Agent Card discovery | `GET /.well-known/agent.json` | cards are published to and read from a shared discovery content topic | `A2aProvider::publish_card`, `A2aClient::discover` |
| Card service `url` | HTTPS base URL | `address` is the provider's inbox topic: `/logos-agent/1/a2a-<account>-inbox/proto` | `AgentCard.address` |
| `message/send` (task request) | JSON-RPC POST | `task_request` JSON published to the provider's inbox topic | `A2aClient::run_task` |
| Task status / `getTask` | JSON-RPC poll | `task_update` messages on the provider's updates topic `/logos-agent/1/a2a-<account>-updates/proto`, consumed by `agent.subscribe` | `A2aProvider::publish_update`, `A2aClient::poll_task` |
| Streaming (SSE) | `capabilities.streaming` | the card declares `streaming: false`: updates arrive as messages on the updates topic, polled by the client, so the card does not claim an SSE transport it does not provide | `Capabilities` |
| `tasks/cancel` | JSON-RPC POST | `task_cancel` JSON on the provider's inbox topic; the provider refunds and marks the task `canceled` | `A2aClient::cancel`, `A2aProvider::serve_pending` |
| Payment | undefined in A2A | `priceLez` per advertised skill; the client pays the provider's `lezAccount` (its own shielded account) on submission, through its own spending threshold; cancel refunds in full | `run_task`, `refund_task` |

## Agent Card schema

The card is the A2A AgentCard with the transport field substituted and two
Logos-native extensions:

```json
{
  "protocolVersion": "0.2.5",
  "name": "greeter-agent",
  "description": "Logos-native A2A agent",
  "version": "1.0.0",
  "address": "/logos-agent/1/a2a-<account>-inbox/proto",
  "lezAccount": "<the provider's shielded LEZ account, where payments go>",
  "capabilities": { "streaming": false },
  "skills": [
    { "id": "demo.echo", "name": "demo.echo",
      "description": "Echo the input back.", "priceLez": "10" }
  ],
  "signingPubkey": "<hex Ed25519 verifying key>",
  "signature": "<hex Ed25519 signature over this JSON with `signature` absent>"
}
```

- `address` replaces A2A's `url` (the transport substitution above).
- `priceLez` / `lezAccount` are the payment extension: `priceLez` is a string
  to stay exact for u128 token units.
- `signingPubkey` + `signature` make the card a signed document: an Ed25519
  signature over the card's canonical JSON (the same document with the
  `signature` field absent). `AgentCard::verify` rejects tampered cards (a
  changed price, a swapped payment account). The signing key is
  self-certifying because a shielded LEZ account's keys produce ZK proofs,
  not message signatures; pin the key once over a trusted channel (for
  example the owner channel), then later cards under the same
  `signingPubkey` verify. `A2aProvider::new_with_signing_key` keeps the key
  stable across redeploys.

## Task lifecycle

Tasks move through the A2A states, carried as the kebab-case wire names:

```
submitted -> working -> completed
                    \-> failed
cancel (before serving) -> canceled
```

The client creates the task in `submitted` when it publishes the request; the
provider publishes `working` when it starts the skill, then `completed` with
the skill's JSON result or `failed` with the error. A `task_cancel` that
arrives before the task is served ends it in `canceled` and refunds the
payment. `input-required` is modelled by `TaskState::InputRequired`; no
default skill currently pauses mid-task for more input, so the provider path
completes or fails every task it serves.

## Wire messages

Task request (client to provider, on the card's `address` topic):

```json
{ "kind": "task_request", "taskId": "task-0", "skill": "demo.echo",
  "params": { "text": "hello" }, "from": "<client account>",
  "priceLez": "10" }
```

Task update (provider to subscribers, on the updates topic):

```json
{ "kind": "task_update", "taskId": "task-0", "state": "completed",
  "result": { "echo": "hello" } }
```

Cancel (client to provider, on the inbox topic):

```json
{ "kind": "task_cancel", "taskId": "task-0" }
```

The provider serves its inbox with `A2aProvider::serve_pending`, which
dispatches each requested skill through the provider's own `SkillRegistry`,
so any registered skill can be sold.

## Payment semantics

- The client pays the declared price to the card's `lezAccount` **before**
  submitting the request, through `Agent::send`, so its own spending
  threshold applies: a task priced above the client's limit is not paid and
  not submitted (the error names the limit).
- A cancel that reaches the provider before the task is served refunds the
  exact payment to the account named in `from`. The refund uses
  `Agent::send_approved`: returning money the client already paid is not a
  spend, so the spending policy does not gate it.
- On networks where the shielded sender proof is unavailable,
  `A2aClient::run_task_with_payment` submits a task whose payment settled in
  an externally verified transaction (`paymentTx` names it).

## Where this is tested

- `tests/a2a_two_agents.rs` (CI): two agents discover each other from
  published cards, run a task `submitted -> completed`, and the payment
  settles autonomously on chain (client 100 -> 90, provider 0 -> 10, no owner
  in the loop); a second test cancels before serving and asserts the refund
  reverses the transfer exactly (client back to 100, provider back to 0); a
  third restarts the client and asserts its task ledger survives.
- Unit tests in `src/a2a.rs`: a card signs and verifies; a tampered price
  fails verification; a card round-trips through discovery and still
  verifies; a persisted signing key keeps `signingPubkey` stable across
  redeploys; a failing skill is isolated and its neighbour task still
  completes.
- The skill surface (`agent.card`, `agent.discover`, `agent.task`,
  `agent.subscribe`, `agent.cancel`) drives this same API by name; see
  `tests/full_skill_catalogue.rs`.
- `tests/a2a_two_agents_waku.rs` (ignored; needs an nwaku node on
  `127.0.0.1:8645` with `--cluster-id=2` plus the local sequencer) is the
  real-transport companion: the same discovery, task lifecycle, and payment
  with every message carried by a live nwaku node. The provider is named
  uniquely per run and the client selects that signed card, so a node whose
  discovery topic buffers cards from earlier runs cannot confuse it.

Reproduce with:

```bash
RISC0_DEV_MODE=1 cargo test -p logos_agent --test a2a_two_agents          # CI form
docker run -d --name nwaku -p 8645:8645 wakuorg/nwaku:v0.38.0 \
  --rest=true --rest-address=0.0.0.0 --rest-port=8645 --relay=true --cluster-id=2
RISC0_DEV_MODE=0 cargo test --test a2a_two_agents_waku -- --ignored --nocapture
```
