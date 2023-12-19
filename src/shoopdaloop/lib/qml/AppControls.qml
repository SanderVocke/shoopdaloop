import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtQuick.Dialogs

import ShoopConstants

import '../qml_url_to_filename.js' as UrlToFilename

Item {
    id: root

    signal saveSession(string filename)
    signal loadSession(string filename)

    property bool loading_session : false
    property bool saving_session : false
    property alias sync_active : sync_active_button.sync_active
    property var backend : null

    property bool settings_io_enabled: false

    function update() {
        registries.state_registry.replace('sync_active', sync_active)
    }

    onSync_activeChanged: update()
    Component.onCompleted: update()

    Row {
        spacing: 6
        anchors.fill: parent

        ExtendedButton {
            height: 40
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

                ShoopMenuItem {
                    text: "Save session"
                    onClicked: { savesessiondialog.open() }
                }
                ShoopMenuItem {
                    text: "Load session"
                    onClicked: loadsessiondialog.open()
                }
                ShoopMenuItem {
                    text: "Profiling"
                    onClicked: profilingwindow.visible = true
                }
                ShoopMenuItem {
                    text: "Settings"
                    onClicked: settings_dialog.open()
                }
                ShoopMenuItem {
                    text: "Debug Inspection"
                    onClicked: debugwindow.visible = true
                }
            }

            FileDialog {
                id: savesessiondialog
                fileMode: FileDialog.SaveFile
                acceptLabel: 'Save'
                nameFilters: ["ShoopDaLoop session files (*.shl)", "All files (*)"]
                defaultSuffix: 'shl'
                onAccepted: {
                    close()
                    var filename = UrlToFilename.qml_url_to_filename(selectedFile.toString());
                    root.saveSession(filename)
                }
                
            }

            FileDialog {
                id: loadsessiondialog
                fileMode: FileDialog.OpenFile
                acceptLabel: 'Load'
                nameFilters: ["ShoopDaLoop session files (*.shl)", "All files (*)"]
                onAccepted: {
                    close()
                    var filename = UrlToFilename.qml_url_to_filename(selectedFile.toString());
                    root.loadSession(filename)
                }
                
            }

            ProfilingWindow {
                id: profilingwindow
                backend: root.backend
            }

            DebugInspectionMainWindow {
                id: debugwindow
            }
        }

        ExtendedButton {
            tooltip: "Stop all loops (respects sync active)"

            height: 40
            width: 30
            onClicked: {
                var loops = registries.objects_registry.select_values(o => o instanceof LoopWidget)
                loops[0].transition_loops(loops, ShoopConstants.LoopMode.Stopped, 0, root.sync_active)
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
            height: 40
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
            height: 40
            width: 30
            onClicked: registries.state_registry.clear_set('selected_loop_ids')

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'border-none-variant'
                color: Material.foreground
            }
        }
    }

    RegistryLookup {
        id: registry_lookup
        registry: registries.state_registry
        key: 'midi_control_port'
    }

    SettingsDialog {
        id: settings_dialog
        io_enabled: root.settings_io_enabled
    }
}