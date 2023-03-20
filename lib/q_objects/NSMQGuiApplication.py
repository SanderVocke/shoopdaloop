import sys
import os
import signal
sys.path.append('../..')
import importlib
pynsm = importlib.import_module('third_party.new-session-manager.extras.pynsm.nsmclient')

script_pwd = os.path.dirname(__file__)

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

class NSMQGuiApplication(QGuiApplication):
    def __init__(self, title, argv):
        super(NSMQGuiApplication, self).__init__(argv)
        self.nsm_client = None
        self.title = title
        signal.signal(signal.SIGINT, exit_signal_handler)
        signal.signal(signal.SIGQUIT, exit_signal_handler)
        signal.signal(signal.SIGTERM, exit_signal_handler)

        try:
            nsm_client = pynsm.NSMClient(
                prettyName = title,
                supportsSaveStatus = False,
                saveCallback = lambda path, session, client: self.save_session_handler(path, session, client),
                openOrNewCallback = lambda path, session, client: self.load_session_handler(path, session, client),
                exitProgramCallback = lambda path, session, client: self.exit_nsm_handler(),
                loggingLevel = 'info'
            )
            self.title = nsm_client.ourClientNameUnderNSM
        except pynsm.NSMNotRunningError as e:
            pass
        
        timer = QTimer()
        timer.start(100)
        timer.timeout.connect(lambda: self.tick())
    
    # The following ensures the Python interpreter has a chance to run, which
    # would not happen otherwise once the Qt event loop starts - and this 
    # is necessary for allowing the signal handlers to do their job.
    # It's a hack but it works...
    # NOTE: later added NSM react as another goal for this task.
    def tick():
        global nsm_client
        if nsm_client:
                nsm_client.reactToMessage()

    def exit(self, retcode):
        if self.nsm_client:
            print("Requesting exit from NSM.")
            self.nsm_client.serverSendExitToSelf()
        else:
            print("Exiting.")
            self.really_exit()

    def really_exit(self, retcode):
        super.exit(retcode)
    
    def nsm_save_session(path):
        print("NSM: save session {}".format(path))
        time.sleep(10.0)
        # counter = backend_mgr.session_save_counter
        # failed_counter = backend_mgr.session_save_failed_counter
        # qml_app_state.save_session(path, True)
        # while backend_mgr.session_save_counter == counter:
        #     time.sleep(0.01)
        #     app.processEvents() # We are on the GUI thread, ensure we stay responsive
        # if backend_mgr.session_save_failed_counter > failed_counter:
        #     raise Exception('NSM session save failed.')
        print("NSM: save session finished.")
    
    def nsm_load_session(path):
        print("NSM: load session {}".format(path))
        time.sleep(10.0)
        # counter = backend_mgr.session_load_counter
        # failed_counter = backend_mgr.session_load_failed_counter
        # qml_app_state.load_session(path)
        # while backend_mgr.session_load_counter == counter:
        #     time.sleep(0.01)
        #     app.processEvents() # We are on the GUI thread, ensure we stay responsive
        # if backend_mgr.session_load_failed_counter > failed_counter:
        #     raise Exception('NSM session load failed.')
        print("NSM: load session finished.")

    def save_session_handler(path, session, client):
        #initialize_app_if_not_already(client)
        nsm_save_session(path)

    def load_session_handler(path, session, client):
        #initialize_app_if_not_already(client)
        if os.path.isfile(path):
            print("NSM: load session {}".format(path))
            nsm_load_session(path)
        else:
            print("NSM: create session {}".format(path))
            print(qml_app_state)
            nsm_save_session(path)
    
    def exec(self):
        print("Entering Qt event loop.")
        return super.exec()