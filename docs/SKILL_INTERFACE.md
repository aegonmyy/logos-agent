# Skill interface specification

This is the LP-0008 skill interface spec: the contract every agent skill
implements, the full default catalogue (21 skills), the dispatch semantics, and
how third-party skills are added without modifying the agent core. The same
contract backs the "documented skill interface (module/SDK)" requirement.

The spec is enforced, not just written:

- `meta.skills` returns this catalogue in machine-readable form at runtime
  (name, description, params with required/optional flags) — run it anywhere
  and compare against this document.
- `tests/full_skill_catalogue.rs` runs in CI and pins all 21 skill names plus
  the parameter behaviors documented here (including the two threshold-gated
  skills), so the doc and the code cannot drift apart silently.

## The contract

A skill is a named, documented capability. It receives JSON arguments and
returns a JSON value; failures are errors, not result codes.

```rust
pub struct ParamSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

#[async_trait(?Send)]
pub trait Skill: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn params(&self) -> Vec<ParamSpec> { Vec::new() }
    async fn invoke(&self, ctx: &mut SkillContext<'_>, args: Value) -> Result<Value>;
}
```

`SkillContext` carries what the skill may touch: the agent (identity, policy,
history) and, only when the host provides one, the wallet. Storage and
messaging skills never see the wallet; blockchain skills require it and fail
with a clear error when it is absent.

## Registration

Skills live in a `SkillRegistry`. Four registration paths:

| Call | Registers |
|---|---|
| `SkillRegistry::with_defaults()` | the six blockchain skills (`wallet.balance/send/history`, `program.query/call/deploy`) |
| `register_storage(Arc<dyn Storage>)` | the four storage skills, bound to that backend |
| `register_messaging(Arc<dyn Messaging>)` | the three messaging skills, bound to that backend |
| `register(Box<dyn Skill>)` | any custom skill |

The reflective `meta.*` skills are built into dispatch and always available;
the five `agent.*` A2A skills are listed by the catalogue and carried by the
A2A client/provider session (below). Registering a skill whose name already
exists shadows the earlier one (last registration wins), so a deployment can
override a default with its own implementation without touching the core.

Backends are traits too (`Storage`, `Messaging` in `src/storage.rs` /
`src/messaging.rs`): Codex and in-memory storage, Waku and in-memory messaging.
A deployment binds the skills to whichever backend it constructs.

## Dispatch semantics

`registry.dispatch(name, ctx, args)` looks up the skill by name, enforces the
declared contract, and awaits its `invoke`. Arguments are a JSON object;
string, number, or array values are accepted where the table below says so.
Unknown skill names are errors, and so is a dispatch that omits a parameter
the skill declares required (or passes it as null): the registry rejects it
before the skill runs, from the `ParamSpec` list alone. Enforcement therefore
does not depend on the skill author remembering to validate their own
arguments — declaring `ParamSpec::required` is what makes the argument
required. A skill that returns `Err` is a failed invocation; the caller
(owner channel, A2A provider, CLI) decides how to surface it. A failing skill
never aborts the registry: the next dispatch starts clean (pinned by
`a2a::tests::failing_skill_is_isolated_and_does_not_affect_other_tasks`,
which runs a failing task and a working task through the same provider round).

### The spending gate

Two skills move value and therefore pass through the agent's spending policy
before submission:

- `wallet.send` prices itself (the `amount` argument) against the policy.
- `program.call` cannot price itself: the instruction stream is opaque, so the
  caller declares `spend`, the token value the call may move on the agent's
  behalf. A declared spend is checked against the policy and recorded against
  the period allowance when the call submits; an undeclared call is unpriced,
  so it is always held.

Both return `{"status": "needs_owner_approval", ...}` instead of submitting
when the policy says hold. The boundary is strict: a spend exactly at the
per-tx limit executes autonomously, one token more holds (pinned by
`per_tx_boundary_at_the_limit_is_autonomous`).

## The default catalogue

### Storage (4)

| Skill | Params | Returns |
|---|---|---|
| `storage.upload` | `label`* (human label), `data`* (contents) | `{"address": "<content address>"}` |
| `storage.download` | `address`* (content address) | `{"data": "<contents>"}` |
| `storage.list` | (none) | `{"objects": [{"label", "address"}, ...]}` |
| `storage.share` | `address`*, `recipient`* (Logos identity) | `{"status": "shared", ...}` |

Uploads are encrypted client-side before they reach the backend; the address
is the content address of the stored object.

