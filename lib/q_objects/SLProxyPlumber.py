from PyQt6.QtCore import QObject
import re
import sys

sys.path.append('../..')

from third_party.pyjacklib import jacklib

# SLProxyPlumber maintains internal JACK connections between a Jackd proxy server
# and a Sooperlooper instance.
class SLProxyPlumber(QObject):
    def __init__(self, parent, jack_client, jack_library_inst, sooperlooper_client_name, loops_per_track):
        super(SLProxyPlumber, self).__init__(parent)
        self.sl_client_name = sooperlooper_client_name
        self.jack_client = jack_client
        self.jack = jack_library_inst
        self.loops_per_track = loops_per_track

    def __enter__(self):
        self.update_connections()

    def __exit__(self, type, value, traceback):
        pass
    
    def achieve_port_connections(self, src_name, dst_names, all_ports, exact=True):
        if not src_name in all_ports:
            return
        
        src = self.jack.port_by_name(self.jack_client, src_name)

        if src == None:
            return
        
        connected_ports = self.jack.c_char_p_p_to_list(list(self.jack.port_get_all_connections(self.jack_client, src)))

        # Disconnect any non-needed connections
        if exact:
            not_needed = [p for p in connected_ports if p not in dst_names]
            for p in not_needed:
                self.jack.disconnect(self.jack_client, src_name, p)
        
        # Make new connections
        for dst_name in [p for p in dst_names if p not in connected_ports]:
            self.jack.connect(self.jack_client, src_name, dst_name)

    def update_connections(self):
        all_ports = self.jack.c_char_p_p_to_list(self.jack.get_ports(self.jack_client))

        for port in all_ports:
            # Inputs should go to their resp. loops
            m = re.match(r'system:track_([1-9]+)_in_([lr])', port)
            if m:
                track_idx = int(m.group(1)) - 1
                loop_idxs = [track_idx * self.loops_per_track + i for i in range(self.loops_per_track)]
                src_channel = m.group(2) # l or r
                dst_channel = '1' if src_channel == 'l' else '2'
                dst_names = ['{}:loop{}_in_{}'.format(self.sl_client_name, loop_idx, dst_channel) for loop_idx in loop_idxs]
                dst_names = [dst_name for dst_name in dst_names if dst_name in all_ports]
                self.achieve_port_connections(port, dst_names, all_ports)
            
            if port == '{}:common_out_1'.format(self.sl_client_name):
                dst_name = 'system:common_out_l'
                if dst_name in all_ports:
                    self.achieve_port_connections(port, [dst_name], all_ports)
                else:
                    print("SLProxyPlumber: destination {} does not exist for source {}".format(dst_name, port))
            
            if port == '{}:common_out_2'.format(self.sl_client_name):
                dst_name = 'system:common_out_r'
                if dst_name in all_ports:
                    self.achieve_port_connections(port, [dst_name], all_ports)
                else:
                    print("SLProxyPlumber: destination {} does not exist for source {}".format(dst_name, port))

