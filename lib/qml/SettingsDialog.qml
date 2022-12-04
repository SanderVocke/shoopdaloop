import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 2.15
import QtQuick.Controls.Material 2.15
import MIDIControlDialect 1.0
import MIDIControlInputRule 1.0
import Qt.labs.qmlmodels 1.0

import '../../build/StatesAndActions.js' as StatesAndActions

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

        Row {
            spacing: 2
            property var active_profile: profile_selector.data

            Text {
                anchors.verticalCenter: profile_selector.verticalCenter
                color: Material.foreground
                text: 'Showing MIDI control profile:'
            }
            ComboBox {
                id: profile_selector
                property var data: active_settings.profiles
                model: data.map((element, index) => index) // Create array of indices only
                width: 300
                displayText: {
                    console.log(data)
                    console.log(data[currentIndex])
                    console.log(data[currentIndex][0])
                    return data[currentIndex][0].name
                }

                delegate: ItemDelegate {
                    width: profile_selector.width
                    contentItem: Text {
                        property var data : profile_selector.data[modelData]
                        text: data[0].name + (data[1] ? '' : ' (inactive)')
                        color: data[1] ? Material.foreground : 'grey'
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
            CheckBox {
                property bool controlledState: parent.active_profile[1]
                onControlledStateChanged: () => {
                    console.log('controlled to', controlledState)
                    checkState = controlledState ? Qt.Checked : Qt.Unchecked
                    console.log('=>', checkState)
                }
                onCheckStateChanged: () => {
                    console.log('check to', checkState)
                    active_settings.profiles[profile_selector.currentIndex][1] = checkState == Qt.Checked
                    console.log('=>', active_settings.profiles[profile_selector.currentIndex][1])
                }
            }
        }
    }

    component JACKSettings : Item {}

    component MIDIControlSettingsData : QtObject {

        readonly property var builtin_akai_apc_mini_profile : {
            'name': 'AKAI APC Mini',

            'midi_in_regex': '.*APC MINI MIDI.*',
            'midi_out_regex': '.*APC MINI MIDI.*',

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

            'inputRules': [
                // Input rule signature:
                // [
                //    { filter_type1: filter_arg1, filter_type2: filter_arg2, ...},
                //    [ condition_formula1, condition_formula2, ...]
                //    handler_formula
                // ]
                [
                    {[StatesAndActions.MIDIMessageFilterType.IsNoteOn]: null},
                    ['note <= 64'],
                    'loopAction(note_track, note_loop, play_or_stop)'
                ],
                [
                    {[StatesAndActions.MIDIMessageFilterType.IsCCKind]: null},
                    ['48 <= controller < 56'],
                    'setVolume(fader_track, value/127.0)'
                ],
            ],

            'loopStateChangeFormulas': {
                // Signature { loop_state: handler_formula, ... }
                [StatesAndActions.LoopState.Recording]: 'noteOn(0, loop_note, 3)',
                [StatesAndActions.LoopState.Playing]: 'noteOn(0, loop_note, 1)',
                [StatesAndActions.LoopState.Stopped]: 'noteOn(0, loop_note, 0)',
                [StatesAndActions.LoopState.PlayingMuted]: 'noteOn(0, loop_note, 0)'
            },

            'loop_state_change_default_output_rule': 'noteOn(0, loop_note, 5)', // Unhandled state becomes yellow
            
            'reset_output_rule': 'notesOn(0, 0, 98, 0)' // Everything off
        }

        // The actual setting is a map of autoconnect rule names to dialect names.
        readonly property var default_profiles: [
            // Signature: [profile, active]
            [ builtin_akai_apc_mini_profile, true ]
        ]

        property var profiles: default_profiles
    }
}
