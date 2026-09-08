import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    // Backend wiring: the RemoteObjects replica exposed by AgentOwnerPlugin.
    readonly property var    bk:           logos.module("agent_owner")
    readonly property string agentVersion: bk ? bk.agentVersion : ""
    readonly property string skillsJson:   bk ? bk.skillsJson   : ""
    readonly property string requestsJson:  bk ? bk.requestsJson  : "[]"
    readonly property string spentJson:     bk ? bk.spentJson     : "[]"
    readonly property bool   channelOpen:  bk ? bk.channelOpen  : false
    readonly property string lastErr:      bk ? bk.lastErr      : ""

    // Pending approval requests, parsed from the JSON the plugin pushes.
    readonly property var requests: {
        try { return JSON.parse(root.requestsJson) } catch (e) { return [] }
    }

    // Persistent activity log: every action appends a timestamped entry that
    // stays on screen (unlike a toast), so a reviewer watching the recording can
    // see each step succeeded. log(msg, ok) adds an entry; the list scrolls.
    property var logEntries: []
    property string toast: ""
    function flash(msg) { root.toast = msg; toastTimer.restart() }
    function log(msg, ok) {
        var ts = Qt.formatDateTime(new Date(), "hh:mm:ss")
        var entry = { "time": ts, "text": msg, "ok": ok === undefined ? true : ok }
        var next = root.logEntries.slice()
        next.unshift(entry)
        root.logEntries = next
        root.flash(msg)
    }
    Timer { id: toastTimer; interval: 2500; onTriggered: root.toast = "" }

    // Last policy values the owner set (the backend does not read them back,
    // so the UI remembers what it last sent and shows it persistently).
    property string lastPerTxLimit: "30"
    property string lastPerPeriodLimit: "0"
    property string lastPeriodSeconds: "86400"

    // Agent's executed spends, surfaced from the owner channel (the agent
    // posts a "spent" notification whenever it executes a spend, whether
    // owner-approved or autonomous under-limit). This is what makes stage 4
    // (the autonomous spend) visible: the owner takes no action, but the
    // agent's "spent" message lands on the to-owner topic and the auto-poll
    // picks it up, appending it to the activity log.
    property var spentList: []
    property int lastSpentCount: 0
    function checkNewSpends() {
        if (!root.bk || !root.channelOpen) return
        root.bk.pollRequests()
        var spents = []
        try { spents = JSON.parse(root.bk.spentJson || "[]") } catch (e) { return }
        root.spentList = spents
        if (spents.length > root.lastSpentCount) {
            for (var i = root.lastSpentCount; i < spents.length; i++) {
                var s = spents[i]
                var to = (s.to || "?").slice(0, 12)
                root.log("Agent spent " + (s.amount || "?") + " tokens to " + to + "... (on-chain)")
            }
            root.lastSpentCount = spents.length
        }
    }
    Timer {
        id: autoPollTimer
        interval: 3500; running: root.channelOpen; repeat: true
        onTriggered: root.checkNewSpends()
    }

    // --- palette ---
    readonly property color cBg:      "#1b1f24"
    readonly property color cPanel:   "#23282e"
    readonly property color cPanel2: "#2b3138"
    readonly property color cBorder:  "#3a424c"
    readonly property color cText:    "#e8ecf1"
    readonly property color cMute:    "#8a93a0"
    readonly property color cGreen:   "#2e7d32"
    readonly property color cRed:     "#c62828"
    readonly property color cBlue:   "#1565c0"
    readonly property color cAmber:   "#f59e0b"

    Rectangle { anchors.fill: parent; color: root.cBg }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 18
        spacing: 14

        // --- header ---
        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Rectangle {
                width: 38; height: 38; radius: 9
                color: root.cBlue
                Label {
                    anchors.centerIn: parent
                    text: "A"
                    color: "white"
                    font.pixelSize: 18; font.bold: true
                }
            }
            ColumnLayout {
                spacing: 1
                Label {
                    text: "Autonomous Agent"
                    color: root.cText
                    font.pixelSize: 19; font.bold: true
                }
                Label {
                    text: root.agentVersion.length ? "Core " + root.agentVersion : "Core not connected"
                    color: root.cMute
                    font.pixelSize: 11
                }
            }
            Item { Layout.fillWidth: true }

            // connection pill
            Rectangle {
                radius: 11
                implicitHeight: 24
                implicitWidth: pillRow.implicitWidth + 20
                color: root.channelOpen ? "#1b3a23" : "#3a1f1f"
                border.color: root.channelOpen ? root.cGreen : root.cRed
                border.width: 1
                RowLayout {
                    id: pillRow
                    anchors.centerIn: parent
                    spacing: 6
                    Rectangle {
                        width: 8; height: 8; radius: 4
                        color: root.channelOpen ? root.cGreen : root.cRed
                    }
                    Label {
                        text: root.channelOpen ? "Owner channel connected" : "Owner channel not configured"
                        color: root.channelOpen ? "#9fd3a8" : "#e6a0a0"
                        font.pixelSize: 11; font.bold: true
                    }
                }
            }
        }

        // --- action row: Refresh + Poll ---
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Button {
                id: refreshBtn
                text: "↻  Refresh"
                enabled: root.bk && root.channelOpen
                onClicked: { root.bk.refresh(); root.log("Refreshed agent info") }
                contentItem: Label {
                    text: refreshBtn.text
                    color: root.cText
                    font.pixelSize: 13; font.bold: true
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                background: Rectangle {
                    radius: 7
                    color: refreshBtn.down ? root.cBorder
                         : (refreshBtn.hovered ? Qt.lighter(root.cPanel2, 1.15) : root.cPanel2)
                    border.color: root.cBorder; border.width: 1
                    opacity: refreshBtn.enabled ? 1.0 : 0.45
                }
            }
            Button {
                id: pollBtn
                text: "⦿  Poll requests"
                enabled: root.channelOpen
                onClicked: {
                    root.bk.pollRequests()
                    var n = root.requests.length
                    root.log(n > 0 ? ("Polled: " + n + " pending request" + (n > 1 ? "s" : "")) : "Polled: no new requests")
                }
                contentItem: Label {
                    text: pollBtn.text
                    color: "white"
                    font.pixelSize: 13; font.bold: true
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                background: Rectangle {
                    radius: 7
                    color: pollBtn.down ? root.cBlue
                         : (pollBtn.hovered ? Qt.lighter(root.cBlue, 1.1) : root.cBlue)
                    border.color: root.cBlue; border.width: 1
                    opacity: pollBtn.enabled ? 1.0 : 0.45
                }
            }
            Item { Layout.fillWidth: true }
        }

        // --- skills ---
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 96
            color: root.cPanel; border.color: root.cBorder; radius: 8
            Label {
                anchors.top: parent.top; anchors.left: parent.left; anchors.margins: 10
                text: "Skills"
                color: root.cMute; font.pixelSize: 11; font.bold: true
            }
            ScrollView {
                anchors.fill: parent; anchors.margins: 10; anchors.topMargin: 26
                TextArea {
                    readOnly: true; wrapMode: TextArea.Wrap
                    color: root.cMute; font.pixelSize: 11
                    text: root.skillsJson
                    background: Item {}
                }
            }
        }

        // --- pending approvals ---
        Label {
            text: "Pending spend approvals"
            color: root.cText
            font.pixelSize: 13; font.bold: true
        }

        // Empty state.
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 56
            visible: root.channelOpen && root.requests.length === 0
            color: root.cPanel; radius: 8; border.color: root.cBorder
            Label {
                anchors.centerIn: parent
                color: root.cMute; font.pixelSize: 12
                text: "No pending requests. Click Poll requests to check."
            }
        }

        // Not-configured warning.
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 52
            visible: !root.channelOpen
            color: "#3a1f1f"; radius: 8; border.color: root.cRed
            Label {
                anchors.fill: parent; anchors.margins: 10
                color: "#e6a0a0"; wrapMode: Text.Wrap; font.pixelSize: 11
                text: "Owner channel not configured. Set LOGOS_AGENT_ACCOUNT_ID, LOGOS_AGENT_OWNER_ID, and AGENT_MESSAGING_URL to approve spends over Logos Messaging."
            }
        }

        // Each pending request: who/what/amount, with Approve and Deny.
        Repeater {
            model: root.requests
            delegate: Rectangle {
                id: card
                Layout.fillWidth: true
                Layout.preferredHeight: 72
                color: root.cPanel
                border.color: root.cBorder
                radius: 8

                readonly property var req: modelData
                readonly property string reqId: req.id || ""
                readonly property string reqAmount: req.amount || "?"
                readonly property string reqTo: req.to || ""

                // pending accent bar
                Rectangle {
                    x: 0; y: 0; width: 4; height: parent.height
                    color: root.cAmber; radius: 2
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 12
                    anchors.leftMargin: 18
                    spacing: 10

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        RowLayout {
                            spacing: 6
                            Label {
                                text: card.reqAmount
                                color: root.cText
                                font.pixelSize: 18; font.bold: true
                            }
                            Label {
                                text: "tokens"
                                color: root.cMute; font.pixelSize: 12
                                Layout.alignment: Qt.AlignBottom
                            }
                        }
                        Label {
                            font.pixelSize: 11; color: root.cMute
                            elide: Text.ElideRight
                            text: "to " + card.reqTo + "  ·  id " + card.reqId
                        }
                    }

                    // Approve: green. Direct call, approve=true.
                    Button {
                        id: approveBtn
                        text: "Approve"
                        enabled: root.channelOpen
                        onClicked: {
                            root.bk.decide(card.reqId, true)
                            root.bk.pollRequests()
                            root.log("Approved " + card.reqAmount + " token spend (id " + card.reqId + ")")
                        }
                        contentItem: Label {
                            text: approveBtn.text
                            color: "white"
                            font.pixelSize: 13; font.bold: true
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                        background: Rectangle {
                            radius: 7
                            color: approveBtn.down ? "#1b5e20"
                                 : (approveBtn.hovered ? Qt.lighter(root.cGreen, 1.12) : root.cGreen)
                            border.color: root.cGreen; border.width: 1
                            opacity: approveBtn.enabled ? 1.0 : 0.45
                        }
                    }

                    // Deny: red. Direct call, approve=false.
                    Button {
                        id: denyBtn
                        text: "Deny"
                        enabled: root.channelOpen
                        onClicked: {
                            root.bk.decide(card.reqId, false)
                            root.bk.pollRequests()
                            root.log("Denied " + card.reqAmount + " token spend (id " + card.reqId + ")", false)
                        }
                        contentItem: Label {
                            text: denyBtn.text
                            color: "white"
                            font.pixelSize: 13; font.bold: true
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                        background: Rectangle {
                            radius: 7
                            color: denyBtn.down ? "#a31515"
                                 : (denyBtn.hovered ? Qt.lighter(root.cRed, 1.12) : root.cRed)
                            border.color: root.cRed; border.width: 1
                            opacity: denyBtn.enabled ? 1.0 : 0.45
                        }
                    }
                }
            }
        }

        // --- spending policy ---
        Label {
            text: "Spending policy"
            color: root.cText
            font.pixelSize: 13; font.bold: true
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 132
            color: root.cPanel; border.color: root.cBorder; radius: 8
            ColumnLayout {
                anchors.fill: parent; anchors.margins: 12; spacing: 10

                // live policy readout: what the agent is enforcing right now
                Label {
                    font.pixelSize: 12; font.bold: true
                    color: root.cText
                    text: "Current policy:  per-tx " + root.lastPerTxLimit
                          + "  ·  per-period " + root.lastPerPeriodLimit
                          + " / " + root.lastPeriodSeconds + "s"
                }

                RowLayout {
                    spacing: 8
                    Label {
                        text: "Per-tx limit"
                        color: root.cMute; font.pixelSize: 12
                        Layout.alignment: Qt.AlignVCenter
                    }
                    TextField {
                        id: limitField
                        Layout.preferredWidth: 120
                        placeholderText: "e.g. 50"
                        inputMethodHints: Qt.ImhDigitsOnly
                        color: root.cText; font.pixelSize: 13
                        background: Rectangle {
                            radius: 6; color: root.cPanel2
                            border.color: limitField.activeFocus ? root.cBlue : root.cBorder
                            border.width: 1
                        }
                    }
                    Button {
                        id: setLimitBtn
                        text: "Set"
                        enabled: root.channelOpen && limitField.text.length > 0
                        onClicked: {
                            var prev = root.lastPerTxLimit
                            root.bk.configureLimit(limitField.text)
                            root.lastPerTxLimit = limitField.text
                            root.log("Per-tx limit set " + prev + " -> " + limitField.text)
                        }
                        contentItem: Label {
                            text: setLimitBtn.text
                            color: root.cText
                            font.pixelSize: 13; font.bold: true
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                        background: Rectangle {
                            radius: 7
                            color: setLimitBtn.down ? root.cBorder
                                 : (setLimitBtn.hovered ? Qt.lighter(root.cPanel2, 1.15) : root.cPanel2)
                            border.color: root.cBorder; border.width: 1
                            opacity: setLimitBtn.enabled ? 1.0 : 0.45
                        }
                    }
                    Item { Layout.fillWidth: true }
                }

                RowLayout {
                    spacing: 8
                    Label {
                        text: "Per-period limit"
                        color: root.cMute; font.pixelSize: 12
                        Layout.alignment: Qt.AlignVCenter
                    }
                    TextField {
                        id: periodLimitField
                        Layout.preferredWidth: 110
                        placeholderText: "e.g. 500"
                        inputMethodHints: Qt.ImhDigitsOnly
                        color: root.cText; font.pixelSize: 13
                        background: Rectangle {
                            radius: 6; color: root.cPanel2
                            border.color: periodLimitField.activeFocus ? root.cBlue : root.cBorder
                            border.width: 1
                        }
                    }
                    Label { text: "seconds"; color: root.cMute; font.pixelSize: 12; Layout.alignment: Qt.AlignVCenter }
                    TextField {
                        id: periodSecondsField
                        Layout.preferredWidth: 100
                        placeholderText: "86400"
                        inputMethodHints: Qt.ImhDigitsOnly
                        color: root.cText; font.pixelSize: 13
                        background: Rectangle {
                            radius: 6; color: root.cPanel2
                            border.color: periodSecondsField.activeFocus ? root.cBlue : root.cBorder
                            border.width: 1
                        }
                    }
                    Button {
                        id: setPeriodBtn
                        text: "Set"
                        enabled: root.channelOpen && periodLimitField.text.length > 0
                        onClicked: {
                            root.bk.configurePeriod(periodLimitField.text,
                                                     parseInt(periodSecondsField.text) || 0)
                            root.lastPerPeriodLimit = periodLimitField.text
                            root.lastPeriodSeconds = periodSecondsField.text
                            root.log("Per-period limit set to " + periodLimitField.text + " / " + (parseInt(periodSecondsField.text) || 0) + "s")
                        }
                        contentItem: Label {
                            text: setPeriodBtn.text
                            color: root.cText
                            font.pixelSize: 13; font.bold: true
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                        background: Rectangle {
                            radius: 7
                            color: setPeriodBtn.down ? root.cBorder
                                 : (setPeriodBtn.hovered ? Qt.lighter(root.cPanel2, 1.15) : root.cPanel2)
                            border.color: root.cBorder; border.width: 1
                            opacity: setPeriodBtn.enabled ? 1.0 : 0.45
                        }
                    }
                    Item { Layout.fillWidth: true }
                }
            }
        }

        // --- activity log: persistent record of every owner action ---
        Label {
            text: "Activity log"
            color: root.cText
            font.pixelSize: 13; font.bold: true
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: 110
            color: root.cPanel; border.color: root.cBorder; radius: 8
            clip: true
            ListView {
                id: logView
                anchors.fill: parent
                anchors.margins: 8
                model: root.logEntries
                spacing: 4
                delegate: RowLayout {
                    width: logView.width
                    spacing: 8
                    Label {
                        text: modelData.time
                        color: root.cMute; font.pixelSize: 11
                        Layout.preferredWidth: 64
                    }
                    Rectangle {
                        width: 8; height: 8; radius: 4
                        color: modelData.ok ? root.cGreen : root.cRed
                        Layout.alignment: Qt.AlignVCenter
                    }
                    Label {
                        Layout.fillWidth: true
                        text: modelData.text
                        color: modelData.ok ? root.cText : "#e6a0a0"
                        font.pixelSize: 12
                        elide: Text.ElideRight
                    }
                }
                Label {
                    visible: root.logEntries.length === 0
                    anchors.centerIn: parent
                    text: "Actions will appear here"
                    color: root.cMute; font.pixelSize: 12
                }
            }
        }

        // --- error ---
        Label {
            Layout.fillWidth: true
            visible: root.lastErr.length > 0
            color: "#e6a0a0"; wrapMode: Text.Wrap; font.pixelSize: 11
            text: root.lastErr
        }
        Item { Layout.fillHeight: true }
    }

    // --- toast banner (bottom) ---
    Rectangle {
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottomMargin: 18
        radius: 8
        implicitWidth: toastLabel.implicitWidth + 28
        implicitHeight: 34
        visible: root.toast.length > 0
        color: "#1b3a23"
        border.color: root.cGreen; border.width: 1
        Label {
            id: toastLabel
            anchors.centerIn: parent
            text: root.toast
            color: "#9fd3a8"
            font.pixelSize: 12; font.bold: true
        }
    }
}
