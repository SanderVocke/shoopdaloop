import QtQuick 2.15

import '../../build/types.js' as Types

// Instantiates loop channels to match a given set of ports and port pairs.
Item {
    id: router
    property list<var> dry_port_pairs   // List of ***PortPair
    property list<var> wet_port_pairs   // List of ***PortPair
    property list<var> direct_port_pairs // List of ***PortPair
    property var maybe_mixed_wet_audio_output_port  // AudioPort
    property var loop
    onLoopChanged: () => console.log(loop)

    readonly property var audio_channel_descriptors: {
        var r = []
        for (var i=0; i<dry_port_pairs.length; i++) {
            var p = dry_port_pairs[i]
            if (p instanceof AudioPortPair) {
                r.push({
                    'mode': Types.ChannelMode.Dry,
                    'ports': p.ports
                })
            }
        }
        for (var i=0; i<wet_port_pairs.length; i++) {
            var p = wet_port_pairs[i]
            if (p instanceof AudioPortPair) {
                var pp = p.ports
                if (maybe_mixed_wet_audio_output_port) { pp.push(maybe_mixed_wet_audio_output_port) }
                r.push({
                    'mode': Types.ChannelMode.Wet,
                    'ports': pp
                })
            }
        }
        for (var i=0; i<direct_port_pairs.length; i++) {
            var p = direct_port_pairs[i]
            if (p instanceof AudioPortPair) {
                var pp = p.ports
                if (maybe_mixed_wet_audio_output_port) { pp.push(maybe_mixed_wet_audio_output_port) }
                r.push({
                    'mode': Types.ChannelMode.Direct,
                    'ports': pp
                })
            }
        }
        console.log('ports per audio', r)
        return r
    }

    readonly property var midi_channel_descriptors: {
        var r = []
        for (var i=0; i<dry_port_pairs.length; i++) {
            var p = dry_port_pairs[i]
            if (p instanceof MidiPortPair) {
                r.push({
                    'mode': Types.ChannelMode.Dry,
                    'ports': p.ports
                })
            }
        }
        for (var i=0; i<wet_port_pairs.length; i++) {
            var p = wet_port_pairs[i]
            if (p instanceof MidiPortPair) {
                r.push({
                    'mode': Types.ChannelMode.Wet,
                    'ports': p.ports
                })
            }
        }
        for (var i=0; i<direct_port_pairs.length; i++) {
            var p = direct_port_pairs[i]
            if (p instanceof MidiPortPair) {
                r.push({
                    'mode': Types.ChannelMode.Direct,
                    'ports': p.ports
                })
            }
        }
        console.log('ports per midi', r)
        return r
    }

    Repeater {
        model: audio_channel_descriptors.length
        LoopAudioChannel {
            ports : audio_channel_descriptors[index]['ports']
            initial_mode : { console.log(audio_channel_descriptors[index]['mode']); return audio_channel_descriptors[index]['mode'] }
            loop: router.loop
        }
    }
    Repeater {
        model: midi_channel_descriptors.length
        LoopMidiChannel {
            ports : midi_channel_descriptors[index]['ports']
            initial_mode : midi_channel_descriptors[index]['mode']
            loop : router.loop
        }
    }
}