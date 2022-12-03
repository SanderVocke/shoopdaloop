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

    Item {
        anchors.fill: parent

        TabBar {
            id: bar
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top

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
            anchors.top: bar.bottom
            anchors.bottom: parent.bottom
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
                id: midi_settings_table
                TableModelColumn { display: 'Device connection rule' }
                TableModelColumn { display: 'MIDI dialect' }

                rows: {
                    var r = [{ 'Device connection rule': 'Device connection rule', 'MIDI dialect': 'MIDI dialect' }]
                    var keys = Object.keys(active_midi_settings.settings)
                    for (var i=0; i<keys.length; i++) {
                        var autoconnect_rule_name = keys[i]
                        var dialect_name = active_midi_settings.settings[autoconnect_rule_name]
                        r.push({
                            'Device connection rule': autoconnect_rule_name,
                            'MIDI dialect': dialect_name
                        })
                    }
                    for (var key in r) { 
                        console.log(key, r[key])
                    }
                    return r
                }
            }

            delegate: DelegateChooser {
                DelegateChoice {
                    row: 0
                    delegate: Text {
                        text: display
                    }
                }
                DelegateChoice {
                    delegate: ComboBox {
                        implicitWidth: 200
                        implicitHeight: 50

                        //border.width: 2
                        //radius: 4

                        //border.color: 'grey'
                        //color: '#222222'

                        //Text {
                        //    text: display
                        //    color: Material.foreground
                        //    anchors.centerIn: parent
                        //}

                        model: midi_settings_table.rows.map(r => r[Object.keys(r)[column]])
                    }
                }
            }
        }
    }

    component JACKSettings : Item {}

    component MIDIControlSettingsData : QtObject {

        readonly property var builtin_akai_apc_mini_dialect : {
            'substitutions': {
                // Map our loop index to the note identifier on the APC
                'loop_note':  '0 if (track == 0 and loop == 0) else (56+track-1-loop*8)',
                // Map note identifier on APC to our track index
                'note_track': '0 if note == 0 else (note % 8 + 1)',
                // Map note identifier on APC to our loop index within  the track
                'note_loop':  '0 if note == 0 else 7 - (note / 8)',
                // Map fader CC identifier on APC to our track index
                'fader_track': 'controller-48+1' //'controller-48+1 if controller >= 48 and controller < 56'
            },

            'inputRules': {

            },

            'loopStateChangeFormulas': {

            },

            'loop_state_change_default_output_rule': 'noteOn(0, loop_note, 5)', // Unhandled state becomes yellow
            
            'reset_output_rule': 'notesOn(0, 0, 98, 0)' // Everything off
        }

        readonly property var builtin_dialects : {
            'AKAI APC Mini (built-in)': builtin_akai_apc_mini_dialect
        }

        property var user_dialects: {}

        readonly property var builtin_autoconnect_rules: {
            'AKAI APC Mini (built-in)': null
        }

        property var user_autoconnect_rules: {}

        // The actual setting is a map of autoconnect rule names to dialect names.
        readonly property var default_settings: {
            'AKAI APC Mini (built-in)': 'AKAI APC Mini (built-in)'
        }
        property var settings: default_settings
    }
}
