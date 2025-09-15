import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import Qt.labs.platform as LabsPlatform
import ShoopDaLoop.Rust

import 'js/qml_url_to_filename.js' as UrlToFilename
import 'js/delay.js' as Delay

Item {
    id: root

    signal saveSession(string filename)
    signal loadSession(string filename)
    signal processThreadSegfault()
    signal processThreadAbort()
    signal openConnections()

    property alias sync_active : sync_active_button.sync_active
    property alias solo_active : solo_active_button.solo_active
    property alias play_after_record_active : play_after_record_active_button.play_after_record_active
    property var backend : null

    property ShoopRustLogger logger : ShoopRustLogger { name: "Frontend.Qml.AppControls" }

    onSync_activeChanged: {
        logger.debug("Sync active changed to " + sync_active)
    }
    onSolo_activeChanged: {
        logger.debug("Solo active changed to " + solo_active)
    }
    onPlay_after_record_activeChanged: {
        logger.debug("Play after record active changed to " + play_after_record_active)
    }

    property bool settings_io_enabled: false

    function add_dsp_load_point(dsp_load) {
        monitorwindow.add_dsp_load_point(dsp_load)
    }
    function add_audio_buffer_pool_point(buffers_created, buffers_available) {
        monitorwindow.add_audio_buffer_pool_point(buffers_created, buffers_available)
    }
    function add_backend_refresh_interval_point(interval) {
        monitorwindow.add_backend_refresh_interval_point(interval)
    }

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
                    text: "Connections"
                    onClicked: { root.openConnections() }
                }
                ShoopMenuItem {
                    text: "Save session"
                    onClicked: { savesessiondialog.open() }
                }
                ShoopMenuItem {
                    text: "Load session"
                    onClicked: loadsessiondialog.open()
                }
                ShoopMenuItem {
                    text: "Audio Profiling"
                    onClicked: profilingwindow.visible = true
                }
                ShoopMenuItem {
                    text: "Monitoring"
                    onClicked: monitorwindow.visible = true
                }
                ShoopMenuItem {
                    text: "Settings"
                    onClicked: settings_window.visible = true
                }

                Instantiator {
                    model : {
                        return global_args.developer_mode ? [true] : []
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
                            text: "Test thread panic (UI thread)"
                            onClicked: {
                                ShoopRustOSUtils.cause_panic()
                            }
                        }

                        ShoopMenuItem {
                            text: "Test segfault (UI thread)"
                            onClicked: {
                                ShoopRustOSUtils.cause_segfault()
                            }
                        }

                        ShoopMenuItem {
                            text: "Test abort (UI thread)"
                            onClicked: {
                                ShoopRustOSUtils.cause_abort()
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

            MonitorWindow {
                id: monitorwindow
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
                var loops = AppRegistries.objects_registry.select_values(o => o.objectName === "Qml.LoopWidget" && o.mode !== ShoopRustConstants.LoopMode.Stopped)
                if (loops.length > 0) {
                    loops[0].transition_loops(
                        loops,
                        ShoopRustConstants.LoopMode.Stopped,
                        root.sync_active ? 0 : ShoopRustConstants.DontWaitForSync)
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
            tooltip: "Deselect all loops."
            id: deselect_button
            height: 40
            width: 30
            onClicked: AppRegistries.state_registry.clear_set('selected_loop_ids')

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
                            var loops = AppRegistries.objects_registry.select_values(o => o.objectName === "Qml.LoopWidget" && o.maybe_backend_loop)
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
                            var loops = AppRegistries.objects_registry.select_values(o => o.objectName === "Qml.LoopWidget" && o.maybe_backend_loop && !o.is_sync)
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
                            var loops = AppRegistries.objects_registry.select_values(o => o.objectName === "Qml.LoopWidget")
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
                            var loops = AppRegistries.objects_registry.select_values(o => o.objectName === "Qml.LoopWidget" && !o.is_sync)
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
            onClicked: AppRegistries.state_registry.toggle_default_recording_action()
            highlighted : false
            readonly property string value : AppRegistries.state_registry.default_recording_action

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
            onClicked: state = !state

            property bool inverted : ShoopRustKeyModifiers.alt_pressed
            property bool state : true
            property bool play_after_record_active : inverted ? !state : state

            onPlay_after_record_activeChanged: AppRegistries.state_registry.set_play_after_record_active(play_after_record_active)
            Component.onCompleted: AppRegistries.state_registry.set_play_after_record_active(play_after_record_active)

            Connections {
                target: AppRegistries.state_registry
                function onPlay_after_record_activeChanged() {
                    let v = AppRegistries.state_registry.play_after_record || false
                    play_after_record_active_button.state = play_after_record_active_button.inverted ? !v : v
                }
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
            onClicked: state = !state

            property bool inverted : ShoopRustKeyModifiers.control_pressed
            property bool state : true
            property bool sync_active : inverted ? !state : state

            onSync_activeChanged: AppRegistries.state_registry.set_sync_active(sync_active)
            Component.onCompleted: AppRegistries.state_registry.set_sync_active(sync_active)

            Connections {
                target: AppRegistries.state_registry
                function onSync_activeChanged() {
                    let v = AppRegistries.state_registry.sync_active
                    sync_active_button.state = sync_active_button.inverted ? !v : v
                }
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
            onClicked: state = !state

            property bool inverted : ShoopRustKeyModifiers.shift_pressed
            property bool state : false
            property bool solo_active : inverted ? !state : state

            onSolo_activeChanged: AppRegistries.state_registry.set_solo_active(solo_active)
            Component.onCompleted: AppRegistries.state_registry.set_solo_active(solo_active)

            Connections {
                target: AppRegistries.state_registry
                function onSolo_activeChanged() {
                    let v = AppRegistries.state_registry.solo_active
                    solo_active_button.state = solo_active_button.inverted ? !v : v
                }
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

            value: AppRegistries.state_registry.apply_n_cycles
            from: -1

            editable: true
            valueFromText: function(text, locale) {
                ShoopRustReleaseFocusNotifier.notify()
                return Number.fromLocaleString(locale, text);
            }

            onValueModified: {
                if (value != AppRegistries.state_registry.apply_n_cycles) {
                    AppRegistries.state_registry.set_apply_n_cycles(value)
                    value = Qt.binding(() => AppRegistries.state_registry.apply_n_cycles)
                }
            }

            textFromValue: (value) => (value == 0) ? '∞' : value.toString()

            ControlTooltip {
                text: "If set, recording actions will run for the specified fixed amount of cycles."
            }
        }
    }

    SettingsWindow {
        id: settings_window
        io_enabled: root.settings_io_enabled
    }
}
