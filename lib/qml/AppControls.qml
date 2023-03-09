import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

Item {
    id: root

    signal saveSession(string filename)
    signal loadSession(string filename)
    signal stopAllExceptMaster()

    property bool loading_session : false
    property bool saving_session : false

    Row {
        spacing: 0
        anchors.fill: parent

        Button {
            anchors {
                top: parent.top
            }
            height: 35
            onClicked: mainmenu.popup()

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'dots-vertical'
                color: Material.foreground
            }

            Menu {
                id: mainmenu

                MenuItem {
                    text: "Save copy of session"
                    onClicked: { savesessiondialog.open() }
                }
                MenuItem {
                    text: "Load session"
                    onClicked: loadsessiondialog.open()
                }
            }

            FileDialog {
                id: savesessiondialog
                fileMode: FileDialog.SaveFile
                acceptLabel: 'Save'
                nameFilters: ["ShoopDaLoop session files (*.shl)(*.shl)"]
                defaultSuffix: 'shl'
                onAccepted: {
                    var filename = selectedFile.toString().replace('file://', '');
                    root.saveSession(filename)
                }
            }

            FileDialog {
                id: loadsessiondialog
                fileMode: FileDialog.OpenFile
                acceptLabel: 'Load'
                nameFilters: ["ShoopDaLoop session files (*.shl)(*.shl)", "All files (*)"]
                onAccepted: {
                    var filename = selectedFile.toString().replace('file://', '');
                    root.loadSession(filename)
                }

            }
        }

        // Button {
        //     anchors {
        //         left: parent.left
        //         right: parent.right
        //     }
        //     height: 35
        //     onClicked: settings.open()

        //     MaterialDesignIcon {
        //         size: parent.width - 10
        //         anchors.centerIn: parent
        //         name: 'settings'
        //         color: Material.foreground
        //     }

        //     SettingsDialog {
        //         id: settings
        //         parent: Overlay.overlay
        //         x: (parent.width - width) / 2
        //         y: (parent.height - height) / 2
        //     }
        // }

        Button {
            anchors {
                top: parent.top
            }
            height: 35
            onClicked: root.stopAllExceptMaster()

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'stop'
                color: Material.foreground
            }
        }
    }

    Popup {
        visible: root.saving_session
        modal: true

        anchors.centerIn: parent

        Text {
            color: Material.foreground
            text: "Saving session..."
        }
    }

    Popup {
        visible: root.loading_session
        modal: true

        anchors.centerIn: parent

        Text {
            color: Material.foreground
            text: "Loading session..."
        }
    }
}