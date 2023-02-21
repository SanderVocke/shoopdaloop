import QtQuick 2.15

// Instantiates loop channels to match a given set of ports and port pairs.
Item {
    id: router
    property list<var> dry_port_pairs   // List of ***PortPair
    property list<var> wet_port_pairs   // List of ***PortPair
    property list<var> direct_port_pairs // List of ***PortPair
    property var maybe_mixed_wet_audio_output_port  // AudioPort
    property var loop
    onLoopChanged: () => console.log(loop)

    readonly property var ports_per_audio_channel: {
        var r = []
        for (var i=0; i<dry_port_pairs.length; i++) {
            var p = dry_port_pairs[i]
            if (p instanceof AudioPortPair) {
                r.push(p.ports)
            }
        }
        for (var i=0; i<wet_port_pairs.length; i++) {
            var p = wet_port_pairs[i]
            if (p instanceof AudioPortPair) {
                var pp = p.ports
                if (maybe_mixed_wet_audio_output_port) { pp.push(maybe_mixed_wet_audio_output_port) }
                r.push(pp)
            }
        }
        for (var i=0; i<direct_port_pairs.length; i++) {
            var p = direct_port_pairs[i]
            if (p instanceof AudioPortPair) {
                var pp = p.ports
                if (maybe_mixed_wet_audio_output_port) { pp.push(maybe_mixed_wet_audio_output_port) }
                r.push(pp)
            }
        }
        console.log('ports per audio', r)
        return r
    }

    readonly property var ports_per_midi_channel: {
        var r = []
        for (var i=0; i<dry_port_pairs.length; i++) {
            var p = dry_port_pairs[i]
            if (p instanceof MidiPortPair) {
                r.push(p.ports)
            }
        }
        for (var i=0; i<wet_port_pairs.length; i++) {
            var p = wet_port_pairs[i]
            if (p instanceof MidiPortPair) {
                r.push(p.ports)
            }
        }
        for (var i=0; i<direct_port_pairs.length; i++) {
            var p = direct_port_pairs[i]
            if (p instanceof MidiPortPair) {
                r.push(p.ports)
            }
        }
        console.log('ports per midi', r)
        return r
    }

    Repeater {
        model: ports_per_audio_channel.length
        LoopAudioChannel {
            ports : ports_per_audio_channel[index]
            loop: router.loop
        }
    }
    Repeater {
        model: ports_per_midi_channel.length
        LoopMidiChannel {
            ports : ports_per_midi_channel[index]
            loop : router.loop
        }
    }
}