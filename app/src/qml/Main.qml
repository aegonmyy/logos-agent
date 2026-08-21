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
    readonly property bool   channelOpen:  bk ? bk.channelOpen  : false
    readonly property string lastErr:      bk ? bk.lastErr      : ""

    // Pending approval requests, parsed from the JSON the plugin pushes.
    readonly property var requests: {
        try { return JSON.parse(root.requestsJson) } catch (e) { return [] }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Label {
            text: "Autonomous Agent"
            font.pixelSize: 22
            font.bold: true
        }

        RowLayout {
            spacing: 8
            Label { text: "Core:" }
            Label { text: root.agentVersion.length ? root.agentVersion : "—" }
            Item { Layout.fillWidth: true }
            Button {
                text: "Refresh"
                onClicked: if (root.bk) root.bk.refresh()
            }
        }

        Label {
            text: "Skills"
            font.bold: true
        }

        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 120
            TextArea {
                readOnly: true
                wrapMode: TextArea.Wrap
                text: root.skillsJson
            }
        }

        Label {
            text: "Pending spend approvals"
            font.bold: true
        }

        Label {
            visible: !root.channelOpen
            color: "#c0392b"
            wrapMode: Text.Wrap
            text: "Owner channel not configured. Set LOGOS_AGENT_ACCOUNT_ID, " +
                  "LOGOS_AGENT_OWNER_ID, and AGENT_MESSAGING_URL to approve " +
                  "spends over Logos Messaging."
        }

        // Each pending request: who/what/amount, with Approve and Deny.
        Repeater {
            model: root.requests
            delegate: Rectangle {
                id: card
                Layout.fillWidth: true
                Layout.preferredHeight: 64
                color: "#f5f5f5"
                border.color: "#d0d0d0"
                radius: 6

                readonly property var req: modelData
                readonly property string reqId: req.id || ""
                readonly property string reqAmount: req.amount || "?"
                readonly property string reqTo: req.to || ""

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 10
                    spacing: 8

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label { text: "Spend " + card.reqAmount + " tokens" }
                        Label {
                            font.pixelSize: 11
                            color: "#666"
                            text: "to " + card.reqTo + "  ·  id " + card.reqId
                            elide: Text.ElideRight
                        }
                    }

                    Button {
                        text: "Approve"
                        enabled: root.channelOpen
                        onClicked: {
                            root.bk.decide(card.reqId, true)
                            root.bk.pollRequests()
                        }
                    }
                    Button {
                        text: "Deny"
                        enabled: root.channelOpen
                        onClicked: {
                            root.bk.decide(card.reqId, false)
                            root.bk.pollRequests()
                        }
                    }
                }
            }
        }

        RowLayout {
            spacing: 8
            Button {
                text: "Poll requests"
                enabled: root.channelOpen
                onClicked: if (root.bk) root.bk.pollRequests()
            }
            Item { Layout.fillWidth: true }
        }

        Label {
            text: "Spending policy"
            font.bold: true
        }

        RowLayout {
            spacing: 8
            Label { text: "Per-tx limit:" }
            TextField {
                id: limitField
                Layout.preferredWidth: 120
                placeholderText: "e.g. 50"
                inputMethodHints: Qt.ImhDigitsOnly
            }
            Button {
                text: "Set"
                enabled: root.channelOpen && limitField.text.length > 0
                onClicked: root.bk.configureLimit(limitField.text)
            }
        }

        RowLayout {
            spacing: 8
            Label { text: "Per-period limit:" }
            TextField {
                id: periodLimitField
                Layout.preferredWidth: 120
                placeholderText: "e.g. 500"
                inputMethodHints: Qt.ImhDigitsOnly
            }
            Label { text: "seconds:" }
            TextField {
                id: periodSecondsField
                Layout.preferredWidth: 100
                placeholderText: "86400"
                inputMethodHints: Qt.ImhDigitsOnly
            }
            Button {
                text: "Set"
                enabled: root.channelOpen && periodLimitField.text.length > 0
                onClicked: root.bk.configurePeriod(periodLimitField.text,
                                                   parseInt(periodSecondsField.text) || 0)
            }
        }

        Label {
            visible: root.lastErr.length > 0
            color: "#c0392b"
            wrapMode: Text.Wrap
            text: root.lastErr
        }
    }
}
