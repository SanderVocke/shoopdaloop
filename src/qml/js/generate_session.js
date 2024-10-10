function generate_audio_port(id, name_parts, type, input_connectability, output_connectability, gain, muted, passthrough_muted, internal_port_connection_ids, external_port_connections, min_n_ringbuffer_samples) {
    var rval = {
        'id': id,
        'internal_port_connections': internal_port_connection_ids,
        'schema': 'audioport.1',
        'name_parts': name_parts,
        'type': type,
        'input_connectability': input_connectability,
        'output_connectability': output_connectability,
        'gain': gain,
        'muted': muted,
        'passthrough_muted': passthrough_muted,
        'external_port_connections': external_port_connections,
        'min_n_ringbuffer_samples': min_n_ringbuffer_samples
    }
    return rval
}

function generate_midi_port(id, name_parts, type, input_connectability, output_connectability, muted, passthrough_muted, internal_port_connection_ids, external_port_connections, min_n_ringbuffer_samples) {
    var rval = {
        'id': id,
        'internal_port_connections': internal_port_connection_ids,
        'schema': 'midiport.1',
        'name_parts': name_parts,
        'type': type,
        'input_connectability': input_connectability,
        'output_connectability': output_connectability,
        'muted': muted,
        'passthrough_muted': passthrough_muted,
        'external_port_connections': external_port_connections,
        'min_n_ringbuffer_samples': min_n_ringbuffer_samples
    }
    return rval
}

function generate_loop_channel(id, mode, type, data_length, start_offset, n_preplay_samples, gain, connected_port_ids) {
    return {
        'id': id,
        'schema': 'channel.1',
        'mode': mode,
        'type': type,
        'data_length': data_length,
        'start_offset': start_offset,
        'n_preplay_samples': n_preplay_samples,
        'gain': gain,
        'connected_port_ids': connected_port_ids
    };
}

function generate_loop(id, name, length, is_sync, channels) {
    return {
        'id': id,
        'name': name,
        'schema': 'loop.1',
        'length': length,
        'is_sync': is_sync,
        'channels': channels
    }
}

function generate_track(id, name, loops, ports, fx_chain) {
    var rval = {
        'id': id,
        'schema': 'track.1',
        'name': name,
        'loops': loops,
        'ports': ports
    }
    if (fx_chain) { rval.fx_chain = fx_chain }
    return rval
}

function generate_bus(id, name, ports, fx_chain) {
    var rval = {
        'id': id,
        'schema': 'bus.1',
        'name': name,
        'ports': ports
    }
    if (fx_chain) { rval.fx_chain = fx_chain }
    return rval
}

