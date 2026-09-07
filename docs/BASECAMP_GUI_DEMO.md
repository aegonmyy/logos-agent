# Basecamp GUI Demo

A headless recording of the Basecamp owner app driving a live autonomous agent
through the full owner-approval lifecycle over real Waku messaging, with every
decision settled on a real local LEZ sequencer.

- **Video:** `recordings/basecamp-gui-demo.mp4` (3 min 3 s, 1920x1080)
- **Reproduce:** `scripts/record-gui-demo.sh` (brings up Xvfb + nwaku + the
  `logos-standalone-app` with the `agent_owner` plugin + the agent driver test,
  drives the QML UI through the QML Inspector TCP API, captures with ffmpeg)

## What the demo shows

The `agent_owner` Basecamp app (a `ui_qml` plugin loaded by
`logos-standalone-app`) talks to the agent core over Qt RemoteObjects and to the
agent's owner channel over real Logos Messaging (nwaku, Logos Dev Network
cluster 2). The agent holds its own shielded LEZ account with a per-transaction
limit of 30. The owner uses the GUI to approve, deny, and reconfigure the agent,
and each decision settles on-chain against a standalone LEZ sequencer.

| Stage | Owner action (GUI) | Agent result | Balance |
|---|---|---|---|
| 1. Approve | Poll requests, then Approve a 50-token spend (over the 30 limit) | executes the spend on-chain | 100 -> 50 |
| 2. Deny | Poll requests, then Deny a 50-token spend | drops the spend, no movement | 50 |
| 3. Reconfigure | type 45 in the per-tx limit field, then Set | per-tx limit raised to 45 | 50 |
| 4. Autonomous | (none) a 40-token spend is now under the 45 limit | executes autonomously, no approval round-trip | 50 -> 10 |

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

The QML UI is driven through the QML Inspector (a TCP server on port 3768 with a
newline-delimited JSON protocol built into `logos-standalone-app`):
`findAndClick` locates buttons by their text label and sends real mouse events
into the Qt scene graph, and `evaluate` sets text-field properties directly.
This is far more reliable than xdotool coordinate guessing against a
QQuickWidget.

The agent side is a Rust integration test (`tests/record_gui_driver.rs`) that
creates the agent, funds it with 100 tokens, opens the owner channel over real
Waku, and proposes the over-limit spends. It waits for the GUI's decisions to
arrive over Waku and executes or cancels the spends accordingly, verifying the
on-chain balance after each stage.

## Narration

The raw recording has no narration. A human-narrated cut covering the Basecamp
app and CLI will accompany the re-submission; the narration script is
`submission/DEMO_SCRIPT.md`.
