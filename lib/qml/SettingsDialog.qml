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
    height: 500

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
            id: profile_select_row
            spacing: 5
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }

            Label {
                anchors.verticalCenter: profile_selector.verticalCenter
                verticalAlignment: Text.AlignVCenter
                color: Material.foreground
                text: 'Showing MIDI control profile:'
            }

            ComboBox {
                id: profile_selector
                property var data: active_settings.profiles
                property var active_profile_entry: currentIndex >= 0 ? data[currentIndex] : null
                model: Object.keys(data).map((element, index) => index) // Create array of indices only
                width: 300
                
                contentItem: Item {
                    anchors.fill: parent
                    Label {
                        anchors.fill: parent
                        anchors.leftMargin: 10
                        color: profile_selector.active_profile_entry.active ? Material.foreground : 'grey'
                        verticalAlignment: Text.AlignVCenter
                        text: profile_selector.active_profile_entry.profile.name +
                            (profile_selector.active_profile_entry.active ? '' : ' (inactive)')
                    }
                }

                delegate: ItemDelegate {
                    width: profile_selector.width
                    contentItem: Text {
                        property var entry : profile_selector.data[modelData]
                        text: entry.profile.name + (entry.active ? '' : ' (inactive)')
                        color: entry.active ? Material.foreground : 'grey'
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
            CheckBox {
                text: 'Profile active'
                property bool controlledState: profile_selector.active_profile_entry.active
                Component.onCompleted: checkState = controlledState ? Qt.Checked : Qt.Unchecked
                onControlledStateChanged: () => { checkState = controlledState ? Qt.Checked : Qt.Unchecked }
                onCheckStateChanged: () => { active_settings.profiles[profile_selector.currentIndex].active = checkState == Qt.Checked }
            }
        }

        ToolSeparator {
            id: separator

            anchors {
                top: profile_select_row.bottom
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
                        title: 'JACK port autoconnect'
                        width: parent.width

                        Row {
                            anchors.fill:parent
                            spacing: 10

                            Label {
                                text: 'Input port regex:'
                                verticalAlignment: Text.AlignVCenter
                                anchors.verticalCenter: tf1.verticalCenter
                            }
                            TextField {
                                id: tf1
                                placeholderText: 'input port regex'
                                property string controlledState: profile_selector.active_profile_entry.profile.midi_in_regex
                                onControlledStateChanged: () => { text = controlledState }
                                onTextChanged: () => { profile_selector.active_profile_entry.profile.midi_in_regex = text }
                                Component.onCompleted: () => { text = controlledState }
                                width: 190
                            }
                            Label {
                                text: 'Output port regex:'
                                verticalAlignment: Text.AlignVCenter
                                anchors.verticalCenter: tf1.verticalCenter
                            }
                            TextField {
                                placeholderText: 'output port regex'
                                property string controlledState: profile_selector.active_profile_entry.profile.midi_out_regex
                                onControlledStateChanged: () => { text = controlledState }
                                onTextChanged: () => { profile_selector.active_profile_entry.profile.midi_out_regex = text }
                                Component.onCompleted: () => { text = controlledState }
                                width: 190
                            }
                        }
                    }

                    GroupBox {
                        title: 'Formula substitutions'
                        width: parent.width
                        height: 350

                        TableView {
                            height: 300

                            model: TableModel {
                                TableModelColumn { display: 'from' }
                                TableModelColumn { display: 'to' }

                                rows: Object.entries(
                                    profile_selector.active_profile_entry.profile.substitutions
                                ).map((entry) => {
                                    return {
                                        'from': entry[0],
                                        'to': entry[1]
                                    }
                                })
                            }

                            delegate: DelegateChooser {
                                DelegateChoice {
                                    column: 0
                                    ItemDelegate {
                                        TextField {
                                            width: 120
                                            text: Object.keys(
                                                profile_selector.active_profile_entry.profile.substitutions
                                            )[row]
                                        }
                                    }
                                }
                                DelegateChoice {
                                    column: 1
                                    ItemDelegate {
                                        TextField {
                                            width: 300
                                            text: Object.values(
                                                profile_selector.active_profile_entry.profile.substitutions
                                            )[row]
                                        }
                                    }
                                }
                            }
                        }
                    }

                    GroupBox {
                        title: 'MIDI input handling'
                        width: parent.width
                    }

                    GroupBox {
                        title: 'Loop state change event handling'
                        width: parent.width
                    }

                    GroupBox {
                        title: 'Other event handling'
                        width: parent.width
                    }
                }
            }
        }
    }

    component SubsettingsLabel: Label {
        verticalAlignment: Text.AlignVCenter
        font.bold: true
        font.pixelSize: 15
    }

    component JACKSettings : Item {}

    component MIDIControlProfile : Item {
        property string name
        property string midi_in_regex
        property string midi_out_regex
        property var substitutions
        property var input_rules
        property var loop_state_change_formulas
        property string default_loop_state_change_formula
        property string reset_formula
    }

    component MIDIControlProfileEntry : Item {
        property var profile
        property bool active
    }

    component MIDIControlSettingsData : QtObject {

        readonly property var builtin_akai_apc_mini_profile : MIDIControlProfile {
            name: 'AKAI APC Mini'

            midi_in_regex: '.*APC MINI MIDI.*'
            midi_out_regex: '.*APC MINI MIDI.*'

            substitutions: {
                // Map our loop index to the note identifier on the APC
                'loop_note':  '0 if (track == 0 and loop == 0) else (56+track-1-loop*8)',
                // Map note identifier on APC to our track index
                'note_track': '0 if note == 0 else (note % 8 + 1)',
                // Map note identifier on APC to our loop index within  the track
                'note_loop':  '0 if note == 0 else 7 - (note / 8)',
                // Map fader CC identifier on APC to our track index
                'fader_track': 'controller-48+1' //'controller-48+1 if controller >= 48 and controller < 56'
            }

            input_rules: [
                // Input rule signature:
                // [
                //    { filter_type1: filter_arg1, filter_type2: filter_arg2, ...},
                //    condition_formula,
                //    handler_formula
                // ]
                [
                    {[StatesAndActions.MIDIMessageFilterType.IsNoteOn]: null},
                    'note <= 64',
                    'loopAction(note_track, note_loop, play_or_stop)'
                ],
                [
                    {[StatesAndActions.MIDIMessageFilterType.IsCCKind]: null},
                    '48 <= controller < 56',
                    'setVolume(fader_track, value/127.0)'
                ]
            ]

            loop_state_change_formulas: ({
                // Signature { loop_state: handler_formula, ... }
                [StatesAndActions.LoopState.Recording]: 'noteOn(0, loop_note, 3)',
                [StatesAndActions.LoopState.Playing]: 'noteOn(0, loop_note, 1)',
                [StatesAndActions.LoopState.Stopped]: 'noteOn(0, loop_note, 0)',
                [StatesAndActions.LoopState.PlayingMuted]: 'noteOn(0, loop_note, 0)'
            })

            default_loop_state_change_formula: 'noteOn(0, loop_note, 5)' // Unhandled state becomes yellow
            
            reset_formula: 'notesOn(0, 0, 98, 0)' // Everything off
        }

        // The actual setting is a map of autoconnect rule names to dialect names.
        property list<Item> default_profiles: [
            MIDIControlProfileEntry {
                profile: builtin_akai_apc_mini_profile
                active: true
            }
        ]

        property var profiles: default_profiles
    }
}