function generate_fx_chain(id, title, type, ports) {
    return {
        'id': id,
        'title': title,
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

function generate_session(app_version, sample_rate, track_groups, buses, ports, scripts, fx_chain_states) {
    var rval = {
        'schema': 'session.1',
        'app_version': app_version,
        'track_groups': track_groups,
        'buses': buses,
        'ports': ports,
        'scripts': scripts,
        'fx_chain_states': fx_chain_states
    }
    if (sample_rate) { rval["sample_rate"] = sample_rate }
    return rval
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

function generate_default_track(
    name,
    n_loops,
    id,
    first_loop_is_sync,
    port_name_base,
    n_audio_dry = 0,
    n_audio_wet = 0,
    n_audio_direct = 2,
    have_midi_dry = false,
    have_midi_direct = true,
    have_drywet_explicit_ports = false,
    drywet_carla_type = undefined
    ) {
    let default_ringbuffer_size = 48000 * 30 // normally 30 seconds (TODO more flexible)

    function fx_chain_port_id_part(typestr, inoutstr, idx) {
        var r = "_fx_chain_" + typestr + "_" + inoutstr;
        if(idx != undefined) {
            r = r + "_" + (idx+1).toString()
        }
        return r
    }
    function external_port_id_part(typestr, inoutstr, connstr, idx) {
        return "_" + typestr + "_" + connstr + "_" + inoutstr + "_" + (idx+1).toString()
    }

    var fx_chain = null
    var n_fx_audio_outputs = 0
    var n_fx_audio_inputs = 0
    var n_fx_midi_inputs = 0
    if (drywet_carla_type) {
        n_fx_audio_inputs =
            drywet_carla_type == 'carla_rack' ? 2 :
            drywet_carla_type == 'test2x2x1' ? 2 :
            drywet_carla_type == 'carla_patchbay' ? 2 :
            drywet_carla_type == 'carla_patchbay_16' ? 16 :
            0
        n_fx_audio_outputs = n_fx_audio_inputs
        n_fx_midi_inputs = 1
        var ports = []
        for (var i=0; i<n_fx_audio_inputs; i++) {
            var id_post = fx_chain_port_id_part("audio", "in", i)
            ports.push(generate_audio_port(
                id + id_post,
                [port_name_base, id_post],
                'driver',
                ['internal'], // input
                [], // output
                1.0,
                false,
                false,
                [], // internal connections
                [],
                0 // no always on ringbuffer
            ))
        }
        for (var i=0; i<n_fx_audio_outputs; i++) {
            var id_post = fx_chain_port_id_part("audio", "out", i)
            ports.push(generate_audio_port(
                id + id_post,
                [port_name_base, id_post],
                'driver',
                [], // input
                ['internal'], // output
                1.0,
                false,
                false,
                [id + external_port_id_part("audio", "out", "wet",  i)], // internal connections
                [],
                default_ringbuffer_size // always on ringbuffer on, because this port will be recorded by a channel
            ))
        }
        for (var i=0; i<n_fx_midi_inputs; i++) {
            var id_post = fx_chain_port_id_part("midi", "in", i)
            ports.push(generate_midi_port(
                id + id_post,
                [port_name_base, id_post],
                'driver',
                ['internal'], // input
                [], // output
                false,
                false,
                [], // internal connections
                [],
                0 // no always on ringbuffer
            ))
        }
        
        fx_chain = generate_fx_chain(
            id + '_fx_chain',
            name,
            drywet_carla_type,
            ports
        )
    }

    var audio_dry_port_pairs = Array.from(Array(n_audio_dry).keys()).map((idx) => {
        var in_id_post = external_port_id_part("audio", "in", "dry", idx);
        var in_id = id + in_id_post;
        var send_id_post = external_port_id_part("audio", "send", "dry", idx);
        var send_id = id + send_id_post;
        var fx_in_id = id + fx_chain_port_id_part("audio", "in", idx);

        var rval = [
            generate_audio_port(
                in_id,
                [port_name_base, in_id_post],
                'driver',
                ['external'], // input
                ['internal'], // output
                1.0,
                false,
                true,
                have_drywet_explicit_ports ? [send_id] : [fx_in_id], // internal connections
                [],
                default_ringbuffer_size // always on ringbuffer on, because this port will be recorded by a channel
            )]
        if (have_drywet_explicit_ports) {
            rval.push(generate_audio_port(
                send_id,
                [port_name_base, send_id_post],
                'driver',
                ['internal'], // input
                ['external'], // output
                1.0,
                false,
                false,
                [], // internal connections
                [],
                0 // no always on ringbuffer
            ))
        }
        
        return rval
    })
    var audio_wet_port_pairs = Array.from(Array(n_audio_wet).keys()).map((idx) => {
        var return_id_post = external_port_id_part("audio", "return", "wet", idx);
        var return_id = id + return_id_post;
        var out_id_post = external_port_id_part("audio", "out", "wet", idx);
        var out_id = id + out_id_post;

        var rval = []
        if (have_drywet_explicit_ports) {
            rval.push(generate_audio_port(
                return_id,
                [port_name_base, return_id_post],
                'driver',
                ['external'], // input
                ['internal'], // output
                1.0,
                false,
                false,
                [out_id], // internal connections
                [],
                default_ringbuffer_size // always on ringbuffer on, because this port will be recorded by a channel
            ))
        }
        rval.push(generate_audio_port(
            out_id,
            [port_name_base, out_id_post],
            'driver',
            ['internal'], // input
            ['external'], // output
            1.0,
            false,
            false,
            [], // internal connections
            [],
            0 // no always on ringbuffer
        ))
        
        return rval
    })
    var audio_direct_port_pairs = Array.from(Array(n_audio_direct).keys()).map((idx) => {
        var idx_str = (idx+1).toString();
        var in_id_post = n_audio_direct == 1 ? "_direct_in" : "_direct_in_" + idx_str;
        var out_id_post = n_audio_direct == 1 ? "_direct_out" : "_direct_out_" + idx_str;
        var in_id = id + in_id_post;
        var out_id = id + out_id_post;
        return [
            generate_audio_port(
                in_id,
                [port_name_base, in_id_post],
                'driver',
                ['external'], // input
                ['internal'], // output
                1.0,
                false,
                true,
                [out_id], // internal connections
                [],
                default_ringbuffer_size // always on ringbuffer on, because this port will be recorded by a channel
            ),
            generate_audio_port(
                out_id,
                [port_name_base, out_id_post],
                'driver',
                ['internal'], // input
                ['external'], // output
                1.0,
                false,
                false,
                [], // internal connections
                [],
                0 // no always on ringbuffer
            ),
        ]
    })
    var midi_direct_port_pairs = have_midi_direct ? (() => {
        var in_id_post = "_direct_midi_in";
        var out_id_post = "_direct_midi_out";
        var in_id = id + in_id_post;
        var out_id = id + out_id_post;
        return [[
            generate_midi_port(
                in_id,
                [port_name_base, in_id_post],
                'driver',
                ['external'], // input
                ['internal'], // output
                false,
                true,
                [out_id], // internal connections
                [],
                default_ringbuffer_size // always on ringbuffer on, because this port will be recorded by a channel
            ),
            generate_midi_port(
                out_id,
                [port_name_base, out_id_post],
                'driver',
                ['internal'], // input
                ['external'], // output
                false,
                false,
                [], // internal connections
                [],
                0 // no always on ringbuffer
            ),
        ]]
    })() : [];
    var midi_dry_port_pairs = have_midi_dry ? (() => {
        var in_id_post = "_dry_midi_in";
        var in_id = id + in_id_post;
        var send_id_post = "_dry_midi_send";
        var send_id = id + send_id_post;
        var fx_in_id = id + fx_chain_port_id_part("midi", "in", 0);

        var rval = [
            generate_midi_port(
                in_id,
                [port_name_base, in_id_post],
                'driver',
                ['external'], // input
                ['internal'], // output
                false,
                true,
                have_drywet_explicit_ports ? [send_id] : [fx_in_id], // internal connections
                [],
                default_ringbuffer_size // always on ringbuffer on, because this port will be recorded by a channel
            )
        ]
        if (have_drywet_explicit_ports) {
            rval.push(generate_midi_port(
                send_id,
                [port_name_base, send_id_post],
                'driver',
                ['internal'], // input
                ['external'], // output
                false,
                false,
                [], // internal connections
                [],
                0 // no always on ringbuffer
            ))
        }

        return [rval]
    })() : [];

    var loops = []
    for (var i=0; i<n_loops; i++) {
        var ii = i.toString();
        var channels = []
        audio_direct_port_pairs.forEach((pair, idx) => {
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_audio_direct_" + (idx+1).toString(), 'direct', 'audio', 0, 0, 0, 1.0, pair.map(p => p.id)
            ))
        })
        midi_direct_port_pairs.forEach((pair, idx) => {
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_midi_direct_" + (idx+1).toString(), 'direct', 'midi', 0, 0, 0, 1.0, pair.map(p => p.id)
            ))
        })
        audio_dry_port_pairs.forEach((pair, idx) => {
            var ports_to_connect = pair.map(p => p.id)
            if (n_fx_audio_inputs > idx) { ports_to_connect.push(id + fx_chain_port_id_part("audio", "in", idx)) }
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_audio_dry_" + (idx+1).toString(), 'dry', 'audio', 0, 0, 0, 1.0, ports_to_connect
            ))
        })
        midi_dry_port_pairs.forEach((pair, idx) => {
            var ports_to_connect = pair.map(p => p.id)
            if (n_fx_midi_inputs > idx) { ports_to_connect.push(id + fx_chain_port_id_part("midi", "in", idx)) }
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_midi_dry_" + (idx+1).toString(), 'dry', 'midi', 0, 0, 0, 1.0, ports_to_connect
            ))
        })
        audio_wet_port_pairs.forEach((pair, idx) => {
            var ports_to_connect = pair.map(p => p.id)
            if (n_fx_audio_outputs > idx) { ports_to_connect.push(id + fx_chain_port_id_part("audio", "out", idx)) }
            channels.push(generate_loop_channel(
                id + '_loop_' + ii + "_audio_wet_" + (idx+1).toString(), 'wet', 'audio', 0, 0, 0, 1.0, ports_to_connect
            ))
        })
        var loop = generate_loop(id+'_loop_'+ii, "(" + (i+1).toString() + ")", 0, (first_loop_is_sync && i==0) ? true : false, channels);
        loops.push(loop);
    }
    var all_ports = [];
    [audio_direct_port_pairs, audio_dry_port_pairs, audio_wet_port_pairs, midi_direct_port_pairs, midi_dry_port_pairs].forEach(pairs => {
        pairs.forEach(p => p.forEach(pp => all_ports.push(pp)))
    })

    var track = generate_track(id, name, loops, all_ports, fx_chain);
    return track;
}

