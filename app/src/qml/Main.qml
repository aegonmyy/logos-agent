import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    // Backend wiring: the RemoteObjects replica exposed by AgentOwnerPlugin.
    readonly property var    bk:           logos.module("agent_owner")
    readonly property string agentVersion: bk ? bk.agentVersion : ""
    readonly property string skillsJson:   bk ? bk.skillsJson   : ""
    readonly property string lastErr:      bk ? bk.lastErr      : ""

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
            Layout.fillHeight: true
            TextArea {
                readOnly: true
                wrapMode: TextArea.Wrap
                text: root.skillsJson
            }
        }

        Label {
            visible: root.lastErr.length > 0
            color: "#c0392b"
            text: root.lastErr
        }
    }
}
