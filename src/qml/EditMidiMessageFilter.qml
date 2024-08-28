import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Layouts
import ShoopDaLoop.PythonLogger

import '../midi.js' as Midi
import '../midi_control.js' as MidiControl
import 'test/testDeepEqual.js' as TestDeepEqual

Column {
    id: root
    width: parent.width

    property var filters: create_default_filters(MidiControl.UiMessageFilterKind.NoteOn)
    property var filters_descriptor: MidiControl.parse_midi_filters(filters)

    property PythonLogger logger: PythonLogger { name: 'Frontend.Qml.EditMidiMessageFilter'}

    property var maybe_suggested_filters: null
    property var maybe_suggested_filters_description: maybe_suggested_filters !== null ?
        MidiControl.parse_midi_filters(maybe_suggested_filters) : null

    // Register for MIDI events to have MIDI learn
    RegistryLookup {
        id: lookup_midi_control_port
        registry: registries.state_registry
        key: 'midi_control_port'
    }
    Connections {
        target: lookup_midi_control_port.object
        function onMsgReceived(msg) { root.midi_msg_received(msg) }
    }

    function midi_msg_received(msg) {
        if (Midi.maybe_note(msg) !== undefined) {
            root.maybe_suggested_filters = [ MidiControl.match_type(Midi.NoteOn),
                                             MidiControl.match_note(Midi.maybe_note(msg)),
                                             MidiControl.match_channel(Midi.channel(msg)) ]
        } else if (Midi.maybe_cc(msg) !== undefined) {
            root.maybe_suggested_filters = [ MidiControl.match_type(Midi.ControlChange),
                                             MidiControl.match_cc(Midi.maybe_cc(msg)),
                                             MidiControl.match_channel(Midi.channel(msg)) ]
        } else if (Midi.maybe_program(msg) !== undefined) {
            root.maybe_suggested_filters = [ MidiControl.match_type(Midi.ProgramChange),
                                             MidiControl.match_program(Midi.maybe_program(msg)),
                                             MidiControl.match_channel(Midi.channel(msg)) ]
        } else {
            root.maybe_suggested_filters = null
        }
        root.logger.debug(() => (`Received: [${msg}]. Suggestion: [${root.maybe_suggested_filters}]`))
    }

    function create_default_filters(kind) {
        if (kind == MidiControl.MessageFilterKind.NoteOn) {
            return [ MidiControl.match_type(Midi.NoteOn) ]
        } else if (kind == MidiControl.MessageFilterKind.NoteOff) {
            return [ MidiControl.match_type(Midi.NoteOff) ]
        } else if (kind == MidiControl.MessageFilterKind.ControlChange) {
            return [ MidiControl.match_type(Midi.ControlChange) ]
        } else if (kind == MidiControl.MessageFilterKind.ProgramChange) {
            return [ MidiControl.match_type(Midi.ProgramChange) ]
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

            Column {
                spacing: 5

                Row {
                    spacing: 5

                    Label {
                        text: 'On:'
                        anchors.verticalCenter: kind_combo.verticalCenter
                    }
                    ComboBox {
                        id: kind_combo
                        model: Object.values(MidiControl.MessageFilterKind)
                        currentIndex: Object.values(MidiControl.MessageFilterKind).indexOf(root.filters_descriptor.kind)
                        onActivated: (idx) => {
                            let new_val = Object.values(MidiControl.MessageFilterKind)[idx]
                            if (MidiControl.parse_midi_filters(filters).kind != new_val) {
                                root.filters = root.create_default_filters(new_val)
                                if (new_val == MidiControl.MessageFilterKind.Advanced) {
                                    bar.currentIndex = 1
                                }
                            }
                        }
                    }

                    Label {
                        text: ', note'
                        visible: MidiControl.supports_note(root.filters_descriptor)
                        anchors.verticalCenter: kind_combo.verticalCenter
                    }

                    ComboBox {
                        model: ['Any', 'Choose...']
                        visible: MidiControl.supports_note(root.filters_descriptor)
                                && !MidiControl.has_note(root.filters_descriptor)
                        anchors.verticalCenter: kind_combo.verticalCenter
                        currentIndex: MidiControl.has_note(root.filters_descriptor) ? 1 : 0
                        onActivated: (idx) => {
                            if (idx > 0) {
                                root.filters.push([1, 0xFF, 0])
                                root.filtersChanged()
                            }
                        }
                    }

                    SpinBox {
                        anchors.verticalCenter: kind_combo.verticalCenter
                        from: 0
                        to: 127
                        editable: true
                        value: root.filters_descriptor.note || 0
                        visible: MidiControl.supports_note(root.filters_descriptor)
                                && MidiControl.has_note(root.filters_descriptor)
                        onValueModified: {
                            root.filters = root.filters
                                .map(f => {
                                    if ((f && f[0] == 1 && MidiControl.is_identity_mask(f[1]))) {
                                        f[2] = value; return f;
                                    }
                                    return f;
                                })
                        }

                        ExtendedButton {
                            tooltip: "Remove note filter"
                            anchors.left: parent.right
                            anchors.top: parent.top
                            width: 16
                            height: 30
                            MaterialDesignIcon {
                                size: 14
                                name: 'delete'
                                color: Material.foreground
                                anchors.centerIn: parent
                            }
                            onClicked: {
                                root.filters = root.filters.filter(f => (f && !(f[0] == 1 && MidiControl.is_identity_mask(f[1]))))
                            }
                        }
                    }

                    Label {
                        text: ', CC'
                        visible: MidiControl.supports_cc(root.filters_descriptor)
                        anchors.verticalCenter: kind_combo.verticalCenter
                    }

                    ComboBox {
                        model: ['Any', 'Choose...']
                        visible: MidiControl.supports_cc(root.filters_descriptor)
                                && !MidiControl.has_cc(root.filters_descriptor)
                        anchors.verticalCenter: kind_combo.verticalCenter
                        currentIndex: MidiControl.has_cc(root.filters_descriptor) ? 1 : 0
                        onActivated: (idx) => {
                            if (idx > 0) {
                                root.filters.push([1, 0xFF, 0])
                                root.filtersChanged()
                            }
                        }
                    }

                    SpinBox {
                        anchors.verticalCenter: kind_combo.verticalCenter
                        from: 0
                        to: 127
                        editable: true
                        value: root.filters_descriptor.cc || 0
                        visible: MidiControl.supports_cc(root.filters_descriptor)
                                && MidiControl.has_cc(root.filters_descriptor)
                        onValueModified: {
                            root.filters = root.filters
                                .map(f => {
                                    if ((f && f[0] == 1 && MidiControl.is_identity_mask(f[1]))) {
                                        f[2] = value; return f;
                                    }
                                    return f;
                                })
                        }

                        ExtendedButton {
                            tooltip: "Remove CC filter"
                            anchors.left: parent.right
                            anchors.top: parent.top
                            width: 16
                            height: 30
                            MaterialDesignIcon {
                                size: 14
                                name: 'delete'
                                color: Material.foreground
                                anchors.centerIn: parent
                            }
                            onClicked: {
                                root.filters = root.filters.filter(f => (f && !(f[0] == 1 && MidiControl.is_identity_mask(f[1]))))
                            }
                        }
                    }

                    Label {
                        text: ', program'
                        visible: MidiControl.supports_program(root.filters_descriptor)
                        anchors.verticalCenter: kind_combo.verticalCenter
                    }

                    ComboBox {
                        model: ['Any', 'Choose...']
                        visible: MidiControl.supports_program(root.filters_descriptor)
                                && !MidiControl.has_program(root.filters_descriptor)
                        anchors.verticalCenter: kind_combo.verticalCenter
                        currentIndex: MidiControl.has_program(root.filters_descriptor) ? 1 : 0
                        onActivated: (idx) => {
                            if (idx > 0) {
                                root.filters.push([1, 0xFF, 0])
                                root.filtersChanged()
                            }
                        }
                    }

                    SpinBox {
                        anchors.verticalCenter: kind_combo.verticalCenter
                        from: 0
                        to: 127
                        editable: true
                        value: root.filters_descriptor.program || 0
                        visible: MidiControl.supports_program(root.filters_descriptor)
                                && MidiControl.has_program(root.filters_descriptor)
                        onValueModified: {
                            root.filters = root.filters
                                .map(f => {
                                    if ((f && f[0] == 1 && MidiControl.is_identity_mask(f[1]))) {
                                        f[2] = value; return f;
                                    }
                                    return f;
                                })
                        }

                        ExtendedButton {
                            tooltip: "Remove program filter"
                            anchors.left: parent.right
                            anchors.top: parent.top
                            width: 16
                            height: 30
                            MaterialDesignIcon {
                                size: 14
                                name: 'delete'
                                color: Material.foreground
                                anchors.centerIn: parent
                            }
                            onClicked: {
                                root.filters = root.filters.filter(f => (f && !(f[0] == 1 && MidiControl.is_identity_mask(f[1]))))
                            }
                        }
                    }

                    Label {
                        text: ', channel'
                        visible: MidiControl.supports_channel(root.filters_descriptor)
                        anchors.verticalCenter: kind_combo.verticalCenter
                    }

                    ComboBox {
                        model: ['Any', 'Choose...']
                        visible: MidiControl.supports_channel(root.filters_descriptor)
                                && !MidiControl.has_channel(root.filters_descriptor)
                        anchors.verticalCenter: kind_combo.verticalCenter
                        currentIndex: MidiControl.has_channel(root.filters_descriptor) ? 1 : 0
                        onActivated: (idx) => {
                            if (idx > 0) {
                                root.filters.push([0, 0x0F, 0])
                                root.filtersChanged()
                            }
                        }
                    }

                    SpinBox {
                        anchors.verticalCenter: kind_combo.verticalCenter
                        from: 0
                        to: 127
                        editable: true
                        value: root.filters_descriptor.channel || 0
                        visible: MidiControl.supports_channel(root.filters_descriptor)
                                && MidiControl.has_channel(root.filters_descriptor)
                        onValueModified: {
                            root.filters = root.filters
                                .map(f => {
                                    if ((f && f[0] == 0 && MidiControl.is_channel_mask(f[1]))) {
                                        f[2] = value; return f;
                                    }
                                    return f;
                                })
                        }

                        ExtendedButton {
                            tooltip: "Remove channel filter"
                            anchors.left: parent.right
                            anchors.top: parent.top
                            width: 16
                            height: 30
                            MaterialDesignIcon {
                                size: 14
                                name: 'delete'
                                color: Material.foreground
                                anchors.centerIn: parent
                            }
                            onClicked: {
                                root.filters = root.filters.filter(f => (f && !(f[0] == 0 && MidiControl.is_channel_mask(f[1]))))
                            }
                        }
                    }
                }

                Row {
                    visible: root.maybe_suggested_filters !== null
                    spacing: 5
                    Label {
                        anchors.verticalCenter: use_btn.verticalCenter
                        text: `Received: ${root.maybe_suggested_filters_description ? root.maybe_suggested_filters_description.description : ''}`
                    }
                    Button {
                        id: use_btn
                        text: 'Use'
                        onClicked: root.filters = root.maybe_suggested_filters
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
        property var filter: [ 0, 0xF0, Midi.NoteOn ]

        function update_filter(new_filter) {
            filter = new_filter
            filterUpdated(new_filter)
        }

        signal deleteRequested()
        signal filterUpdated(var filter)

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