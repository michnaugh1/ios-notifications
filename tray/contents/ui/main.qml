import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as P5Support

PlasmoidItem {
    id: root

    property string daemonState: "initializing"
    property string deviceAddress: ""
    property string lastError: ""
    property int notificationsToday: 0
    property int nextBackoffSecs: 0

    Plasmoid.title: i18n("iOS Notifications")
    Plasmoid.icon: iconForState(daemonState)

    toolTipMainText: i18n("iOS Notifications")
    toolTipSubText: tooltipForState(daemonState)

    function iconForState(s) {
        switch (s) {
            case "connected":    return "phone";
            case "connecting":   return "view-refresh";
            case "backoff":      return "task-recurring";
            case "paused":       return "media-playback-pause";
            case "error":        return "dialog-error";
            case "initializing": return "view-refresh";
            default:             return "phone";
        }
    }

    function tooltipForState(s) {
        switch (s) {
            case "connected":
                return i18n("Connected (%1 today)", notificationsToday);
            case "connecting":
                return i18n("Connecting…");
            case "backoff":
                return i18n("Retrying… (in %1s)", nextBackoffSecs);
            case "paused":
                return i18n("Paused");
            case "error":
                return i18n("Error: %1", lastError || i18n("unknown"));
            case "initializing":
                return i18n("Starting…");
            default:
                return s;
        }
    }

    // Use plasma5support DataSource to run shell commands.
    P5Support.DataSource {
        id: exec
        engine: "executable"
        connectedSources: []
        onNewData: function (sourceName, data) {
            const stdout = (data["stdout"] || "").trim();
            disconnectSource(sourceName);
            // Route by source name prefix
            if (sourceName.startsWith("get:State|")) {
                if (stdout) daemonState = stdout;
            } else if (sourceName.startsWith("get:DeviceAddress|")) {
                deviceAddress = stdout;
            } else if (sourceName.startsWith("get:LastError|")) {
                lastError = stdout;
            } else if (sourceName.startsWith("get:NotificationsToday|")) {
                notificationsToday = parseInt(stdout) || 0;
            } else if (sourceName.startsWith("get:NextBackoffSecs|")) {
                nextBackoffSecs = parseInt(stdout) || 0;
            }
        }

        function helperPath() {
            return Qt.resolvedUrl("../code/dbus-helper.sh").toString().replace("file://", "");
        }

        function get(prop) {
            const cmd = helperPath() + " get " + prop;
            const tag = "get:" + prop + "|" + Date.now();
            connectSource("sh -c '" + cmd + "; printf %s \\\"\\\"' # " + tag);
        }

        function call(method) {
            const cmd = helperPath() + " call " + method;
            connectSource("sh -c '" + cmd + " >/dev/null 2>&1' # call:" + method + "|" + Date.now());
        }
    }

    Timer {
        interval: 2000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            exec.get("State");
            exec.get("DeviceAddress");
            exec.get("LastError");
            exec.get("NotificationsToday");
            exec.get("NextBackoffSecs");
        }
    }

    compactRepresentation: Kirigami.Icon {
        source: iconForState(daemonState)
        active: daemonState === "connecting" || daemonState === "initializing"
        MouseArea {
            anchors.fill: parent
            onClicked: root.expanded = !root.expanded
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        Layout.preferredHeight: Kirigami.Units.gridUnit * 14
        spacing: Kirigami.Units.smallSpacing

        PlasmaCore.Heading {
            text: i18n("iOS Notifications")
            level: 2
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.largeSpacing
        }

        Kirigami.Icon {
            source: iconForState(daemonState)
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: Kirigami.Units.iconSizes.huge
            Layout.preferredHeight: Kirigami.Units.iconSizes.huge
        }

        Text {
            text: tooltipForState(daemonState)
            color: Kirigami.Theme.textColor
            Layout.alignment: Qt.AlignHCenter
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
        }

        Text {
            visible: deviceAddress.length > 0
            text: i18n("Device: %1", deviceAddress)
            font.pixelSize: Kirigami.Theme.smallFont.pixelSize
            color: Kirigami.Theme.disabledTextColor
            Layout.alignment: Qt.AlignHCenter
        }

        Item { Layout.fillHeight: true }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: Kirigami.Units.smallSpacing

            QQC2.Button {
                text: i18n("Reconnect")
                icon.name: "view-refresh"
                onClicked: exec.call("Reconnect")
            }

            QQC2.Button {
                text: daemonState === "paused" ? i18n("Resume") : i18n("Pause")
                icon.name: daemonState === "paused" ? "media-playback-start" : "media-playback-pause"
                onClicked: {
                    if (daemonState === "paused") exec.call("Resume");
                    else exec.call("Pause");
                }
            }

            QQC2.Button {
                text: i18n("Reload")
                icon.name: "document-revert"
                onClicked: exec.call("ReloadConfig")
            }
        }

        Item { Layout.preferredHeight: Kirigami.Units.smallSpacing }
    }
}
