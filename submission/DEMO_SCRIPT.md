# LP-0008 demo — narration script

A read-aloud script for the `RISC0_DEV_MODE=0` walkthrough. Written to be
spoken, not read. The long explanations are parked in the spots where the
terminal goes quiet generating a proof, so you always have something to say.

**Stage directions are in brackets. Everything else is what you say.**

This script matches `recordings/demo-narratable.mp4` (12 min 24 s) exactly.
The video shows three scenarios against a local Docker LEZ sequencer, then
ends at the "Demo complete" banner. The public-testnet evidence is documented
in `docs/TESTNET_EVIDENCE.md` and `docs/THREE_TESTNET_SETTLEMENTS.md` and is
referenced in the Close, not shown as footage.

---

## Before you hit record

- The video is already recorded and retimed (`demo-narratable.mp4`). You are
  recording a voiceover track over it, not a live screen capture.
- Play the mp4 and read the sections below in sync with the on-screen banners.
- Real proofs are slow. The retimed video collapsed the dead air, but the
  sections are sized to the time you have. If you finish a section early, pause;
  if you run long, trim the optional lines (marked **[optional]**).

---

## Open — 0:04, the title banner is on screen

> Hey. This is what I built for LP-0008, a Logos-native autonomous agent.
> Right at the top here, `RISC0_DEV_MODE = 0`. Everything you're about to see
> is backed by real Groth16 proofs, not the dev-mode shortcut. When it feels
> slow, that's the point. That's real cryptography.
>
> The whole idea is an agent that's actually sovereign. It owns its own keys,
> holds its own money, stores its own files, talks over its own encrypted
> channels, and none of it runs through me or through some server. Let me run
> it and explain as it goes.

---

## Scenario 1 — Sovereign wallet — 0:04 to 2:59

**[SAY as the title banner holds — short]**

> First scenario is the wallet. It spins up a real local Logos execution zone
> in Docker, sequencer and all, then the agent gives itself an identity and
> starts moving money.

**[WHILE THE STACK BOOTS AND THE FIRST PROOFS GENERATE — long cluster, keep talking]**

> So while that's working, let me explain identity, because it's the part I'm
> proudest of. The agent has its own shielded LEZ account, its own keypair.
> On-chain it's indistinguishable from any other account holder. Nobody can
> look at the chain and say "that's a bot." It receives funds like anyone, it
> spends like anyone.
>
> The second piece is the spending threshold, the heart of the safety model.
> I, as the owner, set a per-transaction limit. Below that line the agent just
> acts, signs and submits on its own, no human in the loop. That's what's
> proving out right now, a transfer under the limit going through
> autonomously.
>
> **[optional]** Above that line it does not spend. It stops, sends me a
> request over our private channel, and waits. So the agent is genuinely
> autonomous for the small stuff and genuinely on a leash for anything big.
> Useful without being dangerous.
>
> The reason this works trustlessly is the Logos stack. On a normal cloud
> setup, an "agent wallet" means a custodian holds the keys. Here there's no
> custodian. The agent holds its own keys, the proof system enforces the rules,
> the chain settles it.

