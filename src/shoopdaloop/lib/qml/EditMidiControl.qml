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

            onUpdateFilters: (filters) => {
                configuration.contents[index].filters = filters
                configuration.contentsChanged()
            }
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
                var new_contents = configuration.contents.concat([{
                    'filters': dialog.filters,
                    'action': 'Stop Loops',
                }])
                root.logger.info(() => (JSON.stringify(new_contents)))
                configuration.contents = new_contents
                dialog.close()
                dialog.destroy()
            })
        }
    }

    component EditMidiControlItem: GroupBox {
        property var item
        id: box

        property var rule_descriptor: MidiControl.parse_midi_filters(item.filters)
        property var builtin_action: (MidiControl.builtin_actions[item.action] || {
            'name': 'Custom...',
            'description': 'Custom action',
            'script': 'print_debug("hello")'
        })
        property var action_inputs: builtin_action.inputs || {}

        signal updateFilters(list<var> filters)

        Column {
            Row {
                spacing: 3

                Label {
                    text: 'On:'
                    anchors.verticalCenter: action_combo.verticalCenter
                }

                TextField {
                    enabled: false
                    text: rule_descriptor.description
                    anchors.verticalCenter: action_combo.verticalCenter
                    height: action_combo.height
                    width: 250
                }

                ExtendedButton {
                    height: action_combo.height
                    width: height - 10
                    tooltip: "Edit MIDI filter"

                    MaterialDesignIcon {
                        size: 16
                        name: 'pencil'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }
                    onClicked: {
                        var dialog = midi_filters_dialog_factory.createObject(root, {
                            'filters': item.filters,
                        })

                        dialog.open()
                        dialog.accepted.connect(function() {
                            box.updateFilters(dialog.filters)
                            configuration.contentsChanged()
                            dialog.close()
                            dialog.destroy()
                        })
                    }
                }

                Label {
                    text: ':'
                    anchors.verticalCenter: action_combo.verticalCenter
                }

                ComboBox {
                    id: action_combo
                    width: 200
                    height: 40
                    popup.width: 300
                    model: Object.keys(MidiControl.builtin_actions).concat(['Custom...'])
                    onActivated: (idx) => {
                        if (model[idx] === 'Custom...') {
                            box.item.action = ''
                            if (item.hasOwnProperty('inputs')) { delete item.inputs }
                        } else {
                            box.item.action = model[idx]
                            item.inputs = {}
                            for (var input_name in MidiControl.builtin_actions[model[idx]].inputs) {
                                item.inputs[input_name] = MidiControl.builtin_actions[model[idx]].inputs[input_name].default
                            }
                        }
                        box.itemChanged()
                    }
                    currentIndex: {
                        let idx = action_combo.model.indexOf(box.item.action)
                        currentIndex = (idx < 0) ? model.length - 1 : idx
                    }
                }

                Label {
                    text: 'with:'
                    anchors.verticalCenter: action_combo.verticalCenter
                    visible: Object.keys(action_inputs).length > 0
                }
            }

            Row {
                spacing: 3
                id: custom_action_script_row
                visible: action_combo.currentValue === 'Custom...'

                Item { width: 200; height: 30 }

                Label {
                    text: '•  execute: '
                    anchors.verticalCenter: custom_action_script_field.verticalCenter
                }

                TextField {
                    id: custom_action_script_field
                    height: 40
                    placeholderText: 'custom action script'
                    text: box.item.action
                    width: 500
                    onAccepted: {
                        box.item.action = text
                        box.itemChanged()
                    }
                }
            }

            Repeater {
                id: actions_repeater

                model: Object.keys(action_inputs).length

                Row {
                    spacing: 3
                    id: action_row
                    property var input: Object.values(action_inputs)[index]
                    property string input_name: Object.keys(action_inputs)[index]
                    property var maybe_configured_value: (item.hasOwnProperty('inputs') && item.inputs.hasOwnProperty(input_name)) ?
                        item.inputs[input_name] : null

                    Item { width: 200; height: 30 }

                    Label {
                        text: '•  ' + input_name + ': '
                        anchors.verticalCenter: input_combo.verticalCenter
                    }

                    ComboBox {
                        id: input_combo
                        height: 40
                        width: 150
                        model: input.hasOwnProperty('presets') ? Object.keys(input.presets).concat(['custom...']) || [] : []
                        visible: input.hasOwnProperty('presets') && Object.keys(input.presets).length > 0
                        currentIndex: {
                            let idx = (maybe_configured_value !== null) ? model.indexOf(maybe_configured_value) : 0
                            if (idx === -1) { idx = model.length - 1 }
                            return idx
                        }
                        onActivated: (idx) => {
                            if (idx < (model.length - 1)) {
                                if (!box.item.hasOwnProperty('inputs')) { box.item.inputs = {} }
                                box.item["inputs"][action_row.input_name] = model[idx]
                                box.itemChanged()
                            }
                        }
                    }

                    TextField {
                        id: input_script_field
                        visible: (input_combo.currentText === 'custom...') ||
                                 (!input.hasOwnProperty('presets')) ||
                                    (Object.keys(input.presets).length === 0)
                        height: input_combo.height
                        anchors.verticalCenter: input_combo.verticalCenter
                        placeholderText: 'custom input script'
                        width: 400

                        text: maybe_configured_value || ''

                        onAccepted: {
                            if (!box.item.hasOwnProperty('inputs')) { box.item.inputs = {} }
                            box.item["inputs"][action_row.input_name] = text
                            box.itemChanged()
                        }
                    }
                }
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