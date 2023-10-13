import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

import '../midi.js' as Midi
import '../midi_control.js' as MidiControl

Column {
    id: root
    width: parent.width

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
                'filters': [MidiControl.match_type(Midi.NoteOn)],
                'action': 'Stop All',
            })
            configuration.contentsChanged()
        }
    }

    component EditMidiControlItem: GroupBox {
        property var item

        id: box

        property string rule_kind_description: {
            let filters = item.filters
            var filters_covered = 0
            let n_filters = item.filters.length
            var maybe_msgtype_filter = null
            var maybe_2ndbyte_filter = null
            var maybe_3rdbyte_filter = null

            for(var i=0; i<item.filters.length; i++) {
                let filter = item.filters[i]
                if (filter[0] == 0 && filter[1] == 0xF0 && filter[2] == Midi.NoteOn) {
                    maybe_msgtype_filter = 'note on'
                    filters_covered++
                }
            }

            let all_covered = filters_covered == n_filters
            if(all_covered && maybe_msgtype_filter == 'note on') {
                if (n_filters == 1) {
                    return 'Any note on'
                }
            }

            return 'Advanced rule'
        }

        Row {
            Label {
                text: 'On: ' + box.rule_kind_description
            }

            ComboBox {
                width: 150
                popup.width: 300
                model: Object.keys(MidiControl.builtin_actions).concat(['Custom...'])
            }
        }
    }
}