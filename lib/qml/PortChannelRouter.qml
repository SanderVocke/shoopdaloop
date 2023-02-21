import QtQuick 2.15

// Instantiates loop channels to match a given set of ports and port pairs.
Item {
    property list<var> dry_port_pairs   // List of ***PortPair
    property list<var> wet_port_pairs   // List of ***PortPair
    property list<var> direct_port_pairs // List of ***PortPair
    property var maybe_mixed_wet_audio_output_port  // AudioPort

    readonly property var ports_per_audio_channel: {
        var r = []
        for (var i=0; i<dry_port_pairs.length; i++) {
            var p = dry_port_pairs[i]
            if (p instanceof AudioPortPair) {
                r.push(p)
            }
        }
        for (var i=0; i<wet_port_pairs.length; i++) {
            var p = wet_port_pairs[i]
            if (p instanceof AudioPortPair) {
                if (maybe_mixed_wet_audio_output_port) { p.push(maybe_mixed_wet_audio_output_port) }
                r.push(p)
            }
        }
        for (var i=0; i<direct_port_pairs.length; i++) {
            var p = direct_port_pairs[i]
            if (p instanceof AudioPortPair) {
                if (maybe_mixed_wet_audio_output_port) { p.push(maybe_mixed_wet_audio_output_port) }
                r.push(p)
            }
        }
    }

    readonly property var ports_per_midi_channel: {
        var r = []
        for (var i=0; i<dry_port_pairs.length; i++) {
            var p = dry_port_pairs[i]
            if (p instanceof MidiPortPair) {
                r.push(p)
            }
        }
        for (var i=0; i<wet_port_pairs.length; i++) {
            var p = wet_port_pairs[i]
            if (p instanceof MidiPortPair) {
                r.push(p)
            }
        }
        for (var i=0; i<direct_port_pairs.length; i++) {
            var p = direct_port_pairs[i]
            if (p instanceof MidiPortPair) {
                r.push(p)
            }
        }
    }

    Repeater {
        model: ports_per_audio_channel.length
        LoopAudioChannel {
            ports : ports_per_audio_channel[index]
        }
    }
    Repeater {
        model: ports_per_midi_channel.length
        LoopMidiChannel {
            ports : ports_per_midi_channel[index]
        }
    }
}