# LP-0008 demo — narration script

A read-aloud script for recording the `RISC0_DEV_MODE=0` walkthrough. Written to
be spoken, not read. The long explanations are parked in the spots where the
terminal goes quiet generating a proof, so you always have something to say.

**Stage directions are in brackets. Everything else is what you say.**

---

## Before you hit record

- Warm the build once so the video shows the *run*, not a 10-minute compile:
  `RISC0_DEV_MODE=0 cargo test --no-run` (and make sure Docker is up and `r0vm` is
  installed).
- Start `./scripts/demo.sh`, and begin recording on the first banner.
- Real proofs are slow — each private transaction is a few minutes. You've got
  plenty of talking material below for those gaps; if a proof is still churning
  when you run out, just say "still going — real proofs aren't free," and wait, or
  trim the dead air in editing. What matters is that the proof output is visibly on
  screen, because that's what proves dev-mode is off.

---

## Open — the first banner is on screen

> Hey. So this is the thing I built for LP-0008 — a Logos-native autonomous agent.
> Before anything else, look at the top here: `RISC0_DEV_MODE = 0`. That means
> everything you're about to see is backed by real Groth16 proofs, not the fake
> dev-mode shortcut. So when it feels slow later — that's the point. That's real
> cryptography happening.
>
> The whole idea is an agent that's actually *sovereign*. It owns its own keys, it
> holds its own money, it stores its own files, it talks over its own encrypted
> channels — and none of that runs through me or through some server. Let me just
> run it and I'll explain as it goes.

---

## Scenario 1 — Sovereign wallet

**[SAY as the Scenario 1 banner appears — short]**

> First scenario is the wallet. Watch — it's going to spin up a real local Logos
> execution zone in Docker: a sequencer, an indexer, the whole chain. Then the
> agent gives itself an identity and starts moving money.

**[WHILE THE STACK BOOTS AND THE FIRST PROOFS GENERATE — long cluster, keep talking]**

> Okay, while that's working — let me tell you what "identity" means here, because
> it's the part I'm proudest of. The agent has its own shielded LEZ account. Its
> own keypair. On-chain it's indistinguishable from any other account holder —
> nobody can look at the chain and say "oh, that's a bot." It receives funds like
> anyone, it spends like anyone.
>
> And the second piece is the spending threshold, which is really the heart of the
> safety model. I, as the owner, set a per-transaction limit. Below that line, the
> agent just acts — it signs and submits on its own, no human in the loop. That's
> what you're seeing prove out right now: a transfer under the limit going through
> autonomously.
>
> But above that line, it does *not* spend. It stops, it sends me a request over
> our private channel, and it waits. So the agent is genuinely autonomous for the
> small stuff, and genuinely on a leash for anything big. That's the whole deal —
> useful without being dangerous.
>
> The reason this can even work trustlessly is the Logos stack. On a normal cloud
> setup, an "agent wallet" means a custodian holds the keys — you're trusting a
> company. Here there's no custodian. The agent holds its own keys, the proof
> system enforces the rules, and the chain settles it. Take that away and you're
> back to asking permission from somebody's server.

**[SAY when Scenario 1 finishes — `test result: ok` — short]**

> And there it is — passed. You can see it spent what was under the limit, and it
> *held* the one that was over. Exactly the behaviour I want. On to control.

---

## Scenario 2 — Owner control

**[SAY as the Scenario 2 banner appears — short]**

> This one's the conversation between me and my agent. It's going to propose a
> spend that's over the limit, I approve it; propose another, I deny it; and then
> I raise the limit and watch it start acting on its own again.

**[WHILE IT PROVES — long cluster, keep talking]**

