import QtQuick 6.3

Item {
    readonly property var use_libs: [
        'shoop_control',
        'shoop_helpers',
        'shoop_coords'
    ]
    readonly property var actions: [
        { 'Stop All': 'shoop_control.loop_transition(shoop_control.loop_get_all(), shoop_control.constants.LoopMode_Stopped, 0)' }
    ]

    // MIDI message classification
    readonly property int NoteOn: 0x9F
    readonly property int NoteOff: 0x8F
    readonly property int ControlChange: 0xBF
    readonly property int ProgramChange: 0xCF
    readonly property int PitchBend: 0xEF
    readonly property int Aftertouch: 0xAF
    readonly property int PolyAftertouch: 0xDF
    readonly property int SysEx: 0xFF

    function msg_type(msg) {
        return msg[0] & 0xF0
    }

    // To specify what MIDI messages to react to, a message filter is defined as
    // a list of constants. The filter accepts messages that have an equal number
    // of bytes as the list of constants.
    // Each constant in the list is compared to the resp. byte:
    // - if the constant is an integer, they are ANDed. Nonzero result = accepted.
    // - if the constant is null, the byte is ignored.
    // - if the constant is undefined, the byte is considered an input to the
    //   corresponding message handler.
    // - if the constant is a list, each item in the list is tried. One accept
    //   means the entire byte is accepted. If the list contains an undefined,
    //   the byte becomes an input in addition to the filter.

    // The configuration is a list of filters with coupled actions.
    property var configuration : [
        [[NoteOn, null, null], 'Stop All']
    ]
}