### Messaging (3)

| Skill | Params | Returns |
|---|---|---|
| `messaging.send` | `to`* (address or group topic), `message`* | `{"message_id": "..."}` |
| `messaging.join` | `group_id`* | `{"status": "joined", "group_id": ...}` |
| `messaging.create_group` | `members`* (array of identities) | `{"group_id": "..."}` |

Group ids are `/logos-agent/1/group-...` content topics.

### Blockchain (6)

| Skill | Params | Returns |
|---|---|---|
| `wallet.balance` | `token`* (definition account id) | `{"token", "balance"}` (syncs first) |
| `wallet.send` | `to`*, `amount`* | `{"status": "executed", ...}` or `{"status": "needs_owner_approval", "limit", ...}` |
| `wallet.history` | (none) | `{"transactions": [{"to", "amount"}, ...]}` |
| `program.query` | `account`* (program-owned account id) | `{"account", "state": <account>}` |
| `program.call` | `program_id`* (64 hex), `accounts`* (array of public account ids), `instruction`* (array of u32 words), `spend` (optional, token amount) | tx result or `{"status": "needs_owner_approval", "declared_spend", "limit"}` |
| `program.deploy` | `binary_path`* (compiled program ELF) | `{"program_id": "<64 hex>"}` (deterministic image id) |

### Meta (3)

| Skill | Params | Returns |
|---|---|---|
| `meta.skills` | (none) | the catalogue (this document's tables, machine-readable) |
| `meta.status` | (none) | `{"account_id", "per_tx_limit", "per_period_limit", "period_seconds", "skill_count"}` |
| `meta.configure` | `key`* (`per_tx_limit` / `per_period_limit` / `period_seconds`), `value`* | `{"status": "configured", <key>}` |

`meta.configure` is how the owner widens or narrows the agent's autonomy at
runtime; through the owner channel it is the same call the Basecamp UI makes.

### A2A (5)

| Skill | Backing operation |
|---|---|
| `agent.card` | `A2aProvider::card` / `publish_card`: this agent's signed Agent Card |
| `agent.discover` | `A2aClient::discover`: cards published on a discovery topic |
| `agent.task` | `A2aClient::run_task`: pay the card's price and submit a task |
| `agent.subscribe` | `A2aClient::poll_task`: read `task_update` messages |
| `agent.cancel` | `A2aClient::cancel`: cancel before serving; the provider refunds |

These carry A2A session state (cards, task ids), so they are driven through
the A2A client/provider rather than plain JSON dispatch; the catalogue marks
them `requires_stateful_a2a_context`. Every operation is exercised end to end
in `tests/a2a_two_agents.rs` (CI, in-memory) and `tests/a2a_two_agents_waku.rs`
(live nwaku). The wire protocol, card schema, and payment semantics are
specified in [`A2A_BINDING.md`](A2A_BINDING.md).

## Adding a skill (third-party path)

Implement the trait, register it. The core is not modified. The repo's
`EchoSkill` is the worked example (`src/skills.rs`):

```rust
impl Skill for EchoSkill {
    fn name(&self) -> &'static str { "demo.echo" }
    fn description(&self) -> &'static str { "Echo back the provided text." }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::required("text", "Text to echo back.")]
    }
    async fn invoke(&self, _ctx: &mut SkillContext<'_>, args: Value) -> Result<Value> {
        let text = arg_str(&args, "text")?;
        Ok(json!({ "echo": text }))
    }
}

registry.register(Box::new(EchoSkill));
```

After registration, the skill is first-class: it appears in `meta.skills`, it
is dispatchable by name from the owner channel and every other caller, and (on
the provider side) it can be advertised with a LEZ price on an Agent Card and
sold through A2A (`A2aProvider::serve_pending` dispatches requested skills
through the same registry).

## Where this is tested

`tests/full_skill_catalogue.rs` (CI) pins all 21 names, the meta skills, the
threshold holds on `program.call`, and the storage/messaging round-trips.
`tests/skills_dispatch.rs` covers dispatch error paths.
`tests/three_category_agents.rs` runs one agent per category exercising its
category's skills end to end. `tests/third_party_skill.rs` is the out-of-crate
proof of this document's third-party claims: two skills defined entirely in
the test crate, one registered and dispatched and listed by `meta.skills`
with its param spec intact (including rejection of a missing required
argument at dispatch), one shadowing `storage.list` over a default registry —
compiled against the public API only, no core modification.
