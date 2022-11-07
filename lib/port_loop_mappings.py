from .flatten import flatten

def get_port_loop_mappings(n_tracks, loops_per_track, loop_channel_names):
    # Set up port loop mappings.
    # n_channels*2 ports per track: dry and wet
    chans = len(loop_channel_names)
    r = {
        'port_name_pairs': [],
        'loops_to_ports': [],
        'loops_soft_sync': [],
        'loops_hard_sync': []
    }

    def add_ports_for_track(track_name):
        for n in loop_channel_names:
            r['port_name_pairs'].append(
                ('{}_in_{}'.format(track_name, n), '{}_send_{}'.format(track_name, n))
            )
            r['port_name_pairs'].append(
                ('{}_return_{}'.format(track_name, n), '{}_out_{}'.format(track_name, n))
            )
        
    def add_loop(track_idx):
        base_loop = len(r['loops_to_ports'])
        base_port = track_idx*chans*2
        # Below procudes [0, 1, 2, 3] offset by base port.
        r['loops_to_ports'] += [base_port+i for i in range(chans * 2)]
        # Below produces [0, -1, 0, -1, 0, -1, ...]
        # so that each first channel is soft-synced to master and rest of channels
        # are not soft-synced
        r['loops_soft_sync'] += flatten([[0, -1] for i in range(chans)])
        # Below produces [0, 0, 1, 1, 2, 2, ...] offset by base loop
        # so that channels are hard-linked together
        r['loops_hard_sync'] += flatten([[i+base_loop, i+base_loop] for i in range(chans)])
    
    print(r['loops_soft_sync'])

    add_ports_for_track('master_loop')
    add_loop(0)
    # For master loop, modify so that it is not soft-synced at all.
    # TODO: how to guarantee synchronization between the master dry
    # and wet loops?
    r['loops_soft_sync'][0] = -1
    r['loops_soft_sync'][2] = -1
    
    for track_idx in range(n_tracks):
        add_ports_for_track('track_{}'.format(track_idx+1))
        
        for i in range(loops_per_track):
            add_loop(track_idx+1)
    
    return r