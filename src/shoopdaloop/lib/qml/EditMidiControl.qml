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
            var dialog = midi_filters_dialog_factory.createObject(root, {
                'filters': [MidiControl.match_type(Midi.NoteOn)],
            })

            dialog.open()
            dialog.accepted.connect(function() {
                configuration.contents.push({
                    'filters': dialog.filters,
                    'action': 'Stop All',
                })
                configuration.contentsChanged()
                dialog.close()
                dialog.destroy()
            })
        }
    }

    component EditMidiControlItem: GroupBox {
        property var item
        id: box

        property var rule_descriptor: MidiControl.parse_midi_filters(item.filters)

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

    Component {
        id: midi_filters_dialog_factory
        Dialog {
            modal: true
            title: 'Specify MIDI filter'
            standardButtons: Dialog.Reset | Dialog.Ok | Dialog.Cancel

            width: Math.min(Overlay.overlay ? Overlay.overlay.width - 50 : 700, 700)
            height: Math.min(Overlay.overlay ? Overlay.overlay.height - 50 : 450, 450)
            anchors.centerIn: Overlay.overlay ? Overlay.overlay : parent

            property alias filters: edit.filters

            EditMidiMessageFilter {
                id: edit
            }
        }
    }
}