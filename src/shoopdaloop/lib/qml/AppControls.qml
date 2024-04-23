import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import Qt.labs.platform as LabsPlatform

import ShoopConstants

import '../qml_url_to_filename.js' as UrlToFilename
import '../delay.js' as Delay

Item {
    id: root

    signal saveSession(string filename)
    signal loadSession(string filename)
    signal processThreadSegfault()
    signal processThreadAbort()

    property bool loading_session : false
    property bool saving_session : false
    property alias sync_active : sync_active_button.sync_active
    property alias solo_active : solo_active_button.solo_active
    property alias play_after_record_active : play_after_record_active_button.play_after_record_active
    property var backend : null

    property bool settings_io_enabled: false

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
                name: 'menu'
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
                            text: "Test throw exception (UI thread)"
                            onClicked: {
                                os_utils.test_exception()
                            }
                        }

                        ShoopMenuItem {
                            text: "Test segfault (UI thread)"
                            onClicked: {
                                os_utils.test_segfault()
                            }
                        }

                        ShoopMenuItem {
                            text: "Test abort (UI thread)"
                            onClicked: {
                                os_utils.test_abort()
                            }
                        }

                        ShoopMenuItem {
                            text: "Test segfault (process thread)"
                            onClicked: {
                                root.processThreadSegfault()
                            }
                        }

                        ShoopMenuItem {
                            text: "Test abort (process thread)"
                            onClicked: {
                                root.processThreadAbort()
                            }
                        }
                    }
                    onObjectAdded: (index, object) => mainmenu.insertMenu(mainmenu.contentData.length, object)
                    onObjectRemoved: (index, object) => mainmenu.removeItem(object)
                }
            }

            ShoopFileDialog {
                id: savesessiondialog
                fileMode: LabsPlatform.FileDialog.SaveFile
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
                fileMode: LabsPlatform.FileDialog.OpenFile
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

        ToolSeparator {
            orientation: Qt.Vertical
            height: 40
        }

        ExtendedButton {
            tooltip: "Stop all loops (respects sync active)"

            height: 40
            width: 30
            onClicked: {
                var loops = registries.objects_registry.select_values(o => o instanceof LoopWidget && o.mode !== ShoopConstants.LoopMode.Stopped)
                loops[0].transition_loops(
                    loops,
                    ShoopConstants.LoopMode.Stopped,
                    root.sync_active ? 0 : ShoopConstants.DontWaitForSync)
            }

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'stop'
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
                standardButtons: Dialog.Yes | Dialog.Cancel

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
                    text: 'placeholder'
                }
            }
        }

        ToolSeparator {
            orientation: Qt.Vertical
            height: 40
        }

        ExtendedButton {
            tooltip: "Default recording action (record or grab). Affects the default action on empty loops when no specific command is received, e.g. when pressing the spacebar or a MIDI input device triggering the default action."
            id: default_recording_action_button
            height: 40
            width: 30
            onClicked: registries.state_registry.toggle_default_recording_action()
            highlighted : false
            readonly property string value : registries.state_registry.default_recording_action

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors {
                    left: parent.left
                    top: parent.top
                    topMargin: 8
                    leftMargin: 2
                }
                name: parent.value == 'record' ? 'arrow-collapse-down' : 'record'
                color: 'grey'
            }
            MaterialDesignIcon {
                size: parent.value == 'grab' ? (Math.min(parent.width, parent.height) - 10) :
                                               (Math.min(parent.width, parent.height) - 5)
                anchors {
                    right: parent.right
                    bottom: parent.bottom
                    bottomMargin: parent.value == 'grab' ? 10 : 5
                    rightMargin:  parent.value == 'grab' ? 4 : 0
                }
                name: parent.value == 'grab' ? 'arrow-collapse-down' : 'record'
                color: 'red'
            }
        }

        ExtendedButton {
            tooltip: "Play-after-record active. If on (highlighted), a fixed-length record or ringbuffer grab action will automatically transition to playback."
            id: play_after_record_active_button
            height: 40
            width: 30
            property bool play_after_record_active_base: registries.state_registry.play_after_record_active
            onClicked: registries.state_registry.set_play_after_record_active(!play_after_record_active_base)
            property bool play_after_record_active : {
                var rval = play_after_record_active_base
                if (key_modifiers.alt_pressed) { rval = !rval }
                return rval
            }

            highlighted : play_after_record_active

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors {
                    left: parent.left
                    top: parent.top
                    topMargin: 8
                    leftMargin: 2
                }
                name: 'record'
                color: play_after_record_active_button.play_after_record_active ?
                    'red' : 'grey'
            }
            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 5
                anchors {
                    right: parent.right
                    bottom: parent.bottom
                    bottomMargin: 6
                    rightMargin: -1
                }
                name: 'play'
                color: play_after_record_active_button.play_after_record_active ?
                    'green' : Material.foreground
            }
        }

        ExtendedButton {
            tooltip: "Sync active. If off (exclamation point), requested actions happen instantly."
            id: sync_active_button
            height: 40
            width: 30
            property bool sync_active_base: registries.state_registry.sync_active
            onClicked: registries.state_registry.set_sync_active(!sync_active_base)
            property bool sync_active : {
                var rval = sync_active_base
                if (key_modifiers.control_pressed) { rval = !rval }
                return rval
            }

            highlighted : sync_active

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: sync_active_button.sync_active ? 'timer-sand' : 'exclamation';
                color: Material.foreground
            }
        }

        ExtendedButton {
            tooltip: "Solo active. If on (highlighted), requested transitions to loop(s) will stop other loop(s) in the same track(s)."
            id: solo_active_button
            height: 40
            width: 30
            property bool solo_active_base: registries.state_registry.solo_active
            onClicked: registries.state_registry.set_solo_active(!solo_active_base)
            property bool solo_active : {
                var rval = solo_active_base
                if (key_modifiers.shift_pressed) { rval = !rval }
                return rval
            }

            highlighted : solo_active

            Label {
                text: 'S'
                anchors.centerIn: parent
                font.pixelSize: 18
                font.weight: Font.Medium
            }
        }

        SpinBox {
            id: apply_n_cycles_spinbox
            height: 30
            width: 80
            anchors.verticalCenter: parent.verticalCenter

            value: registries.state_registry.apply_n_cycles
            from: -1

            onValueModified: {
                if (value != registries.state_registry.apply_n_cycles) {
                    registries.state_registry.set_apply_n_cycles(value)
                    value = Qt.binding(() => registries.state_registry.apply_n_cycles)
                }
            }
            
            textFromValue: (value) => (value == 0) ? '∞' : value.toString()

            ControlTooltip {
                text: "If set, recording actions will run for the specified fixed amount of cycles."
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