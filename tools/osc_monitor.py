from pythonosc.dispatcher import Dispatcher
from pythonosc.osc_server import BlockingOSCUDPServer
import pprint
import sys

def default_handler(address, *args):
    pprint.pprint([address, *args])


dispatcher = Dispatcher()
dispatcher.set_default_handler(default_handler)

ip = sys.argv[1]
port = sys.argv[2]

server = BlockingOSCUDPServer((ip, int(port)), dispatcher)
print('Listening on {}:{}...'.format(ip, port))
server.serve_forever()