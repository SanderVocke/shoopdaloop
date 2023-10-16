import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonLogger

import '../midi.js' as Midi
import '../midi_control.js' as MidiControl

Column {
    id: root
    width: parent.width

    property PythonLogger logger: PythonLogger { name: "Frontend.Qml.EditMidiControl" }

    property MidiControlConfiguration configuration : MidiControlConfiguration {}

    Repeater {
        model: configuration.contents.length
        delegate: EditMidiControlItem {
            width: root.width
            item: configuration.contents[index]
        }
    }

    ExtendedButton {
        tooltip: "Add a MIDI trigger."
        width: 30
        height: 40
        MaterialDesignIcon {
            size: 20
            name: 'plus'
            color: Material.foreground
            anchors.centerIn: parent
        }
        onClicked: {
            configuration.contents.push({
                'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(45)],
                'action': 'Stop All',
            })
            configuration.contentsChanged()
        }
    }

    component EditMidiControlItem: GroupBox {
        property var item
        id: box

        property var rule_descriptor: Midi.parse_midi_filters(item.filters)

        Row {
            Label {
                text: 'On: ' + rule_descriptor.description || 'Unknown'
            }

            ComboBox {
                width: 150
                popup.width: 300
                model: Object.keys(MidiControl.builtin_actions).concat(['Custom...'])
            }
        }
    }
}