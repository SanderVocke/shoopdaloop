import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 2.15
import QtQuick.Controls.Material 2.15
import MIDIControlDialect 1.0
import MIDIControlInputRule 1.0
import Qt.labs.qmlmodels 1.0

Dialog {
    id: dialog
    modal: true
    title: 'Settings'
    standardButtons: Dialog.Close

    width: 800
    height: 450

    Column {
        anchors.fill: parent
        TabBar {
            id: bar
            anchors.left: parent.left
            anchors.right: parent.right

            TabButton {
                text: 'MIDI control'
            }
            TabButton {
                text: 'JACK'
            }
        }

        StackLayout {
            anchors.left: parent.left
            anchors.right: parent.right
            currentIndex: bar.currentIndex

            MIDISettings {
                id: midi_settings

            }
            JACKSettings {
                id: jack_settings
            }
        }
    }

    component MIDISettings : Item {
        property var active_settings : MIDIControlSettingsData { id: active_midi_settings }

        TableView {
            anchors.fill: parent
            columnSpacing : 1
            rowSpacing : 1
            clip : true

            model: TableModel {
                TableModelColumn { display: 'Device connection rule' }
                TableModelColumn { display: 'MIDI dialect' }

                rows: {
                    var r = []
                    var keys = Object.keys(active_midi_settings.settings)
                    for (var i=0; i<keys.length; i++) {
                        var key = keys[i]
                        var value = active_midi_settings.settings[key]
                        r.push({key, value})
                    }
                    return r
                }
            }

            delegate: Rectangle {
                implicitWidth: 100
                implicitHeight: 50
                border.width: 1

                Text {
                    text: display
                    anchors.centerIn: parent
                }
            }
        }
    }

    component JACKSettings : Item {}

    component MIDIControlSettingsData : QtObject {

        readonly property var builtin_akai_apc_mini_dialect : MIDIControlDialect {
            substitutions: ({
                // Map our loop index to the note identifier on the APC
                'loop_note':  '0 if (track == 0 and loop == 0) else (56+track-1-loop*8)',
                // Map note identifier on APC to our track index
                'note_track': '0 if note == 0 else (note % 8 + 1)',
                // Map note identifier on APC to our loop index within  the track
                'note_loop':  '0 if note == 0 else 7 - (note / 8)',
                // Map fader CC identifier on APC to our track index
                'fader_track': 'controller-48+1' //'controller-48+1 if controller >= 48 and controller < 56'
            })
            inputRules: []
            //    MIDIControlInputRule {}
            //]
            loopStateChangeFormulas: ({

            })
            //loop_state_change_default_output_rule: 'noteOn(0, loop_note, 5)' // Unhandled state becomes yellow
            //reset_output_rule: 'notesOn(0, 0, 98, 0)' // Everything off
        }

        readonly property var builtin_dialects : ({
            'AKAI APC Mini (built-in)': builtin_akai_apc_mini_dialect
        })

        property var user_dialects: []

        readonly property var builtin_autoconnect_rules: ({
            'AKAI APC Mini (built-in)': null
        })

        property var user_autoconnect_rules: ({})

        // The actual setting is a map of autoconnect rule names to dialect names.
        readonly property var default_settings: ({
            'AKAI APC Mini (built-in)': 'AKAI APC Mini (built-in)'
        })
        property var settings: default_settings
    }
}
