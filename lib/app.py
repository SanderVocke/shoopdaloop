import os
import sys
from PyQt6.QtGui import QGuiApplication

script_pwd = os.path.dirname(__file__)

class NSMQGuiApplication(QGuiApplication):
    def __init__(self, argv):
        super(NSMQGuiApplication, self).__init__(argv)
        self.nsm_client = None
    
    def exit(self, retcode):
        if nsm_client:
            print("Requesting exit from NSM.")
            nsm_client.serverSendExitToSelf()
        else:
            print("Exiting.")
            self.really_exit()

    def really_exit(self, retcode):
        super.exit(retcode)

def exit_handler():
    current_process = psutil.Process()
    children = current_process.children(recursive=False)
    for child in children:
        print('Send signal {} => {}'.format(sig, child.pid))
        os.kill(child.pid, sig)
    if engine:
        QMetaObject.invokeMethod(engine, 'quit')

# Ensure that we forward any terminating signals to our child
# processes
def exit_signal_handler(sig, frame):
    print('Got signal {}.'.format(sig))    
    print('Exiting due to signal.')
    exit_handler()

def exit_nsm_handler():
    print('Exiting due to NSM request.')
    if backend_mgr:
        backend_mgr.terminate()
    exit_handler()
    print("Exiting.")
    sys.exit(0)

signal.signal(signal.SIGINT, exit_signal_handler)
signal.signal(signal.SIGQUIT, exit_signal_handler)
signal.signal(signal.SIGTERM, exit_signal_handler)