**[IF INDEXER ERRORS SCROLL PAST — the red `ERROR indexer_core` and `WARN` lines around 1:50 — point at them, don't skip them]**

> You'll notice these red lines. The indexer, a separate indexing service in
> this local stack, chokes on one of the early privacy-preserving proofs and
> stops indexing. I'll be straight: it doesn't affect anything you're
> watching. The sequencer is the chain here, it's what accepts and includes
> transactions, and it's what the agent reads its confirmed state from. Every
> transfer still lands, every balance still checks out. The indexer just goes
> stale for the rest of the run. Why it rejects a proof the sequencer accepted,
> I haven't diagnosed, and I'd rather say so than guess.

**[SAY when Scenario 1 finishes — `test result: ok` at 2:55 — short]**

> And there it is, passed. It spent what was under the limit and held the one
> that was over. Exactly the behaviour I want. On to control.

---

## Scenario 2 — Owner control — 2:59 to 6:05

**[SAY as the Scenario 2 banner appears — short]**

> This one's the conversation between me and my agent. It proposes a spend
> over the limit, I approve it; it proposes another, I deny it; then I raise
> the limit and watch it act on its own again.

**[WHILE IT PROVES — long cluster, keep talking]**

> The channel these messages go over is Logos Messaging, end-to-end encrypted,
> no central server. The agent and I each derive the same two topics from our
> identities, and that's our private line. I can reach it from any Logos app
> instance that holds my keys. There's no dashboard, no API endpoint exposed
> on the internet. Just an encrypted conversation.
>
> Two things in here I sweated. First, ordering. When the agent wants to spend
> over the limit, it notifies me first, and only holds the spend as pending if
> that message actually got delivered. So if the network's down and it can't
> reach me, it doesn't quietly park a spend that might later go through without
> me seeing it. It retries, and if it still can't reach me, it refuses. Fails
> safe, every time.
>
> Second, persistence. Those pending approvals get written to disk. So if the
> node restarts, crash or deploy or network blip, the agent comes back up and
> the spend that was waiting for my yes is still waiting. It doesn't lose
> state and it doesn't double-act. That's wired into the deployed binary, not
> just the tests.
>
> **[optional]** And notice what's happening as I reconfigure. I raise the
> limit over the same encrypted channel, and now a spend that would've been
> held a minute ago just goes through. The policy is live. I can retune how
> much rope the agent has without redeploying anything.

**[SAY when Scenario 2 finishes — `test result: ok` at 6:01 — short]**

> Approved one, denied one, changed the limit, and it adapted. That's the
> owner relationship in a nutshell.

---

## Scenario 3 — Agent marketplace (A2A) — 6:05 to 12:20

**[SAY as the Scenario 3 banner appears — short]**

> Last one, and this is where it gets fun. Two separate agents. One offers a
> service for a price, the other discovers it, pays for it, gets the result,
> and I'm not involved at all. No human anywhere in this loop.

**[WHILE IT PROVES — this is the longest window, plenty of time, keep talking]**

> This part follows the A2A protocol, Agent2Agent, the open standard the Linux
> Foundation stewards. Agents publish an Agent Card that says who they are and
> what they charge, they discover each other, and they run a task through a
> defined lifecycle: submitted, working, completed. That's all standard A2A.
>
> But A2A deliberately leaves two holes: it doesn't say how agents pay each
> other, and it doesn't say how they talk securely. Those are exactly the two
> things Logos fills for free. The transport is Logos Messaging instead of
> HTTP, so it's encrypted with no server in the middle. And the payment is a
> real LEZ token transfer, which is what's proving right now. The client agent
> is literally paying the provider agent, on-chain, autonomously, as part of
> accepting the task.
>
> You'll see the first test here is the refund path. A task gets canceled, and
> the agent issues a real on-chain refund. The money side is honest in both
> directions, not just when things go well.
>
> **[optional]** So think about what that unlocks. Any agent built on any
> A2A-compatible framework can discover and call a Logos agent, but the Logos
> agent brings payment and privacy that vanilla A2A just can't offer.
>
> Underneath all three of these scenarios is one small idea: skills. Every
> capability, storage, messaging, a wallet send, a program call, an A2A task,
> is just a skill, one trait with one method. Anyone can add a new skill by
> implementing that trait and registering it. They never touch the core.
> That's how you keep an agent like this open instead of a walled garden.

**[SAY when Scenario 3 finishes — `2 passed` at 12:16 — short]**

> Done. Two agents, a discovered task, a real payment, settled between them
> with no me in the middle. And the final banner: all scenarios passed,
> dev-mode zero the whole way.

---

## Close — 12:20, the "Demo complete" banner

> So that's it. A sovereign agent on Logos, end to end: its own shielded
> identity and funds, a spending leash I control over an encrypted channel,
> agent-to-agent coordination that pays its own way, and every bit of it
> backed by real proofs. No custodian, no server, no host. That's the whole
> reason to build this on Logos instead of anywhere else.
>
> What you just watched runs against a local sequencer. The same agent also
> runs on the official public LEZ testnet, where each of the three category
> agents lands its own included, verified on-chain transaction through a
> program they deployed. The hashes, blocks, and explorer links for that are
> in the repo, in `docs/THREE_TESTNET_SETTLEMENTS.md`. Thanks for watching.
