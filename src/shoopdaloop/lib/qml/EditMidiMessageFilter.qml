import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtQuick.Layouts
import ShoopDaLoop.PythonLogger

import '../midi.js' as Midi
import '../midi_control.js' as MidiControl
import 'test/testDeepEqual.js' as TestDeepEqual

Column {
    id: root
    width: parent.width

    property list<var> filters: create_default_filters(MidiControl.UiMessageFilterKind.NoteOn)
    property var filters_descriptor: MidiControl.parse_midi_filters(filters)

    property PythonLogger logger: PythonLogger { name: 'Frontend.Qml.EditMidiMessageFilter'}

    function create_default_filters(kind) {
        if (kind == MidiControl.UiMessageFilterKind.NoteOn) {
            return [ MidiControl.match_type(Midi.NoteOn) ]
        } else if (kind == MidiControl.UiMessageFilterKind.NoteOff) {
            return [ MidiControl.match_type(Midi.NoteOff) ]
        } else if (kind == MidiControl.UiMessageFilterKind.ControlChange) {
            return [ MidiControl.match_type(Midi.ControlChange) ]
        } else {
            return []
        }
    }

    TabBar {
        id: bar
        width: parent.width

        TabButton { text: 'Regular' }
        TabButton { text: 'Advanced' }
    }

    StackLayout {
        width: parent.width
        anchors.top: bar.bottom
        anchors.topMargin: 5
        currentIndex: bar.currentIndex
        Item {
            id: regular

            Row {
                spacing: 5

                Label {
                    text: 'On:'
                    anchors.verticalCenter: kind_combo.verticalCenter
                }
                ComboBox {
                    id: kind_combo
                    model: Object.values(MidiControl.UiMessageFilterKind)
                    currentIndex: Object.values(MidiControl.UiMessageFilterKind).indexOf(MidiControl.KindToUiKind[root.filters_descriptor.kind])
                    onActivated: (idx) => {
                        let new_val = Object.values(MidiControl.UiMessageFilterKind)[idx]
                        if (MidiControl.KindToUiKind[MidiControl.parse_midi_filters(filters).kind] != new_val) {
                            root.filters = root.create_default_filters(new_val)
                            root.logger.error(`filters => ${JSON.stringify(root.filters)}, index => ${currentIndex}, kind => ${new_val}, idx => ${idx}`)
                        }
                    }
                }

                Loader {
                    active: root.filters_descriptor.kind === MidiControl.MessageFilterKind.AnyNoteOn
                    anchors.verticalCenter: kind_combo.verticalCenter
                    sourceComponent: Component {
                        Row {
                            spacing: 5
                            Label {
                                text: ', note'
                                anchors.verticalCenter: note_combo.verticalCenter
                            }
                            ComboBox {
                                model: ['Any', 'Choose...']
                                id: note_combo
                                currentIndex: root.filters_descriptor.kind === MidiControl.MessageFilterKind.AnyNoteOn ? 0 : 1
                                onActivated: (idx) => {
                                    if (idx > 0) {
                                        root.filters.push([1, 0xFF, 0])
                                        root.filtersChanged()
                                    }
                                }
                            }
                        }
                    }
                }

                Loader {
                    active: root.filters_descriptor.kind === MidiControl.MessageFilterKind.SpecificNoteOn
                    anchors.verticalCenter: kind_combo.verticalCenter
                    sourceComponent: Component {
                        Row {
                            spacing: 5
                            Label {
                                text: ', note'
                                anchors.verticalCenter: note_spinbox.verticalCenter
                            }
                            SpinBox {
                                from: 0
                                to: 127
                                id: note_spinbox
                                value: root.filters_descriptor.note || 0
                                onValueModified: {
                                    root.filters
                                        .filter(f => (f && f[0] == 1 && f[1] == 0xFF))
                                        .forEach(f => { f[2] = value })
                                    root.filtersChanged()
                                }
                            }
                        }
                    }
                }
            }
        }
        Item {
            id: advanced

            Column {
                spacing: 3

                Repeater {
                    model: root.filters.length

                    Column {
                        EditAdvancedFilter {
                            id: edit_advanced
                            Component.onCompleted: { filter = root.filters[index] }
                            Connections {
                                target: root
                                function onFiltersChanged() {
                                    if(root.filters[index] && !TestDeepEqual.testDeepEqual(root.filters[index], edit_advanced.filter, (msg) => {})) {
                                        edit_advanced.filter = root.filters[index]
                                    }
                                }
                            }
                            onDeleteRequested: {
                                root.filters.splice(index, 1)
                            }
                            onFilterUpdated: (filter) => {
                                root.filters[index] = filter
                                root.filtersChanged()
                            }
                        }
                    }
                }

                ExtendedButton {
                    tooltip: "Add a byte filter"
                    width: 30
                    height: 40
                    MaterialDesignIcon {
                        size: 20
                        name: 'plus'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: {
                        root.filters.push([ 0, 0xFF, 0xFF ])
                        root.filtersChanged()
                    }
                }
            }
        }
    }

    component EditAdvancedFilter: Row {
        id: edit_filter
        spacing: 5
        property list<int> filter: [ 0, 0xF0, Midi.NoteOn ]

        function update_filter(new_filter) {
            filter = new_filter
            filterUpdated(new_filter)
        }

        signal deleteRequested()
        signal filterUpdated(list<int> filter)

        Label {
            text: 'Byte'
            anchors.verticalCenter: vert.verticalCenter
            height: vert.height
            verticalAlignment: Text.AlignVCenter
        }

        SpinBox {
            anchors.verticalCenter: vert.verticalCenter
            height: vert.height

            value: filter.length > 0 ? filter[0] : 0
            onValueModified: {
                if (filter.length > 0) {
                    update_filter([ value, filter[1], filter[2] ])
                }
            }

            textFromValue: (value, locale) => {
                if (value == 0) { return '0 (Type + Channel)' }
                if (value == 1) { return '1 (Note / CC)' }
                if (value == 2) { return '2 (Velocity / Value)' }
                return value.toString()
            }
        }

        Label {
            anchors.verticalCenter: vert.verticalCenter
            height: vert.height
            verticalAlignment: Text.AlignVCenter
            text: 'with mask'
        }

        TextField {
            id: vert
            height: 30
            width: 80
            text: '0x' + (filter.length > 1 ? filter[1].toString(16).toUpperCase() : 'FF')
            onAccepted: {
                if (filter.length > 1) {
                    update_filter([ filter[0], parseInt(text), filter[2] ])
                }
            }
        }

        Label {
            anchors.verticalCenter: vert.verticalCenter
            height: vert.height
            verticalAlignment: Text.AlignVCenter
            text: 'equals'
        }

        TextField {
            anchors.verticalCenter: vert.verticalCenter
            height: vert.height
            width: 80
            text: '0x' + (filter.length > 2 ? filter[2].toString(16).toUpperCase() : 'FF')
            onAccepted: {
                if (filter.length > 2) {
                    update_filter([ filter[0], filter[1], parseInt(text) ])
                }
            }
        }

        ExtendedButton {
            tooltip: "Delete this byte filter"
            anchors.verticalCenter: vert.verticalCenter
            width: 30
            height: 40
            MaterialDesignIcon {
                size: 20
                name: 'delete'
                color: Material.foreground
                anchors.centerIn: parent
            }
            onClicked: edit_filter.deleteRequested()
        }
    }
}