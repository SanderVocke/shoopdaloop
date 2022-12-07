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
    standardButtons: Dialog.Reset | Dialog.Save | Dialog.Close

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
        id: midi_settings
        property var active_settings : MIDIControlSettingsData { id: active_midi_settings }
        property var active_profile_entry : active_settings.profiles[profile_selector.currentIndex]

        Column {
            id: header

            Label {
                text: 'For detailed information about MIDI control settings, see the <a href="help_midi.html">help</a>.'
                onLinkActivated: (link) => Qt.openUrlExternally(mainScriptDir + '/resources/help/' + link)
            }

            Row {
                id: profile_select_row
                spacing: 5
                anchors {
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
                    model: Object.keys(midi_settings.active_settings.profiles).map((element, index) => index) // Create array of indices only
                    width: 300
                    
                    contentItem: Item {
                        anchors.fill: parent
                        Label {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            color: midi_settings.active_profile_entry.active ? Material.foreground : 'grey'
                            verticalAlignment: Text.AlignVCenter
                            text: midi_settings.active_profile_entry.profile.name +
                                (midi_settings.active_profile_entry.active ? '' : ' (inactive)')
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
                    property bool controlledState: midi_settings.active_profile_entry.active
                    Component.onCompleted: checkState = controlledState ? Qt.Checked : Qt.Unchecked
                    onControlledStateChanged: () => { checkState = controlledState ? Qt.Checked : Qt.Unchecked }
                    onCheckStateChanged: () => { active_settings.profiles[profile_selector.currentIndex].active = checkState == Qt.Checked }
                }
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
                                property string controlledState: midi_settings.active_profile_entry.profile.midi_in_regex
                                onControlledStateChanged: () => { text = controlledState }
                                onTextChanged: () => { midi_settings.active_profile_entry.profile.midi_in_regex = text }
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
                                property string controlledState: midi_settings.active_profile_entry.profile.midi_out_regex
                                onControlledStateChanged: () => { text = controlledState }
                                onTextChanged: () => { midi_settings.active_profile_entry.profile.midi_out_regex = text }
                                Component.onCompleted: () => { text = controlledState }
                                width: 190
                            }
                        }
                    }

                    GroupBox {
                        title: 'Formula substitutions'
                        width: parent.width
                        
                        Column {
                            spacing: 3
                            id: substitutions_column
                            width: parent.width

                            Repeater {
                                model: midi_settings.active_profile_entry ?
                                    Object.entries(midi_settings.active_profile_entry.profile.substitutions) :
                                    []
                                
                                Rectangle {
                                    radius: 3
                                    
                                    color: Material.background
                                    border.color: 'grey'
                                    border.width: 1

                                    width: substitutions_column.width
                                    height: 42

                                    TextField {
                                        id: from_field
                                        anchors.leftMargin: 10
                                        anchors.left: parent.left
                                        width: 150
                                        text: modelData[0]
                                    }
                                    Label {
                                        id: tolabel
                                        verticalAlignment: Text.AlignVCenter
                                        text: '=>'
                                        anchors {
                                            verticalCenter: from_field.verticalCenter
                                            left: from_field.right
                                            leftMargin: 10
                                        }
                                    }
                                    TextField {
                                        anchors.left: tolabel.right
                                        anchors.leftMargin: 10
                                        anchors.right: deletebutton.left
                                        anchors.rightMargin: 10
                                        text: modelData[1]
                                    }
                                    Button {
                                        id: deletebutton
                                        anchors {
                                            verticalCenter: tolabel.verticalCenter
                                            right: parent.right
                                            rightMargin: 10
                                        }
                                        width: height
                                        height: 40

                                        MaterialDesignIcon {
                                            size: parent.width - 20
                                            name: 'delete'
                                            color: Material.foreground

                                            anchors {
                                                verticalCenter: parent.verticalCenter
                                                horizontalCenter: parent.horizontalCenter
                                            }
                                        }

                                        onClicked: {
                                            delete midi_settings.active_profile_entry.profile.substitutions[modelData[0]]
                                            midi_settings.active_profile_entry.profile.substitutionsChanged()
                                        }
                                    }
                                }
                            }

                            Button {
                                text: 'Add'
                                onClicked: {
                                    midi_settings.active_profile_entry.profile.substitutions[''] = ''
                                    midi_settings.active_profile_entry.profile.substitutionsChanged()
                                }
                            }
                        }
                    }

                    GroupBox {
                        title: 'MIDI input handling'
                        width: parent.width

                        Column {
                            spacing: 3
                            id: input_rules_column
                            width: parent.width

                            Repeater {
                                model: midi_settings.active_profile_entry.profile.input_rules
                                
                                Rectangle {
                                    radius: 3
                                    
                                    color: Material.background
                                    border.color: 'grey'
                                    border.width: 1

                                    width: input_rules_column.width
                                    height: 52

                                    ComboBox {
                                        id: filterchoice
                                        textRole: 'text'
                                        valueRole: 'value'

                                        anchors {
                                            left: parent.left
                                            leftMargin: 10
                                        }
                                        
                                        indicator: MaterialDesignIcon {
                                            size: 20
                                            name: 'menu-down'
                                            color: Material.foreground

                                            anchors {
                                                verticalCenter: parent.verticalCenter
                                                right: parent.right
                                                rightMargin: 5
                                            }
                                        }

                                        model: [ 
                                            { value: StatesAndActions.MIDIMessageFilterType.IsNoteKind, text: 'Note' },
                                            { value: StatesAndActions.MIDIMessageFilterType.IsNoteOn, text: 'Note On' },
                                            { value: StatesAndActions.MIDIMessageFilterType.IsNoteOff, text: 'Note Off' },
                                            { value: StatesAndActions.MIDIMessageFilterType.IsNoteShortPress, text: 'Note Short' },
                                            { value: StatesAndActions.MIDIMessageFilterType.IsNoteDoublePress, text: 'Note Double' },
                                            { value: StatesAndActions.MIDIMessageFilterType.IsCCKind, text: 'CC' }
                                        ]

                                        property int controlledState: midi_settings.active_profile_entry.profile.input_rules[index]['filter']
                                        function update() {
                                            currentIndex = model.findIndex((m) => { return m.value == controlledState })
                                        }

                                        onControlledStateChanged: update()
                                        Component.onCompleted: update()

                                        onActivated: (activated_idx) => {
                                            popup.close()
                                            midi_settings.active_profile_entry.profile.input_rules[index]['filter'] = model[activated_idx].value
                                            midi_settings.active_profile_entry.profile.input_rulesChanged()
                                        }
                                    }
                                    TextField {
                                        id: condition_field
                                        anchors.leftMargin: 10
                                        anchors.left: filterchoice.right
                                        anchors.verticalCenter: filterchoice.verticalCenter
                                        width: 200
                                        text: modelData['condition']
                                    }
                                    TextField {
                                        anchors.left: condition_field.right
                                        anchors.leftMargin: 10
                                        anchors.right: deleterulebutton.left
                                        anchors.rightMargin: 10
                                        anchors.verticalCenter: filterchoice.verticalCenter
                                        text: modelData['action']
                                    }
                                    Button {
                                        id: deleterulebutton
                                        anchors {
                                            verticalCenter: filterchoice.verticalCenter
                                            right: parent.right
                                            rightMargin: 10
                                        }
                                        width: height
                                        height: 40

                                        MaterialDesignIcon {
                                            size: parent.width - 20
                                            name: 'delete'
                                            color: Material.foreground

                                            anchors {
                                                verticalCenter: parent.verticalCenter
                                                horizontalCenter: parent.horizontalCenter
                                            }
                                        }

                                        onClicked: {
                                            midi_settings.active_profile_entry.profile.input_rules.splice(index, 1)
                                            midi_settings.active_profile_entry.profile.input_rulesChanged()
                                        }
                                    }
                                }
                            }

                            Button {
                                text: 'Add'
                                onClicked: {
                                    midi_settings.active_profile_entry.profile.input_rules.push({
                                        'filter': 0,
                                        'condition': '',
                                        'action': ''
                                    })
                                    midi_settings.active_profile_entry.profile.input_rulesChanged()
                                }
                            }
                        }
                    }

                    GroupBox {
                        title: 'Loop state change event handling'
                        width: parent.width

                        Column {
                            spacing: 3
                            id: state_change_column
                            width: parent.width

                            Repeater {
                                model: midi_settings.active_profile_entry.profile.loop_state_change_formulas
                                
                                Rectangle {
                                    radius: 3
                                    
                                    color: Material.background
                                    border.color: 'grey'
                                    border.width: 1

                                    width: state_change_column.width
                                    height: 52

                                    ComboBox {
                                        id: statechoice
                                        textRole: 'text'
                                        valueRole: 'value'

                                        anchors {
                                            left: parent.left
                                            leftMargin: 10
                                        }
                                        
                                        indicator: MaterialDesignIcon {
                                            size: 20
                                            name: 'menu-down'
                                            color: Material.foreground

                                            anchors {
                                                verticalCenter: parent.verticalCenter
                                                right: parent.right
                                                rightMargin: 5
                                            }
                                        }

                                        model: {
                                            var LS = StatesAndActions.LoopState
                                            var names = StatesAndActions.LoopState_names
                                            return Object.entries(LS).map((entry) => {
                                                return { value: entry[1], text: names[entry[1]] }
                                            });
                                        }

                                        property int controlledState: midi_settings.active_profile_entry.profile.loop_state_change_formulas[index]['state']
                                        function update() {
                                            currentIndex = model.findIndex((m) => { return m.value == controlledState })
                                        }

                                        onControlledStateChanged: update()
                                        Component.onCompleted: update()

                                        popup.width: 200

                                        onActivated: (activated_idx) => {
                                            popup.close()
                                            midi_settings.active_profile_entry.profile.loop_state_change_formulas[index]['state'] = model[activated_idx].value
                                            midi_settings.active_profile_entry.profile.loop_state_change_formulasChanged()
                                        }
                                    }
                                    TextField {
                                        anchors.left: statechoice.right
                                        anchors.leftMargin: 10
                                        anchors.right: deletestateformula.left
                                        anchors.rightMargin: 10
                                        anchors.verticalCenter: statechoice.verticalCenter
                                        text: modelData['action']
                                    }
                                    Button {
                                        id: deletestateformula
                                        anchors {
                                            verticalCenter: statechoice.verticalCenter
                                            right: parent.right
                                            rightMargin: 10
                                        }
                                        width: height
                                        height: 40

                                        MaterialDesignIcon {
                                            size: parent.width - 20
                                            name: 'delete'
                                            color: Material.foreground

                                            anchors {
                                                verticalCenter: parent.verticalCenter
                                                horizontalCenter: parent.horizontalCenter
                                            }
                                        }

                                        onClicked: {
                                            midi_settings.active_profile_entry.profile.loop_state_change_formulas.splice(index, 1)
                                            midi_settings.active_profile_entry.profile.loop_state_change_formulasChanged()
                                        }
                                    }
                                }
                            }

                            Button {
                                text: 'Add'
                                onClicked: {
                                    midi_settings.active_profile_entry.profile.loop_state_change_formulas.push({
                                        'state': StatesAndActions.LoopState.Stopped,
                                        'action': ''
                                    })
                                    midi_settings.active_profile_entry.profile.loop_state_change_formulasChanged()
                                }
                            }

                            Rectangle {
                                radius: 3
                                    
                                color: Material.background
                                border.color: 'grey'
                                border.width: 1

                                width: state_change_column.width
                                height: 52

                                Label {
                                    id: defaultstatechangelabel
                                    anchors {
                                        left: parent.left
                                        leftMargin: 10
                                        verticalCenter: parent.verticalCenter
                                    }
                                    text: 'Formula for other states:'
                                }
                                TextField {
                                    anchors.left: defaultstatechangelabel.right
                                    anchors.leftMargin: 10
                                    anchors.right: parent.right
                                    anchors.rightMargin: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: midi_settings.active_profile_entry.profile.default_loop_state_change_formula
                                }
                            }
                        }
                    }

                    GroupBox {
                        title: 'Other event handling'
                        width: parent.width

                        Column {
                            Rectangle {
                                radius: 3
                                    
                                color: Material.background
                                border.color: 'grey'
                                border.width: 1

                                width: state_change_column.width
                                height: 52

                                Label {
                                    id: resetlabel
                                    anchors {
                                        left: parent.left
                                        leftMargin: 10
                                        verticalCenter: parent.verticalCenter
                                    }
                                    text: 'Formula at (re-)connect:'
                                }
                                TextField {
                                    anchors.left: resetlabel.right
                                    anchors.leftMargin: 10
                                    anchors.right: parent.right
                                    anchors.rightMargin: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: midi_settings.active_profile_entry.profile.reset_formula
                                }
                            }
                        }
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
                {
                    'filter': StatesAndActions.MIDIMessageFilterType.IsNoteOn,
                    'condition': 'note <= 64',
                    'action': 'loopAction(note_track, note_loop, play_or_stop)'
                },
                {
                    'filter': StatesAndActions.MIDIMessageFilterType.IsCCKind,
                    'condition': '48 <= controller < 56',
                    'action': 'setVolume(fader_track, value/127.0)'
                }
            ]

            loop_state_change_formulas: [
                { 'state': StatesAndActions.LoopState.Recording, 'action': 'noteOn(0, loop_note, 3)' },
                { 'state': StatesAndActions.LoopState.Playing, 'action': 'noteOn(0, loop_note, 1)' },
                { 'state': StatesAndActions.LoopState.Stopped, 'action': 'noteOn(0, loop_note, 0)' },
                { 'state': StatesAndActions.LoopState.PlayingMuted, 'action': 'noteOn(0, loop_note, 0)' }
            ]

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
