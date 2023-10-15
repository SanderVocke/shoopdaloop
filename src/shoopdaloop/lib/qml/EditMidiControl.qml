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

    enum RecognizedRuleKind {
        AnyNoteOn,
        AnyNoteOff,
        SpecificNoteOn,
        SpecificNoteOff,
        AnyControlChange,
        SpecificControlChange,
        Advanced
    }

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

        property var maybe_msgtype_filters: {
            let filters = item.filters
            var result = []
            for(var i=0; i<item.filters.length; i++) {
                let filter = item.filters[i]
                if (filter[0] == 0 && filter[1] == 0xF0) {
                    result.push(filter[2])
                }
            }
            return result
        }
        property var maybe_2ndbyte_filters: {
            let filters = item.filters
            var result = []
            for(var i=0; i<item.filters.length; i++) {
                let filter = item.filters[i]
                if (filter[0] == 1 && filter[1] == 0xFF) {
                    result.push(filter[2])
                }
            }
            return result
        }
        property var maybe_3rdbyte_filters: {
            let filters = item.filters
            var result = []
            for(var i=0; i<item.filters.length; i++) {
                let filter = item.filters[i]
                if (filter[0] == 2 && filter[1] == 0xFF) {
                    result.push(filter[2])
                }
            }
            return result
        }

        property int rule_kind: {
            let filters = item.filters
            var filters_covered = 0
            let n_filters = item.filters.length
            var rval = undefined

            if (maybe_msgtype_filters.length == 1 && maybe_msgtype_filters[0] == Midi.NoteOn) {
                if (maybe_2ndbyte_filters.length == 0 && maybe_3rdbyte_filters.length == 0) {
                    rval = EditMidiControl.RecognizedRuleKind.AnyNoteOn
                } else if (maybe_2ndbyte_filters.length == 1 && maybe_3rdbyte_filters.length <= 1) {
                    rval = EditMidiControl.RecognizedRuleKind.SpecificNoteOn
                }
            }

            if (rval === undefined) {
                rval = EditMidiControl.RecognizedRuleKind.Advanced
            }

            root.logger.debug(`Rule kind for filters ${filters}: ${rval}`)
            return rval
        }
        property string rule_kind_description: {
            let filters = item.filters
            switch(rule_kind) {
                case EditMidiControl.RecognizedRuleKind.AnyNoteOn:
                    return 'Any Note On'
                case EditMidiControl.RecognizedRuleKind.SpecificNoteOn:
                    return `Note ${maybe_2ndbyte_filters[0]} On` +
                           (maybe_3rdbyte_filters.length == 1 ?
                           ` on ch. ${maybe_3rdbyte_filters[0]}` : '')
                case EditMidiControl.RecognizedRuleKind.Advanced:
                    return 'Advanced rule'
            }
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