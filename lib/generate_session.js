function generate_audio_port_desc(id, name, direction, volume) {
    return {
        'id': id,
        'schema': 'audioport.1',
        'name': name,
        'direction': direction,
        'volume': volume
    };
}

function generate_midi_port_desc(id, name, direction) {
    return {
        'id': id,
        'schema': 'midiport.1',
        'name': name,
        'direction': direction,
    };
}

function generate_loop_channel(id, mode, type, data_length, connected_port_ids) {
    return {
        'id': id,
        'schema': 'channel.1',
        'mode': mode,
        'type': type,
        'data_length': data_length,
        'connected_port_ids': connected_port_ids
    };
}

function generate_loop(id, length, is_master, channels) {
    return {
        'id': id,
        'schema': 'loop.1',
        'length': length,
        'is_master': is_master,
        'channels': channels
    }
}

function generate_track(id, name, loops, ports) {
    return {
        'id': id,
        'schema': 'track.1',
        'name': name,
        'loops': loops,
        'ports': ports
    }
}

function generate_session(tracks, ports) {
    return {
        'schema': 'session.1',
        'tracks': tracks,
        'ports': ports
    }
}

function generate_default_session() {
    var master_loop_audio_l_ports = [
        generate_audio_port_desc('master_loop_audio_l_in', 'master_loop_audio_in_l', 'input', 1.0),
        generate_audio_port_desc('master_loop_audio_l_out', 'master_loop_audio_out_l', 'output', 1.0),
    ];
    var master_loop_audio_r_ports = [
        generate_audio_port_desc('master_loop_audio_r_in', 'master_loop_audio_in_r', 'input', 1.0),
        generate_audio_port_desc('master_loop_audio_r_out', 'master_loop_audio_out_r', 'output', 1.0),
    ];
    var master_loop_midi_ports = [
        generate_midi_port_desc('master_loop_midi_in', 'master_loop_in_midi', 'input'),
        generate_midi_port_desc('master_loop_midi_out', 'master_loop_out_midi', 'output'),
    ];
    var master_loop_audio_l_channel = generate_loop_channel('master_loop_audio_l', 'direct', 'audio', 0,
        master_loop_audio_l_ports.map(p => p.id));
    var master_loop_audio_r_channel = generate_loop_channel('master_loop_audio_r', 'direct', 'audio', 0,
        master_loop_audio_r_ports.map(p => p.id));
    var master_loop_midi_channel = generate_loop_channel('master_loop_midi', 'direct', 'midi', 0,
        master_loop_midi_ports.map(p => p.id));
    var master_loop = generate_loop('master_loop', 0, true, [
        master_loop_audio_l_channel, master_loop_audio_r_channel, master_loop_midi_channel
    ]);
    var master_track = generate_track('master_track', 'Master', [master_loop],
        [...master_loop_audio_l_ports, ...master_loop_audio_r_ports, ...master_loop_midi_ports]);
    


    var first_track_audio_l_ports = [
        generate_audio_port_desc('first_track_audio_l_in', 'first_track_audio_in_l', 'input', 1.0),
        generate_audio_port_desc('first_track_audio_l_out', 'first_track_audio_out_l', 'output', 1.0),
    ];
    var first_track_audio_r_ports = [
        generate_audio_port_desc('first_track_audio_r_in', 'first_track_audio_in_r', 'input', 1.0),
        generate_audio_port_desc('first_track_audio_r_out', 'first_track_audio_out_r', 'output', 1.0),
    ];
    var first_track_midi_ports = [
        generate_midi_port_desc('first_track_midi_in', 'first_track_in_midi', 'input'),
        generate_midi_port_desc('first_track_midi_out', 'first_track_out_midi', 'output'),
    ];
    var first_track_loops = []
    for (var i=0; i<8; i++) {
        var ii = i.toString();
        var audio_l_channel = generate_loop_channel('loop_1_'+ii+'_audio_l', 'direct', 'audio', 0,
            first_track_audio_l_ports.map(p => p.id));
        var audio_r_channel = generate_loop_channel('loop_1_'+ii+'_audio_r', 'direct', 'audio', 0,
            first_track_audio_r_ports.map(p => p.id));
        var midi_channel = generate_loop_channel('loop_1_'+ii+'_midi', 'direct', 'midi', 0,
            first_track_midi_ports.map(p => p.id));
        var loop = generate_loop('loop_1_'+ii, 0, false, [
            audio_l_channel, audio_r_channel, midi_channel
        ]);
        first_track_loops.push(loop);
    }
    var first_track = generate_track('track_1', 'Track 1', first_track_loops,
        [...first_track_audio_l_ports, ...first_track_audio_r_ports, ...first_track_midi_ports]);

    var session = generate_session([master_track, first_track], []);
    return session;    
}