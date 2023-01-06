from .flatten import flatten

def get_port_loop_mappings(n_tracks, loops_per_track, loop_channel_names):
    # Set up port loop mappings.
    # n_channels*2 ports per track: dry and wet
    chans = len(loop_channel_names)
    r = {
        'port_name_pairs': [],
        'mixed_output_port_names': [],
        'loops_to_ports': [],
        'loops_soft_sync': [],
        'loops_hard_sync': [],
        'ports_to_mixed_outputs': [],
        'ports_midi_enabled': [],
        # The following key is a list of tuples.
        # The first element is a port for which we should keep an eye on whether
        # its input is connected to anything or not. If not, its input should be
        # taken from another port's input instead (the 2nd tuple element indicates
        # this port).
        'port_input_remaps_if_disconnected': []
    }

    def add_ports_for_track(track_name):
        # add in + send for each channel,
        # then return + out for each channel
        # NOTE: order of loops is important here
        for n in loop_channel_names:
            r['port_name_pairs'].append(
                ('{}_in_{}'.format(track_name, n), '{}_send_{}'.format(track_name, n))
            )
        for channel_idx in range(len(loop_channel_names)):
            r['ports_to_mixed_outputs'].append(-1) #sends are not mixed anywhere
        for n in loop_channel_names:
            r['port_name_pairs'].append(
                ('{}_return_{}'.format(track_name, n), '{}_out_{}'.format(track_name, n))
            )
        for channel_idx in range(len(loop_channel_names)):
            r['ports_to_mixed_outputs'].append(channel_idx)
        
    def add_loop(track_idx):
        base_loop = len(r['loops_to_ports'])
        base_port = track_idx*chans*2
        # Below procudes [0, 1, 2, 3] offset by base port.
        r['loops_to_ports'] += [base_port+i for i in range(chans * 2)]
        # Below produces [0, -1, 0, -1, 0, -1, ...]
        # so that each first channel is soft-synced to master and rest of channels
        # are not soft-synced
        r['loops_soft_sync'] += flatten([[0, -1] for i in range(chans)])
        # Below produces [0, 0, 2, 2, 4, 4, ...] offset by base loop
        # so that channels are hard-linked together
        r['loops_hard_sync'] += flatten([[i*2+base_loop, i*2+base_loop] for i in range(chans)])
        # Add the return-out pair to the input remappings (if nothing connected to the return,
        # take samples from in).
        r['port_input_remaps_if_disconnected'] += [[base_port+2, base_port+0], [base_port+3, base_port+1]]

    for n in loop_channel_names:
        r['mixed_output_port_names'].append('mixed_out_{}'.format(n))

    add_ports_for_track('master_loop')
    add_loop(0)
    # For master loop, synchronize to itself.
    r['loops_soft_sync'][0] = 0
    r['loops_soft_sync'][2] = 2
    
    for track_idx in range(n_tracks):
        add_ports_for_track('track_{}'.format(track_idx+1))
        
        for i in range(loops_per_track):
            add_loop(track_idx+1)
        
        # first two ports (dry l/r) have MIDI.
        r['ports_midi_enabled'] += [track_idx*4, track_idx*4+1]
    
    return r