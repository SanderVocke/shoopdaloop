import QtQuick 6.3

Item {
    property alias input_name_hint: input.name_hint
    property alias output_name_hint: output.name_hint
    readonly property var input_port: input
    readonly property var output_port: output
    readonly property var ports: [input, output]
    
    MidiPort {
        id: input
        direction: 'in'
    }
    MidiPort {
        id: output
        direction: 'out'
    }
}