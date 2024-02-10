import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

import ShoopConstants

import '../qml_url_to_filename.js' as UrlToFilename
import '../delay.js' as Delay

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

                Instantiator {
                    model : {
                        return global_args.developer ? [true] : []
                    }
                    delegate: Menu {
                        title: "Developer"

                        ShoopMenuItem {
                            text: "Inspect Objects"
                            onClicked: debugwindow.visible = true
                        }

                        ShoopMenuItem {
                            text: "Halt UI (10s)"
                            onClicked: {
                                Delay.blocking_delay(10000)
                            }
                        }

                        ShoopMenuItem {
                            text: "Test throw back-end exception"
                            onClicked: {
                                os_utils.test_exception()
                            }
                        }

                        ShoopMenuItem {
                            text: "Test back-end segfault"
                            onClicked: {
                                os_utils.test_segfault()
                            }
                        }

                        ShoopMenuItem {
                            text: "Test back-end abort"
                            onClicked: {
                                os_utils.test_abort()
                            }
                        }
                    }
                    onObjectAdded: (index, object) => mainmenu.insertMenu(mainmenu.contentData.length, object)
                    onObjectRemoved: (index, object) => mainmenu.removeItem(object)
                }
            }

            ShoopFileDialog {
                id: savesessiondialog
                fileMode: FileDialog.SaveFile
                acceptLabel: 'Save'
                nameFilters: ["ShoopDaLoop session files (*.shl)", "All files (*)"]
                defaultSuffix: 'shl'
                onAccepted: {
                    close()
                    var filename = UrlToFilename.qml_url_to_filename(file.toString());
                    root.saveSession(filename)
                }
            }

            ShoopFileDialog {
                id: loadsessiondialog
                fileMode: FileDialog.OpenFile
                acceptLabel: 'Load'
                nameFilters: ["ShoopDaLoop session files (*.shl)", "All files (*)"]
                onAccepted: {
                    close()
                    var filename = UrlToFilename.qml_url_to_filename(file.toString());
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
                var loops = registries.objects_registry.select_values(o => o instanceof LoopWidget && o.mode !== ShoopConstants.LoopMode.Stopped)
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

        ExtendedButton {
            tooltip: "Clear multiple loops (opens a menu)."
            id: clear_multiple_button
            height: 40
            width: 30

            onClicked: {
                clear_multiple_menu.open()
            }

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'delete'
                color: Material.foreground
            }

            Menu {
                id: clear_multiple_menu

                ShoopMenuItem {
                    text: "Clear recordings"
                    onClicked: {
                        confirm_clear_dialog.text = 'Clear ALL loop recordings?'
                        confirm_clear_dialog.action = () => {
                            var loops = registries.objects_registry.select_values(o => o instanceof LoopWidget && o.maybe_backend_loop)
                            loops.forEach(l => l.clear())
                        }
                        confirm_clear_dialog.open()
                    }
                }
                ShoopMenuItem {
                    text: "Clear recordings except sync"
                    onClicked: {
                        confirm_clear_dialog.text = 'Clear ALL loop recordings except sync?'
                        confirm_clear_dialog.action = () => {
                            var loops = registries.objects_registry.select_values(o => o instanceof LoopWidget && o.maybe_backend_loop && !o.is_sync)
                            loops.forEach(l => l.clear())
                        }
                        confirm_clear_dialog.open()
                    }
                }
                ShoopMenuItem {
                    text: "Clear all"
                    onClicked: {
                        confirm_clear_dialog.text = 'Clear ALL loops?'
                        confirm_clear_dialog.action = () => {
                            var loops = registries.objects_registry.select_values(o => o instanceof LoopWidget)
                            loops.forEach(l => l.clear())
                        }
                        confirm_clear_dialog.open()
                    }
                }
                ShoopMenuItem {
                    text: "Clear all except sync"
                    onClicked: {
                        confirm_clear_dialog.text = 'Clear ALL loops except sync?'
                        confirm_clear_dialog.action = () => {
                            var loops = registries.objects_registry.select_values(o => o instanceof LoopWidget && !o.is_sync)
                            loops.forEach(l => l.clear())
                        }
                        confirm_clear_dialog.open()
                    }
                }
            }

            Dialog {
                id: confirm_clear_dialog
                standardButtons: StandardButton.Yes | StandardButton.Cancel

                implicitWidth: 300

                property var action: () => {}
                property alias text: clear_label.text

                parent: Overlay.overlay
                modal: true
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2

                onAccepted: action()

                title: "Confirm clear"

                Label {
                    id: clear_label
                    anchors.centerIn: parent
                    text: 'hello world'
                }
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