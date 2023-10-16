import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonLogger

import '../midi.js' as Midi
import '../midi_control.js' as MidiControl

Column {
    id: root
    width: parent.width

    property list<var> filters: []

    TabBar {
        id: bar
        width: parent.width

        TabButton { text: 'Regular' }
        TabButton { text: 'Advanced' }
    }

    StackLayout {
        width: parent.width
        currentIndex: bar.currentIndex
        Item {
            id: regular
        }
        Item {
            id: advanced

            Column {
                Repeater {
                    model: root.filters.length

                    EditAdvancedFilter {
                        filter: root.filters[index]
                    }
                }
            }
        }
    }

    component EditAdvancedFilter: Row {
        property list<int> filter: [ 0, 0xF0, Midi.NoteOn ]

        Label {
            text: 'Byte'
        }

        SpinBox {
            value: filter.length > 0 ? filter[0] : 0
            onValueModified: {
                if (filter.length > 0) {
                    filter[0] = value
                }
            }
        }

        Label {
            text: 'with mask'
        }

        TextField {
            text: '0x' + filter.length > 1 ? filter[1].toString(16).toUpperCase() : 'FF'
            onTextChanged: {
                if (filter.length > 1) {
                    filter[1] = parseInt(text)
                }
            }
        }

        Label {
            text: 'equals'
        }

        TextField {
            text: '0x' + filter.length > 2 ? filter[2].toString(16).toUpperCase() : 'FF'
            onTextChanged: {
                if (filter.length > 2) {
                    filter[2] = parseInt(text)
                }
            }
        }
    }
}