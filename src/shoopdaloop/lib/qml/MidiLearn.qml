import QtQuick 6.3

Item {
    id: root
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

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.MidiLearn" }

    function handle_midi(msg) {
        let try_filter_entry = (filter_entry, byte) => {
            if (filter_entry === null) { return true; }
            else if (filter_entry === undefined) { return true; }
            else if Array.isArray(filter_entry) {
                for(var i=0; i<filter_entry.length; i++) {
                    if (!arguments.callee(filter_entry[i], byte)) {
                        return false
                    }
                }
            }
            else {
                return (filter_entry & byte) != 0
            }
        }

        let try_filter = (filter, msg) => {
            if (filter.length != msg.length) { return false; }
            for(var i=0; i<filter.length; i++) {
                if (!try_filter_entry(filter[i], msg[i])) { return false; }
            }
            return true;
        }

        for(var i=0; i<configuration.length; i++) {
            const filter = configuration[i][0]
            const action = configuration[i][1]
            if (try_filter(filter, msg)) {
                root.logger.debug(`Matched MIDI learn filter: msg = ${msg}, filter = ${filter}, action = ${action}`)
            }
        }
    }
}