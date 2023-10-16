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
    standardButtons: Dialog.Reset | Dialog.Save | Dialog.Close

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.SettingsDialog" }

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

    component MIDISettings : Settings {
        name: 'MIDISettings'
        schema_name: 'midi_settings'
        current_version: 1

        contents: ({
            'autoconnect_input_regexes': [],
            'autoconnect_output_regexes': [],
            'midi_control_configuration': {
                'schema': 'midi_control_configuration.1',
                'configuration': []
            }
        })
    }

    component MIDISettingsUi : Item {
        id: midi_settings

        property list<string> autoconnect_input_regexes: []
        property list<string> autoconnect_output_regexes: []

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
                                model: midi_settings.autoconnect_input_regexes.length

                                Row {
                                    spacing: 10
                                    height: 40

                                    ShoopTextField {
                                        property string controlledState: midi_settings.autoconnect_input_regexes[index]
                                        onControlledStateChanged: () => { text = controlledState }
                                        onTextChanged: () => { midi_settings.autoconnect_input_regexes[index] = text }
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
                                            midi_settings.autoconnect_input_regexes.splice(index, 1)
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
                                    midi_settings.autoconnect_input_regexes.push('')
                                    midi_settings.autoconnect_input_regexesChanged()
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
                                model: midi_settings.autoconnect_output_regexes.length
                                onModelChanged: console.log('model changed')

                                Row {
                                    spacing: 10
                                    height: 40

                                    ShoopTextField {
                                        property string controlledState: midi_settings.autoconnect_output_regexes[index]
                                        onControlledStateChanged: () => { text = controlledState }
                                        onTextChanged: () => { midi_settings.autoconnect_output_regexes[index] = text }
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
                                    midi_settings.autoconnect_output_regexes.push('')
                                    midi_settings.autoconnect_output_regexesChanged()
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
                        }
                    }
                }
            }
        }
    }
}
