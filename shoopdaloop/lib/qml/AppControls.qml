import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import '../backend/frontend_interface/types.js' as Types

Item {
    id: root

    signal saveSession(string filename)
    signal loadSession(string filename)

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

        ExtendedButton {
            height: 35
            width: 30
            onClicked: mainmenu.popup()

            tooltip: "Main menu"

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

        ExtendedButton {
            tooltip: "Stop all loops (respects sync active)"

            height: 35
            width: 30
            onClicked: {
                var loops = root.objects_registry.select_values(o => o instanceof LoopWidget)
                var backend_loops = loops.map(o => o.maybe_loaded_loop).filter(o => o != undefined)
                if(backend_loops.length > 0) {
                    backend_loops[0].transition_multiple(backend_loops, Types.LoopMode.Stopped, 0, root.sync_active)
                }
            }

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'stop'
                color: Material.foreground
            }
        }

        ExtendedButton {
            tooltip: "Sync active. If off (exclamation point), requested actions happen instantly."
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

        ExtendedButton {
            tooltip: "Deselect all loops."
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