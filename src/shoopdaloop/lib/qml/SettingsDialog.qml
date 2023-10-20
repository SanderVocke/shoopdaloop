import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Layouts 6.3
import QtQuick.Controls.Material 6.3
import Qt.labs.qmlmodels 1.0

import ShoopDaLoop.PythonLogger

Dialog {
    id: root
    modal: true
    title: 'Settings'
    standardButtons: Dialog.Save | Dialog.Discard | Dialog.Close

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.SettingsDialog" }

    onAccepted: all_settings.save()
    onDiscarded: { all_settings.load(); close() }

    width: Overlay.overlay ? Overlay.overlay.width - 50 : 800
    height: Overlay.overlay ? Overlay.overlay.height - 50 : 500
    anchors.centerIn: Overlay.overlay ? Overlay.overlay : parent

    Item {
        anchors.fill: parent

        TabBar {
            id: bar
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top

            TabButton {
                text: 'MIDI Control'
            }
        }

        StackLayout {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: bar.bottom
            anchors.bottom: parent.bottom
            currentIndex: bar.currentIndex

            MIDISettingsUi {
                id: midi_settings_being_edited
            }
        }
    }

    Settings {
        id: all_settings
        name: "AllSettings"
        schema_name: "settings"
        current_version: 1

        readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.AllSettings" }

        function save() {
            logger.debug(() => ("Saving settings."))
            validate()
            settings_io.save_settings(to_dict(), null)
        }

        function load() {
            logger.debug(() => ("Loading settings."))
            let loaded_settings = settings_io.load_settings(null)
            if (loaded_settings != null) { from_dict(loaded_settings) }
        }

        Component.onCompleted: load()

        contents: ({
            'midi_settings': {
                'schema': 'midi_settings.1',
                'configuration': midi_settings.default_contents()
            },
            'script_settings': {
                'schema': 'script_settings.1',
                'configuration': script_settings.default_contents()
            }
        })
    }

    Settings {
        id: midi_settings
        name: 'MIDISettings'
        schema_name: 'midi_settings'
        current_version: 1

        contents: all_settings.contents.midi_settings.configuration

        function default_contents() { return ({
            'autoconnect_input_regexes': [],
            'autoconnect_output_regexes': [],
            'midi_control_configuration': {
                'schema': 'midi_control_configuration.1',
                'configuration': []
            }
        }) }
    }

    ScriptSettings {
        id: script_settings
        contents: all_settings.contents.script_settings.configuration
    }

    component MIDISettingsUi : Item {
        id: midi_settings_ui

        onAutoconnect_input_regexesChanged: midi_settings.contents.autoconnect_input_regexes = autoconnect_input_regexes
        onAutoconnect_output_regexesChanged: midi_settings.contents.autoconnect_output_regexes = autoconnect_output_regexes

        property list<string> autoconnect_input_regexes: midi_settings.contents ? midi_settings.contents.autoconnect_input_regexes : []
        property list<string> autoconnect_output_regexes: midi_settings.contents ? midi_settings.contents.autoconnect_output_regexes : []

        RegisterInRegistry {
            registry: registries.state_registry
            key: 'autoconnect_input_regexes'
            object: autoconnect_input_regexes
        }
        RegisterInRegistry {
            registry: registries.state_registry
            key: 'autoconnect_output_regexes'
            object: autoconnect_output_regexes
        }

        Column {
            id: header

            Label {
                text: 'For detailed information about MIDI control settings, see the <a href="unknown.html">help</a>.'
                onLinkActivated: (link) => Qt.openUrlExternally(link)
            }
        }

        ToolSeparator {
            id: separator

            anchors {
                top: header.bottom
                left: parent.left
                right: parent.right
            }

            orientation: Qt.Horizontal
        }

        ScrollView {            
            anchors {
                top: separator.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }

            contentHeight: scrollcontent.height

            ScrollBar.vertical.policy: ScrollBar.AlwaysOn

            Item {
                id: scrollcontent

                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                }

                height: childrenRect.height
                
                Column {
                    spacing: 10
                    anchors {
                        top:parent.top
                        left: parent.left
                        right: parent.right
                        rightMargin: 25
                    }

                    GroupBox {
                        title: 'JACK control autoconnect'
                        width: parent.width
                        height: Math.max(input_regexes_column.height, output_regexes_column.height) + 50

                        Column {
                            id: input_regexes_column
                            anchors.left: parent.left
                            anchors.right: parent.horizontalCenter
                            spacing: 3

                            Label {
                                text: 'Input device name regexes'
                            }
                            Repeater {
                                model: midi_settings_ui.autoconnect_input_regexes.length

                                Row {
                                    spacing: 10
                                    height: 40

                                    ShoopTextField {
                                        property string controlledState: midi_settings_ui.autoconnect_input_regexes[index]
                                        onControlledStateChanged: () => { text = controlledState }
                                        onTextChanged: () => { midi_settings_ui.autoconnect_input_regexes[index] = text }
                                        Component.onCompleted: () => { text = controlledState }
                                        width: 190
                                    }
                                    ExtendedButton {
                                        tooltip: "Remove."
                                        width: 30
                                        height: 40
                                        MaterialDesignIcon {
                                            size: 20
                                            name: 'delete'
                                            color: Material.foreground
                                            anchors.centerIn: parent
                                        }
                                        onClicked: {
                                            midi_settings_ui.autoconnect_input_regexes.splice(index, 1)
                                        }
                                    }
                                }
                            }
                            ExtendedButton {
                                tooltip: "Add an input port regex."
                                width: 30
                                height: 40
                                MaterialDesignIcon {
                                    size: 20
                                    name: 'plus'
                                    color: Material.foreground
                                    anchors.centerIn: parent
                                }
                                onClicked: {
                                    midi_settings_ui.autoconnect_input_regexes.push('')
                                    midi_settings_ui.autoconnect_input_regexesChanged()
                                }
                            }
                        }

                        Column {
                            id: output_regexes_column
                            anchors.left: parent.horizontalCenter
                            anchors.right: parent.right
                            spacing: 3
                            
                            Label {
                                text: 'Output device name regexes'
                            }
                            Repeater {
                                model: midi_settings_ui.autoconnect_output_regexes.length

                                Row {
                                    spacing: 10
                                    height: 40

                                    ShoopTextField {
                                        property string controlledState: midi_settings_ui.autoconnect_output_regexes[index]
                                        onControlledStateChanged: () => { text = controlledState }
                                        onTextChanged: () => { midi_settings_ui.autoconnect_output_regexes[index] = text }
                                        Component.onCompleted: () => { text = controlledState }
                                        width: 190
                                    }
                                    ExtendedButton {
                                        tooltip: "Remove."
                                        width: 30
                                        height: 40
                                        MaterialDesignIcon {
                                            size: 20
                                            name: 'delete'
                                            color: Material.foreground
                                            anchors.centerIn: parent
                                        }
                                        onClicked: {
                                            midi_settings.autoconnect_output_regexes.splice(index, 1)
                                        }
                                    }
                                }
                            }
                            ExtendedButton {
                                tooltip: "Add an output port regex."
                                width: 30
                                height: 40
                                MaterialDesignIcon {
                                    size: 20
                                    name: 'plus'
                                    color: Material.foreground
                                    anchors.centerIn: parent
                                }
                                onClicked: {
                                    midi_settings_ui.autoconnect_output_regexes.push('')
                                    midi_settings_ui.autoconnect_output_regexesChanged()
                                }
                            }
                        }
                    }

                    GroupBox {
                        title: 'MIDI control triggers'
                        width: parent.width

                        EditMidiControl {
                            id: edit_midi_control
                            width: parent.width

                            Connections {
                                target: edit_midi_control.configuration
                                function onContentsChanged() {
                                    all_settings.contents.midi_settings.configuration.midi_control_configuration = edit_midi_control.configuration.to_dict()
                                }
                            }

                            Connections {
                                target: all_settings
                                function onContentsChanged() {
                                    edit_midi_control.configuration.from_dict(all_settings.contents.midi_settings.configuration.midi_control_configuration)
                                }
                            }

                            RegisterInRegistry {
                                registry: registries.state_registry
                                key: 'midi_control_configuration'
                                object: edit_midi_control.configuration
                            }
                        }
                    }
                }
            }
        }
    }
}
