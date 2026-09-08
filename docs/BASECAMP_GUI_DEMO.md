# Basecamp GUI Demo

A human-driven recording of the Basecamp owner app driving a live autonomous
agent through the full owner-approval lifecycle over real Waku messaging, with
every decision settled on a real local LEZ sequencer. A real human drives the
GUI over VNC with a real cursor; the agent driver waits for each GUI decision
and verifies the on-chain balance after each stage.

- **Video:** `recordings/basecamp-gui-demo.mp4` (4 min 32 s, 1920x1080)
- **Reproduce (interactive, human-driven):** `scripts/record-gui-demo-interactive.sh`
  (brings up Xvfb + x11vnc + noVNC + nwaku + the `logos-standalone-app` with the
  `agent_owner` plugin + the agent driver test; you drive the GUI in a browser
  over VNC while ffmpeg captures the framebuffer; the script overlays timed
  stage captions and a final settlement summary)
- **Reproduce (headless, scripted):** `scripts/record-gui-demo.sh` drives the
  same QML UI through the QML Inspector TCP API with no human in the loop

## What the demo shows

The `agent_owner` Basecamp app (a `ui_qml` plugin loaded by
`logos-standalone-app`) talks to the agent core over Qt RemoteObjects and to the
agent's owner channel over real Logos Messaging (nwaku, Logos Dev Network
cluster 2). The agent holds its own shielded LEZ account with a per-transaction
limit of 30. The owner uses the GUI to approve, deny, and reconfigure the agent,
and each decision settles on-chain against a standalone LEZ sequencer.
Interaction is real time: while the channel is open the app auto-polls the
owner channel every 3.5 s (`autoPollTimer` in `app/src/qml/Main.qml`), and the
manual Poll button forces an immediate poll; the agent applies each decision on
its next poll cycle.

| Stage | Owner action (GUI) | Agent result | Balance |
|---|---|---|---|
| 1. Approve | Poll requests, then Approve a 50-token spend (over the 30 limit) | executes the spend on-chain | 100 -> 50 |
| 2. Deny | Poll requests, then Deny a 50-token spend | drops the spend, no movement | 50 |
| 3. Reconfigure | type 45 in the per-tx limit field, then Set | per-tx limit raised to 45 | 50 |
| 4. Autonomous | (none) the owner watches the Activity log report the spend | executes autonomously, no approval round-trip; the agent posts a "spent" notice the owner app surfaces | 50 -> 10 |

The driver log lines that anchor each stage:

```
APPROVED: [Executed { id: "req-0", amount: 50 }]
balance after approve: 50
DENIED: [Denied { id: "req-1" }]
balance after deny: 50
RECONFIGURED: per-tx limit is now 45
AUTONOMOUS: 40 tokens spent under the raised limit
balance after autonomous spend: 10
RECORDING_COMPLETE
```

## How it is driven

The shipped recording is the interactive variant: a human opens the Basecamp
app in a browser over noVNC and drives the controls with a real cursor while
ffmpeg captures the Xvfb framebuffer. The agent driver test
(`tests/record_gui_driver.rs`) creates the agent, funds it with 100 tokens,
opens the owner channel over real Waku, and proposes the over-limit spends.
It waits for the GUI's decisions to arrive over Waku and executes or cancels
the spends accordingly, verifying the on-chain balance after each stage.

The headless variant (`scripts/record-gui-demo.sh`) drives the same QML UI
through the QML Inspector (a TCP server on port 3768 with a newline-delimited
JSON protocol built into `logos-standalone-app`): `findAndClick` locates
buttons by their text label and sends real mouse events into the Qt scene
graph, and `evaluate` sets text-field properties directly. This is far more
reliable than xdotool coordinate guessing against a QQuickWidget.

## Narration

The recording has no narration; timed stage captions and a final settlement
summary are overlaid on the video so it is legible without sound. A
human-narrated cut covering the Basecamp app and CLI will accompany the
re-submission; the narration script is `submission/DEMO_SCRIPT.md`.
