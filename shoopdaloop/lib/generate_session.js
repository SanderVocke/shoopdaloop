function generate_audio_port(id, passthrough_to_ids, name_parts, direction, volume, muted) {
    return {
        'id': id,
        'passthrough_to': passthrough_to_ids,
        'schema': 'audioport.1',
        'name_parts': name_parts,
        'direction': direction,
        'volume': volume,
        'muted': muted
    };
}

function generate_midi_port(id, passthrough_to_ids, name_parts, direction, muted) {
    return {
        'id': id,
        'passthrough_to': passthrough_to_ids,
        'schema': 'midiport.1',
        'name_parts': name_parts,
        'direction': direction,
        'muted': muted
    };
}

function generate_loop_channel(id, mode, type, data_length, volume, connected_port_ids) {
    return {
        'id': id,
        'schema': 'channel.1',
        'mode': mode,
        'type': type,
        'data_length': data_length,
        'volume': volume,
        'connected_port_ids': connected_port_ids
    };
}

function generate_loop(id, name, length, is_master, channels) {
    return {
        'id': id,
        'name': name,
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

function generate_scripts(scripts, active_script_id) {
    return {
        'schema': 'scripts.1',
        'scripts': scripts,
        'active_script_id': active_script_id
    }
}

function generate_session(tracks, ports, scenes, scripts) {
    return {
        'schema': 'session.1',
        'tracks': tracks,
        'ports': ports,
        'scenes': scenes,
        'scripts': scripts
    }
}

function generate_scene(id, name, loop_ids) {
    return {
        'schema': 'scene.1',
        'id': id,
        'name': name,
        'loop_ids': loop_ids
    }
}

function generate_script(id, name, length_cycles, elements) {
    return {
        'schema': 'script.1',
        'id': id,
        'name': name,
        'length_cycles': length_cycles,
        'elements': elements
    }
}

function generate_script_element(name, duration, actions) {
    return {
        'schema': 'script_elem.1',
        'name': name,
        'duration': duration,
        'actions': actions
    }
}

function generate_default_track(
    name,
    n_loops,
    id,
    first_loop_is_master,
    port_name_base,
    n_audio_dry = 0,
    n_audio_wet = 0,
    n_audio_direct = 2,
    have_midi_dry = false,
    have_midi_direct = true
    ) {
    var audio_dry_port_pairs = Array.from(Array(n_audio_dry).keys()).map((idx) => {
        var idx_str = (idx+1).toString();
        var in_id_post = n_audio_dry == 1 ? "_in" : "_in_" + idx_str;
        var out_id_post = n_audio_dry == 1 ? "_send" : "_send_" + idx_str;
        var in_id = id + in_id_post;
        var out_id = id + out_id_post;
        return [
            generate_audio_port(in_id, [out_id], [port_name_base, in_id_post], 'input', 1.0, false),
            generate_audio_port(out_id, [], [port_name_base, out_id_post], 'output', 1.0, false),
        ]
    })
    var audio_wet_port_pairs = Array.from(Array(n_audio_wet).keys()).map((idx) => {
        var idx_str = (idx+1).toString();
        var in_id_post = n_audio_wet == 1 ? "_return" : "_return_" + idx_str;
        var out_id_post = n_audio_wet == 1 ? "_out" : "_out_" + idx_str;
        var in_id = id + in_id_post;
        var out_id = id + out_id_post;
        return [
            generate_audio_port(in_id, [out_id], [port_name_base, in_id_post], 'input', 1.0, false),
            generate_audio_port(out_id, [], [port_name_base, out_id_post], 'output', 1.0, false),
        ]
    })
    var audio_direct_port_pairs = Array.from(Array(n_audio_direct).keys()).map((idx) => {
        var idx_str = (idx+1).toString();
        var in_id_post = n_audio_direct == 1 ? "_in" : "_in_" + idx_str;
        var out_id_post = n_audio_direct == 1 ? "_out" : "_out_" + idx_str;
        var in_id = id + in_id_post;
        var out_id = id + out_id_post;
        return [
            generate_audio_port(in_id, [out_id], [port_name_base, in_id_post], 'input', 1.0, false),
            generate_audio_port(out_id, [], [port_name_base, out_id_post], 'output', 1.0, false),
        ]
    })
    var midi_direct_port_pairs = have_midi_direct ? (() => {
        var in_id_post = "_midi_in";
        var out_id_post = "_midi_out";
        var in_id = id + in_id_post;
        var out_id = id + out_id_post;
        return [[
            generate_midi_port(in_id, [out_id], [port_name_base, in_id_post], 'input', false),
            generate_midi_port(out_id, [], [port_name_base, out_id_post], 'output', false),
        ]]
    })() : [];
    var midi_dry_port_pairs = have_midi_dry ? (() => {
        var in_id_post = "_midi_in";
        var out_id_post = "_midi_send";
        var in_id = id + in_id_post;
        var out_id = id + out_id_post;
        return [[
            generate_midi_port(in_id, [out_id], [port_name_base, in_id_post], 'input', false),
            generate_midi_port(out_id, [], [port_name_base, out_id_post], 'output', false),
        ]]
    })() : [];

    var loops = []
    for (var i=0; i<n_loops; i++) {
        var ii = i.toString();
        var channels = []
        audio_direct_port_pairs.forEach((pair, idx) => {
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_audio_direct_" + idx.toString(), 'direct', 'audio', 0, 1.0, pair.map(p => p.id)
            ))
        })
        midi_direct_port_pairs.forEach((pair, idx) => {
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_midi_direct_" + idx.toString(), 'direct', 'midi', 0, 1.0, pair.map(p => p.id)
            ))
        })
        audio_dry_port_pairs.forEach((pair, idx) => {
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_audio_dry_" + idx.toString(), 'dry', 'audio', 0, 1.0, pair.map(p => p.id)
            ))
        })
        midi_dry_port_pairs.forEach((pair, idx) => {
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_midi_dry_" + idx.toString(), 'dry', 'midi', 0, 1.0, pair.map(p => p.id)
            ))
        })
        audio_wet_port_pairs.forEach((pair, idx) => {
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_audio_wet_" + idx.toString(), 'wet', 'audio', 0, 1.0, pair.map(p => p.id)
            ))
        })
        var loop = generate_loop(id+'_loop_'+ii, "(" + ii + ")", 0, (first_loop_is_master && i==0) ? true : false, channels);
        loops.push(loop);
    }
    var all_ports = [];
    [audio_direct_port_pairs, audio_dry_port_pairs, audio_wet_port_pairs, midi_direct_port_pairs, midi_dry_port_pairs].forEach(pairs => {
        pairs.forEach(p => p.forEach(pp => all_ports.push(pp)))
    })
    var track = generate_track(id, name, loops, all_ports);
    return track;
}

function generate_default_session() {
    var master_track = generate_default_track("Master", 1, 'master', true, 'master_loop', 0, 0, 1, false, false)
    master_track.loops[0].name = "master"
    var session = generate_session([master_track], [], [], generate_scripts([], ""));

    return session;    
}