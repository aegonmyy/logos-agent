# Basecamp GUI demo narration (rough read-along)

Read this over the Basecamp GUI demo video. The server-side cut is 4 min 32 s;
if you narrate your own OBS cut, the stage order is the same, just follow the
on-screen cues instead of the second marks. Speak naturally, first person.
You built this, you are narrating your own demo.

Timing is approximate. The on-screen stage captions (1/4, 2/4, 3/4, 4/4) tell
you where you are.

---

[0:00 to 0:30, app loading]

This is the Basecamp owner app. It is a Logos Core module loaded by
logos-standalone-app. The panel on the left is the agent I own. The status pill
at the top says the owner channel is open, which means the agent and I are
talking over real Waku messaging, the Logos Dev Network, cluster 2. No
intermediary server. The agent holds its own shielded LEZ wallet, funded with
100 tokens, and it has a per-transaction spending limit of 30.

[0:30 to 1:00, idle, explain the policy readout]

The readout near the top shows the current policy: per-tx 30, per-period 0,
period 86400 seconds. So any spend over 30 tokens in one transaction has to
come to me for approval. Anything under 30, the agent just spends on its own.
That threshold is the whole point of the owner control loop.

[1:00, stage 1 caption appears: Approve an over-limit 50-token spend]

Stage one. The agent wants to spend 50 tokens, which is over the 30 limit, so
it posts an approval request to me over Waku. I click Poll requests. The
request comes in. 50 tokens, over the limit. I click Approve.

[pause while the click lands and the log updates]

The Activity log shows the approval. The agent executes the spend on chain
against a real local LEZ sequencer. Balance goes from 100 to 50. That is a
real on-chain settlement, not a mock.

[3:23, stage 2 caption: Deny an over-limit spend]

Stage two. The agent posts another over-limit spend. I poll, I see the
request, and this time I click Deny.

[pause]

The log shows the denial. No tokens move. Balance stays at 50. The deny path
is real: the agent drops the pending spend, it does not execute.

[3:35, stage 3 caption: Raise the per-tx limit 30 to 45]

Stage three. I reconfigure the agent. I type 45 in the limit field and click
Set. The owner channel carries the new policy to the agent.

[pause]

The policy readout updates: per-tx is now 45. The agent will now autonomously
spend anything up to 45 tokens without asking me.

[4:28, stage 4 caption: Autonomous 40-token spend under the new limit]

Stage four. The agent spends 40 tokens. That is under the new 45 limit, so it
does not ask me. There is no approval request, no button to click. This is the
autonomous case. The agent posts a spent notice to the owner channel, and the
app auto-polls and surfaces it in the Activity log.

[pause, let the spent line land]

The log shows the agent spent 40 tokens on chain. Balance goes from 50 to 10.
I did not touch anything for this stage. That is the point: under the limit,
the agent acts on its own, and I see it after the fact.

[final settlement summary card]

To recap. Four stages, all on a real local LEZ sequencer, over real Waku.
Approve: 100 to 50. Deny: no movement, stayed at 50. Reconfigure: limit 30 to
45. Autonomous spend under the new limit: 50 to 10. Every decision the owner
makes is carried over the owner channel, and every spend, approved or
autonomous, settles on chain.

---

## Notes for the read

- If you flub a line, just re-read that line. The OBS cut can be trimmed.
- The on-screen captions already say the stage name, so you do not need to
  read the caption text verbatim. Narrate what is happening, do not read the
  screen.
- Total target length is about four and a half minutes. If you finish early,
  slow down at the stage boundaries; the pauses are where the viewer watches
  the balance change.
