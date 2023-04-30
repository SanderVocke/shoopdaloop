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

function generate_track(id, name, loops, ports, fx_chains) {
    return {
        'id': id,
        'schema': 'track.1',
        'name': name,
        'loops': loops,
        'ports': ports,
        'fx_chains': fx_chains
    }
}

function generate_fx_chain(id, type, ports) {
    return {
        'id': id,
        'schema': 'fx_chain.1',
        'type': type,
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

function generate_script_loop_action(target_ids, action, delay_cycles, args) {
    return {
        'schema': 'script_loop_action.1',
        'target_ids': target_ids,
        'action': action,
        'delay_cycles': delay_cycles,
        'args': args
    }
}

function generate_script_scene_action(target_ids, action, delay_cycles, args) {
    return {
        'schema': 'script_scene_action.1',
        'target_ids': target_ids,
        'action': action,
        'delay_cycles': delay_cycles,
        'args': args
    }
}

function generate_script_track_action(target_ids, action, delay_cycles, args) {
    return {
        'schema': 'script_track_action.1',
        'target_ids': target_ids,
        'action': action,
        'delay_cycles': delay_cycles,
        'args': args
    }
}

function generate_script_global_action(action, delay_cycles, args) {
    return {
        'schema': 'script_global_action.1',
        'action': action,
        'delay_cycles': delay_cycles,
        'args': args
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
    have_midi_direct = true,
    have_drywet_jack_ports = false,
    drywet_carla_type = undefined
    ) {
    function fx_chain_port_id_part(typestr, inoutstr, idx) {
        var r = "_fx_chain_" + typestr + "_" + inoutstr;
        if(idx != undefined) {
            r = r + "_" + (idx+1).toString()
        }
        return r
    }
    function external_port_id_part(typestr, inoutstr, idx) {
        return "_" + typestr + "_" + inoutstr + "_" + (idx+1).toString()
    }

    var fx_chains = []
    if (drywet_carla_type) {
        var n_audio_inputs =
            drywet_carla_type == 'carla_rack' ? 2 :
            drywet_carla_type == 'carla_patchbay' ? 2 :
            drywet_carla_type == 'carla_patchbay_16' ? 16 :
            0
        var n_audio_outputs = n_audio_inputs
        var n_midi_inputs = 1
        var ports = []
        for (var i=0; i<n_audio_inputs; i++) {
            var id_post = fx_chain_port_id_part("audio", "in", i)
            ports.push(generate_audio_port(
                id + id_post,
                [],
                [port_name_base, id_post],
                'output',
                1.0,
                false
            ))
        }
        for (var i=0; i<n_audio_outputs; i++) {
            var id_post = fx_chain_port_id_part("audio", "out", i)
            ports.push(generate_audio_port(
                id + id_post,
                [id + external_port_id_part("audio", "out", i)],
                [port_name_base, id_post],
                'input',
                1.0,
                false
            ))
        }
        for (var i=0; i<n_midi_inputs; i++) {
            var id_post = fx_chain_port_id_part("midi", "in", i)
            ports.push(generate_midi_port(
                id + id_post,
                [],
                [port_name_base, id_post],
                'output',
                false
            ))
        }
        
        fx_chains.push(generate_fx_chain(
            id + '_fx_chain',
            drywet_carla_type,
            ports
        ))
    }

    var audio_dry_port_pairs = Array.from(Array(n_audio_dry).keys()).map((idx) => {
        var in_id_post = external_port_id_part("audio", "in", idx);
        var in_id = id + in_id_post;
        var out_id_post = external_port_id_part("audio", "send", idx);
        var out_id = id + out_id_post;
        var fx_in_id = id + fx_chain_port_id_part("audio", "in", idx);

        var rval = [generate_audio_port(in_id, have_drywet_jack_ports ? [out_id] : [fx_in_id], [port_name_base, in_id_post], 'input', 1.0, false)]
        if (have_drywet_jack_ports) {
            rval.push(generate_audio_port(out_id, [], [port_name_base, out_id_post], 'output', 1.0, false))
        }
        
        return rval
    })
    var audio_wet_port_pairs = Array.from(Array(n_audio_wet).keys()).map((idx) => {
        var in_id_post = external_port_id_part("audio", "return", idx);
        var in_id = id + in_id_post;
        var out_id_post = external_port_id_part("audio", "out", idx);
        var out_id = id + out_id_post;

        var rval = []
        if (have_drywet_jack_ports) {
            rval.push(generate_audio_port(in_id, [out_id], [port_name_base, in_id_post], 'input', 1.0, false))
        }
        rval.push(generate_audio_port(out_id, [], [port_name_base, out_id_post], 'output', 1.0, false))
        
        return rval
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
        var in_id = id + in_id_post;
        var out_id_post = "_midi_send";
        var out_id = id + out_id_post;
        var fx_in_id = id + fx_chain_port_id_part("midi", "in", undefined);

        var rval = [generate_midi_port(in_id, have_drywet_jack_ports ? [out_id] : [fx_in_id], [port_name_base, in_id_post], 'input', false)]
        if (have_drywet_jack_ports) {
            rval.push(generate_midi_port(out_id, [], [port_name_base, out_id_post], 'output', false))
        }

        return [rval]
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

    var track = generate_track(id, name, loops, all_ports, fx_chains);
    return track;
}

function generate_default_session() {
    var master_track = generate_default_track("Master", 1, 'master', true, 'master_loop', 0, 0, 1, false, false, false, undefined)
    master_track.loops[0].name = "master"
    var session = generate_session([master_track], [], [],
        generate_scripts([generate_script("script", "Script", 0, [])], "script"));

    return session;    
}