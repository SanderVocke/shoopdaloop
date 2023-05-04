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
    property Registry state_registry
    property Registry objects_registry
    property alias sync_active : sync_active_button.sync_active

    function update() {
        state_registry.replace('sync_active', sync_active)
    }

    onSync_activeChanged: update()
    Component.onCompleted: update()

    Row {
        spacing: 6
        anchors.fill: parent

        Button {
            height: 35
            width: 30
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
                    close()
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
                    close()
                    var filename = selectedFile.toString().replace('file://', '');
                    root.loadSession(filename)
                }

            }
        }

        Button {
            height: 35
            width: 30
            onClicked: root.stopAllExceptMaster()

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'stop'
                color: Material.foreground
            }
        }

        Button {
            id: sync_active_button
            height: 35
            width: 30
            property bool sync_active_base: true
            onClicked: sync_active_base = !sync_active_base
            property bool sync_active : sync_active_base && !key_modifiers.control_pressed

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: sync_active_button.sync_active ? 'timer-sand' : 'exclamation';
                color: Material.foreground
            }
        }

        Button {
            id: deselect_button
            height: 35
            width: 30
            onClicked: root.state_registry.clear_set('selected_loop_ids')

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'border-none-variant'
                color: Material.foreground
            }
        }
    }
}