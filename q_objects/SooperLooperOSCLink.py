import threading
import queue

from pythonosc.udp_client import SimpleUDPClient
from pythonosc.osc_server import BlockingOSCUDPServer
from pythonosc.dispatcher import Dispatcher

from PyQt6.QtCore import QObject, QThread, pyqtSignal

import pprint
import re

class SooperLooperOSCLink(QObject):
    # List is e.g. ['/some/path/stuff', 0, 1, 'textarg']. For any received message.
    received = pyqtSignal(list)
    receivedLoopParam = pyqtSignal(int, str, str)
    sent = pyqtSignal(list)

    # TODO for loop parameter specifically

    def __init__(self, parent, send_ip, send_port, recv_ip, recv_port):
        super(SooperLooperOSCLink, self).__init__(parent)
        self._snd_queue = queue.Queue()

        # Client part (sending)
        def sender():
            while True:
                msg = self._snd_queue.get()
                self._client.send_message(msg[0], msg[1:])
                self.sent.emit(msg)
                self._snd_queue.task_done()
        self._client_thread = threading.Thread(target=sender, daemon=True)
        self._client = SimpleUDPClient(send_ip, send_port)
        
        # Server part (receiving)
        def rcv(addr, *args):
            try:
                msg = [addr, *args]
                self.received.emit(msg)
                maybe_loop_param = re.match(r'/sl/([0-9]+)/get', msg[0])
                if maybe_loop_param and len(msg) == 4:
                    self.receivedLoopParam.emit(int(msg[1]), msg[2], str(msg[3]))
            except e:
                print('Failed to receive message: {}'.format(str(e)))

        self._dispatcher = Dispatcher()
        self._dispatcher.set_default_handler(rcv)
        self._server = BlockingOSCUDPServer((recv_ip, recv_port), self._dispatcher)
        self._server_thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._rcv_port = recv_port
        self._rcv_ip = recv_ip

        self._client_thread.start()
        self._server_thread.start()
    
    # Use for messages that don't expect a response.
    # msg is e.g. ['/some/path/stuff', 0, 1, 'textarg']
    def send(self, msg):
        self._snd_queue.put(msg)
    
    # Use for messages that expect a response from SooperLooper.
    def send_expect_response(self, msg, return_path):
        self.send(msg + ['osc.udp://{}:{}/'.format(self._rcv_ip, self._rcv_port), return_path])

    # Use to get a loop parameter from SooperLooper.
    def request_loop_parameter(self, loop_idx, parameter):
        self.send_expect_response(['/sl/{}/get'.format(loop_idx), '{}'.format(parameter)], '/sl/{}/get'.format(loop_idx))
    
    # Use to get loops' parameters from SooperLooper.
    def request_loops_parameters(self, loop_idxs, parameters):
        for loop_idx in loop_idxs:
            for parameter in parameters:
                self.send_expect_response(['/sl/{}/get'.format(loop_idx), '{}'.format(parameter)], '/sl/{}/get'.format(loop_idx))
    