function generate_default_session(app_version, sample_rate=null, add_sync_track=true, n_loops_in_sync_track=1, n_audio_channels_in_sync_track=1, initial_tracks=[]) {
    // tracks
    var main_tracks = {
        'name': 'main',
        'tracks': initial_tracks
    }
    var sync_tracks = {
        'name': 'sync',
        'tracks': []
    }
    if (add_sync_track) {
        var sync_track = generate_default_track("Sync", n_loops_in_sync_track, 'sync', true, 'sync_loop', 0, 0, n_audio_channels_in_sync_track, false, false, false, undefined)
        sync_track.loops[0].name = "sync loop"
        sync_tracks.tracks.push(sync_track)
    }

    // combine it all
    var session = generate_session(
        app_version,
        sample_rate,
        [ sync_tracks, main_tracks ],
        [],
        [],
        [],
        []
    );

    return session;    
}

function convert_session_descriptor_sample_rate(descriptor, from, to) {
    var rval = descriptor
    let convert = to / from
    rval['sample_rate'] = to
    rval['track_groups'].forEach(g => {
        g['tracks'].forEach(t => {
            t.loops.forEach(l => {
                // Note: using ceil for length and data length fields ensures that channels/loops which are exactly
                // N x (sync loop length) will never be rounded to be more than N x (sync loop length) after all
                // conversions (assuming N > 1). Otherwise, loops may wait an entire extra sync loop cycle because
                // they are 1 sample longer.
                l.length = Math.ceil(convert * l.length)
                l.channels.forEach(c => {
                    c.data_length = Math.ceil(convert * c.data_length)
                    c.start_offset = Math.round(convert * c.start_offset)
                    c.n_preplay_samples = Math.round(convert * c.n_preplay_samples)
                })
            })
        })
    })

    return rval
}