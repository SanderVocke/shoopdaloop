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

    width: 800
    height: 500

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

            MIDISettings {
                id: midi_settings_being_edited
            }
        }
    }

    component MIDISettings : Item {
        id: midi_settings

        function serialize() {
            return JSON.stringify({
                'autoconnect_input_regexes': midi_settings.autoconnect_input_regexes,
                'autoconnect_output_regexes': midi_settings.autoconnect_output_regexes
            })
        }
        function deserialize() {
            var data = JSON.parse(midi_settings_being_edited.serialize())
            autoconnect_input_regexes = data['autoconnect_input_regexes']
            autoconnect_output_regexes = data['autoconnect_output_regexes']
        }

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


    // component MIDIControlSettingsData : QtObject {

    //     readonly property var builtin_akai_apc_mini_profile : MIDIControlProfile {
    //         name: 'AKAI APC Mini'

    //         midi_in_regex: '.*APC MINI MIDI.*'
    //         midi_out_regex: '.*APC MINI MIDI.*'

    //         substitutions: {
    //             // Map our loop index to the note identifier on the APC
    //             'loop_note':  '0 if (track == 0 and loop == 0) else (56+track-1-loop*8)',
    //             // Map note identifier on APC to our track index
    //             'note_track': '0 if note == 0 else (note % 8 + 1)',
    //             // Map note identifier on APC to our loop index within  the track
    //             'note_loop':  '0 if note == 0 else 7 - (note / 8)',
    //             // Map fader CC identifier on APC to our track index
    //             'fader_track': 'controller-48+1' //'controller-48+1 if controller >= 48 and controller < 56'
    //         }

    //         input_rules: [
    //             {
    //                 'filter': StatesAndActions.MIDIMessageFilterType.IsNoteOn,
    //                 'condition': 'note <= 64',
    //                 'action': 'loopAction(note_track, note_loop, play_or_stop)'
    //             },
    //             {
    //                 'filter': StatesAndActions.MIDIMessageFilterType.IsCCKind,
    //                 'condition': '48 <= controller < 56',
    //                 'action': 'setVolume(fader_track, value/127.0)'
    //             }
    //         ]

    //         loop_state_change_formulas: [
    //             { 'mode': StatesAndActions.LoopMode.Recording, 'action': 'noteOn(0, loop_note, 3)' },
    //             { 'mode': StatesAndActions.LoopMode.Playing, 'action': 'noteOn(0, loop_note, 1)' },
    //             { 'mode': StatesAndActions.LoopMode.Stopped, 'action': 'noteOn(0, loop_note, 0)' },
    //             { 'mode': StatesAndActions.LoopMode.PlayingMuted, 'action': 'noteOn(0, loop_note, 0)' }
    //         ]

    //         default_loop_state_change_formula: 'noteOn(0, loop_note, 5)' // Unhandled mode becomes yellow
            
    //         reset_formula: 'notesOn(0, 0, 98, 0)' // Everything off
    //     }
}
