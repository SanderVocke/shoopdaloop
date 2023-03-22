import sys
import os
import signal
import time

from PyQt6.QtQml import QQmlApplicationEngine, QJSValue
from PyQt6.QtGui import QGuiApplication
from PyQt6.QtCore import QTimer, QObject, Q_ARG, QMetaObject, Qt

from ...third_party.pynsm.nsmclient import NSMClient

from ..qml_helpers import register_shoopdaloop_qml_classes

from .SchemaValidator import SchemaValidator
from .FileIO import FileIO

class Application(QGuiApplication):
    def __init__(self, title, main_qml):
        super(Application, self).__init__([])
        self.nsm_client = None
        self.title = title
        signal.signal(signal.SIGINT, self.exit_signal_handler)
        signal.signal(signal.SIGQUIT, self.exit_signal_handler)
        signal.signal(signal.SIGTERM, self.exit_signal_handler)
        
        self.nsmtimer = QTimer()
        self.nsmtimer.start(100)
        self.nsmtimer.timeout.connect(self.tick)

        register_shoopdaloop_qml_classes()
        self.engine = QQmlApplicationEngine()
        self.engine.quit.connect(self.quit)

        self.file_io = FileIO()
        self.schema_validator = SchemaValidator()
        self.engine.rootContext().setContextProperty("schema_validator", self.schema_validator)
        self.engine.rootContext().setContextProperty("file_io", self.file_io)
        if main_qml:
            self.engine.load(main_qml)
        
        try:
            self.nsm_client = pynsm.NSMClient(
                prettyName = title,
                supportsSaveStatus = False,
                saveCallback = lambda path, session, client: self.save_session_handler(path, session, client),
                openOrNewCallback = lambda path, session, client: self.load_session_handler(path, session, client),
                exitProgramCallback = lambda path, session, client: self.nsm_exit_handler(),
                loggingLevel = 'info'
            )
            self.title = self.nsm_client.ourClientNameUnderNSM
        except pynsm.NSMNotRunningError as e:
            pass
    
    def exit(self, retcode):
        if self.nsm_client:
            print("Requesting exit from NSM.")
            self.nsm_client.serverSendExitToSelf()
        else:
            print("Exiting.")
            self.really_exit()

    def really_exit(self, retcode):
        super(Application, self).exit(retcode)
    
    # Ensure that we forward any terminating signals to our child
    # processes
    def exit_signal_handler(self, sig, frame):
        print('Got signal {}.'.format(sig))    
        print('Exiting due to signal.')
        self.exit_handler()
    
    def exit_handler(self):
        # current_process = psutil.Process()
        # children = current_process.children(recursive=False)
        # for child in children:
        #     print('Send signal {} => {}'.format(sig, child.pid))
        #     os.kill(child.pid, sig)
        if self.engine:
            QMetaObject.invokeMethod(self.engine, 'quit')
    
    # The following ensures the Python interpreter has a chance to run, which
    # would not happen otherwise once the Qt event loop starts - and this 
    # is necessary for allowing the signal handlers to do their job.
    # It's a hack but it works...
    # NOTE: later added NSM react as another goal for this task.
    def tick(self):
        if self.nsm_client:
                self.nsm_client.reactToMessage()

    def exit(self, retcode):
        if self.nsm_client:
            print("Requesting exit from NSM.")
            self.nsm_client.serverSendExitToSelf()
        else:
            print("Exiting.")
            self.really_exit()

    def really_exit(self, retcode):
        super(Application, self).exit(retcode)
    
    def nsm_save_session(self, path):
        print("NSM: save session {}".format(path))
        session = self.engine.rootObjects()[0].findChild(QObject, 'session')
        if session:
            QMetaObject.invokeMethod(session, "save_session", Qt.ConnectionType.DirectConnection, Q_ARG('QVariant', path))
            while session.property("saving"):
                time.sleep(0.01)
                self.processEvents()
        else:
            print("No active session object found, ignoring save session.")
        # time.sleep(10.0)
        # counter = backend_mgr.session_save_counter
        # failed_counter = backend_mgr.session_save_failed_counter
        # qml_app_state.save_session(path, True)
        # while backend_mgr.session_save_counter == counter:
        #     time.sleep(0.01)
        #     app.processEvents() # We are on the GUI thread, ensure we stay responsive
        # if backend_mgr.session_save_failed_counter > failed_counter:
        #     raise Exception('NSM session save failed.')
        print("NSM: save session finished.")
    
    def nsm_load_session(self, path):
        print("NSM: load session {}".format(path))
        session = self.engine.rootObjects()[0].findChild(QObject, 'session')
        if session:
            QMetaObject.invokeMethod(session, "load_session", Qt.ConnectionType.DirectConnection, Q_ARG('QVariant', path))
            while session.property("loading"):
                time.sleep(0.01)
                self.processEvents()
        else:
            print("No active session object found, ignoring load session.")
        # counter = backend_mgr.session_load_counter
        # failed_counter = backend_mgr.session_load_failed_counter
        # qml_app_state.load_session(path)
        # while backend_mgr.session_load_counter == counter:
        #     time.sleep(0.01)
        #     app.processEvents() # We are on the GUI thread, ensure we stay responsive
        # if backend_mgr.session_load_failed_counter > failed_counter:
        #     raise Exception('NSM session load failed.')
        print("NSM: load session finished.")

    def save_session_handler(self, path, session, client):
        print ('save_session_handler')
        #initialize_app_if_not_already(client)
        self.nsm_save_session(path)

    def load_session_handler(self, path, session, client):
        print ('load_session_handler')
        #initialize_app_if_not_already(client)
        if os.path.isfile(path):
            print("NSM: load session {}".format(path))
            self.nsm_load_session(path)
        else:
            print("NSM: create session {}".format(path))
            self.nsm_save_session(path)
    
    def nsm_exit_handler(self):
        print('nsm_exit_handler')
        print('Exiting due to NSM request.')
        #if backend_mgr:
        #    backend_mgr.terminate()
        self.exit_handler()
        print("Exiting.")
        sys.exit(0)
    
    def exec(self):
        print("Entering Qt event loop.")
        return super(Application, self).exec()