> So the channel these messages go over is Logos Messaging — end-to-end encrypted,
> no central server. The agent and I each derive the same two topics from our
> identities, and that's our private line. I can reach it from any Logos app
> instance that holds my keys. There's no dashboard, no API endpoint sitting
> exposed on the internet. Just an encrypted conversation.
>
> There are two things in here I really sweated. The first is the ordering. When
> the agent wants to spend over the limit, it notifies me *first*, and only holds
> the spend as pending if that message actually got delivered. So if the network's
> down and it can't reach me, it doesn't quietly park a spend that might later go
> through without me ever seeing it. It retries, and if it still can't reach me, it
> refuses. Fails safe, every time.
>
> The second is persistence. Those pending approvals get written to disk. So if the
> node restarts — crash, deploy, network blip — the agent comes back up and the
> spend that was waiting for my "yes" is still waiting. It doesn't lose state and it
> doesn't double-act. That was an explicit requirement, and it's wired into the
> actual deployed binary, not just the tests.
>
> And notice what's happening as I reconfigure — I raise the limit over the same
> encrypted channel, and now a spend that would've been held a minute ago just goes
> through. The policy is live; I can retune how much rope the agent has without
> redeploying anything.

**[SAY when Scenario 2 finishes — short]**

> Approved one, denied one, changed the limit, and it adapted. That's the owner
> relationship in a nutshell.

---

## Scenario 3 — Agent marketplace (A2A)

**[SAY as the Scenario 3 banner appears — short]**

> Last one, and this is where it gets fun. Two separate agents. One offers a
> service for a price, the other discovers it, pays for it, and gets the result —
> and I'm not involved at all. No human anywhere in this loop.

**[WHILE IT PROVES — long cluster, keep talking]**

> This part follows the A2A protocol — Agent2Agent, the open standard the Linux
> Foundation stewards now. Agents publish an "Agent Card" that says who they are and
> what they charge, they discover each other, and they run a task through a defined
> lifecycle: submitted, working, completed. That's all standard A2A.
>
> But A2A deliberately leaves two holes: it doesn't say how agents pay each other,
> and it doesn't say how they talk securely. Those are exactly the two things Logos
> fills for free. The transport is Logos Messaging instead of HTTP — so it's
> encrypted with no server in the middle. And the payment is a real LEZ token
> transfer — which is what's proving right now. The client agent is literally
> paying the provider agent, on-chain, autonomously, as part of accepting the task.
>
> So think about what that unlocks. Any agent built on any A2A-compatible framework
> can discover and call a Logos agent — but the Logos agent brings payment and
> privacy that vanilla A2A just can't offer. And because I built cancellation to
> issue a real refund, the money side is honest in both directions.
>
> Underneath all three of these scenarios is one small idea: skills. Every
> capability — storage, messaging, a wallet send, a program call, an A2A task — is
> just a "skill," one trait with one method. Anyone can add a new skill by
> implementing that trait and registering it. They never touch the core. That's how
> you keep an agent like this open instead of a walled garden.

**[SAY when Scenario 3 finishes — short]**

> Done — two agents, a discovered task, a real payment, settled between them with no
> me in the middle. And the final banner: all scenarios passed, dev-mode zero the
> whole way.

---

## Real testnet evidence

**[Switch to a terminal or browser showing `docs/TESTNET_EVIDENCE.md` and the explorer account page]**

> One more thing, because a local sequencer is one thing and the real network is
> another. This is the *official* public Logos testnet. This is the agent's own
> account on the explorer — and you can see it holds a hundred tokens it minted to
> itself, with a real proof, on-chain. Here's the transaction hash.
>
> And I'll be honest about what it took, because it's a good story. The testnet
> upgraded to a new version mid-way through this. My client could still *read* the
> chain fine, but its transactions kept getting accepted and then never included —
> they just vanished into the mempool. Took me a while to work out that the
> transaction format had changed in the upgrade. I matched my client to the
> testnet's version, and the exact same transaction went straight in. That account,
> that balance — that's the proof it's real.

---

## Close

> So that's it. A sovereign agent, on Logos, end to end: its own shielded identity
> and funds, a spending leash I control over an encrypted channel, agent-to-agent
> coordination that pays its own way, and every bit of it backed by real proofs —
> including a real transaction on the live testnet. No custodian, no server, no
> host. That's the whole reason to build this on Logos instead of anywhere else.
> Thanks for watching.
