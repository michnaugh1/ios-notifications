import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root

    Plasmoid.title: i18n("iOS Notifications")
    Plasmoid.icon: "phone"

    toolTipMainText: i18n("iOS Notifications")
    toolTipSubText: i18n("Bridge status: not yet wired")

    compactRepresentation: Kirigami.Icon {
        source: "phone"
        active: true
        MouseArea {
            anchors.fill: parent
            onClicked: root.expanded = !root.expanded
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 16
        Layout.preferredHeight: Kirigami.Units.gridUnit * 8

        PlasmaCore.Heading {
            text: i18n("iOS Notifications")
            level: 2
            Layout.alignment: Qt.AlignHCenter
        }

        Text {
            text: i18n("D-Bus wiring lands in the next task.")
            color: Kirigami.Theme.textColor
            Layout.alignment: Qt.AlignHCenter
        }
    }
}
