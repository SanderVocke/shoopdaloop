function generate_audio_port_desc(id, name_parts, direction, volume) {
    return {
        'id': id,
        'schema': 'audioport.1',
        'name_parts': name_parts,
        'direction': direction,
        'volume': volume
    };
}

function generate_midi_port_desc(id, name_parts, direction) {
    return {
        'id': id,
        'schema': 'midiport.1',
        'name_parts': name_parts,
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

function generate_default_track(name, n_loops, id, first_loop_is_master) {
    var audio_l_ports = [
        generate_audio_port_desc(id+'_audio_l_in', [id, '_audio_in_l'], 'input', 1.0),
        generate_audio_port_desc(id+'_audio_l_out', [id, '_audio_out_l'], 'output', 1.0),
    ];
    var audio_r_ports = [
        generate_audio_port_desc(id+'_audio_r_in', [id, '_audio_in_r'], 'input', 1.0),
        generate_audio_port_desc(id+'_audio_r_out', [id, '_audio_out_r'], 'output', 1.0),
    ];
    var midi_ports = [
        generate_midi_port_desc(id+'_midi_in', [id, '_in_midi'], 'input'),
        generate_midi_port_desc(id+'_midi_out', [id, '_out_midi'], 'output'),
    ];
    var loops = []
    for (var i=0; i<n_loops; i++) {
        var ii = i.toString();
        var audio_l_channel = generate_loop_channel(id+'_loop_'+ii+'_audio_l', 'direct', 'audio', 0,
            audio_l_ports.map(p => p.id));
        var audio_r_channel = generate_loop_channel(id+'_loop_'+ii+'_audio_r', 'direct', 'audio', 0,
            audio_r_ports.map(p => p.id));
        var midi_channel = generate_loop_channel(id+'_loop_'+ii+'_midi', 'direct', 'midi', 0,
            midi_ports.map(p => p.id));
        var loop = generate_loop(id+'_loop_'+ii, 0, first_loop_is_master && i==0, [
            audio_l_channel, audio_r_channel, midi_channel
        ]);
        loops.push(loop);
    }
    var track = generate_track(id, name, loops,
        [...audio_l_ports, ...audio_r_ports, ...midi_ports]);
    return track;
}

function generate_default_session() {
    var master_track = generate_default_track("Master", 1, 'master_loop', true)
    var first_track = generate_default_track("Track 1", 8, 'track_1', false)
    
    var session = generate_session([master_track, first_track], []);
    return session;    
}