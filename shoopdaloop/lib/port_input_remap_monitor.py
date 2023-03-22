from threading import Thread
from functools import partial
from time import sleep

import sys
sys.path.append('..')

from third_party.pyjacklib import jacklib

def monitor(jack_client, jack_input_ports, port_input_remaps_if_disconnected, remap_cb, reset_remap_cb):
    remapped_ports_cache = set()

    while True:
        sleep(0.5)

        for pair in port_input_remaps_if_disconnected:
            port = pair[0]
            remapped = pair[1]
            if jacklib.port_connected(jack_input_ports[port]) == 0 and \
               port not in remapped_ports_cache:
                # print('remapping port {} to {}'.format(port, remapped))
                remap_cb(port, remapped)
                remapped_ports_cache.add(port)
            elif jacklib.port_connected(jack_input_ports[port]) > 0 and \
               port in remapped_ports_cache:
                # print('un-remapping port {}'.format(port))
                reset_remap_cb(port)
                remapped_ports_cache.remove(port)


def start_port_input_remapping_monitor(jack_client, # The JACK client
                                       jack_input_ports, # The JACK input ports (idxs should match the mapping)
                                       port_input_remaps_if_disconnected, # See definition in port_loop_remappings
                                       remap_cb, # Takes port idx, port idx to take input from
                                       reset_remap_cb # Takes port idx
):

    t = Thread(target=partial(monitor, jack_client, jack_input_ports, port_input_remaps_if_disconnected, remap_cb, reset_remap_cb),
               daemon=True)
    t.start()