import QtQuick 2.15

Item {
    property alias input_name_hint: input.name_hint
    property alias output_name_hint: output.name_hint
    readonly property var input_port: input
    readonly property var output_port: output
    readonly property var ports: [input, output]
    
    AudioPort {
        id: input
        direction: 'in'
    }
    AudioPort {
        id: output
        direction: 'out'
    }
}