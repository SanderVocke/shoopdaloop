import threading
import queue

from pythonosc.udp_client import SimpleUDPClient
from pythonosc.osc_server import BlockingOSCUDPServer
from pythonosc.dispatcher import Dispatcher

from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot

import pprint

class SooperLooperOSCLink(QObject):
    # List is e.g. ['/some/path/stuff', 0, 1, 'textarg']. For any received message.
    received = pyqtSignal(list)
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
                print('S: {}'.format(pprint.pformat(msg)))
                self.sent.emit(msg)
                self._snd_queue.task_done()
        self._client_thread = threading.Thread(target=sender, daemon=True)
        self._client = SimpleUDPClient(send_ip, send_port)
        
        # Server part (receiving)
        def rcv(addr, *args):
            try:
                msg = [addr, *args]
                # print('R: {}'.format(pprint.pformat(msg)))
                self.received.emit(msg)
            except Exception as e:
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
    @pyqtSlot(list)
    def send(self, msg):
        self._snd_queue.put(msg)
    
    # Use for messages that expect a response from SooperLooper.
    @pyqtSlot(list, str)
    def send_expect_response(self, msg, return_path):
        self.send(msg + ['osc.udp://{}:{}/'.format(self._rcv_ip, self._rcv_port), return_path])
