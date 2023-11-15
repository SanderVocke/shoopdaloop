import sys
import os
import signal
import time

from PySide6.QtQml import QQmlApplicationEngine, QJSValue
from PySide6.QtGui import QGuiApplication, QIcon
from PySide6.QtCore import QTimer, QObject, Q_ARG, QMetaObject, Qt, QEvent, Slot, QtMsgType, Signal
from PySide6.QtQml import QQmlDebuggingEnabler

have_nsm = os.name == 'posix'
if have_nsm:
    from ...third_party.pynsm.nsmclient import NSMClient, NSMNotRunningError

from ..qml_helpers import *

from .SchemaValidator import SchemaValidator
from .FileIO import FileIO
from .ClickTrackGenerator import ClickTrackGenerator
from .KeyModifiers import KeyModifiers
from .ApplicationMetadata import ApplicationMetadata

from ..logging import *

script_dir = os.path.dirname(__file__)

class Application(QGuiApplication):
    exit_handler_called = Signal()

    def __init__(self,
                 title,
                 main_qml,
                 global_args = dict(),
                 additional_root_context = dict(),
                 qml_debug_port=None,
                 qml_debug_wait=False,
                 nsm=False
                 ):
        super(Application, self).__init__([])
        
        pkg_version = None
        with open(script_dir + '/../../version.txt', 'r') as f:
            pkg_version = f.read().strip()
        
        self.setApplicationName('ShoopDaLoop')
        self.setApplicationVersion(pkg_version)
        self.setOrganizationName('ShoopDaLoop')

        self.logger = Logger("Frontend.Qml.App")

        self.nsm_client = None
        self.title = title
        signal.signal(signal.SIGINT, self.exit_signal_handler)
        signal.signal(signal.SIGQUIT, self.exit_signal_handler)
        signal.signal(signal.SIGTERM, self.exit_signal_handler)
        
        self.nsmtimer = QTimer(parent=self)
        self.nsmtimer.start(100)
        self.nsmtimer.timeout.connect(self.tick)

        if qml_debug_port:
            mode = QQmlDebuggingEnabler.StartMode.WaitForClient if qml_debug_wait else QQmlDebuggingEnabler.StartMode.DoNotWaitForClient
            self.logger.info(lambda: "Enabling QML debugging on port {}. Wait on connection: {}.".format(qml_debug_port, qml_debug_wait))
            dbg = QQmlDebuggingEnabler(True)
            QQmlDebuggingEnabler.startTcpDebugServer(qml_debug_port)

        register_shoopdaloop_qml_classes()
        self.global_args = global_args
        self.additional_root_context = additional_root_context
        self.additional_root_context['application'] = self
        
        self.installEventFilter(self)
        self.engine = None
        
        if main_qml:
            self.reload(main_qml)
        
            def start_nsm():
                if have_nsm:
                    try:
                        self.nsm_client = NSMClient(
                            prettyName = title,
                            supportsSaveStatus = False,
                            saveCallback = lambda path, session, client: self.save_session_handler(path, session, client),
                            openOrNewCallback = lambda path, session, client: self.load_session_handler(path, session, client),
                            exitProgramCallback = lambda path, session, client: self.nsm_exit_handler(),
                            loggingLevel = 'info'
                        )
                        self.title = self.nsm_client.ourClientNameUnderNSM
                    except NSMNotRunningError as e:
                        pass
        
            if nsm:
                if len(self.engine.rootObjects()) > 0:
                    self.engine.rootObjects()[0].sceneGraphInitialized.connect(start_nsm)
    
    def reload(self, filename, quit_on_quit=True):
        if self.engine:
            self.engine.deleteLater()
            self.engine = None
            
        self.engine = QQmlApplicationEngine(parent=self)
        
        if quit_on_quit:
            self.engine.quit.connect(self.quit)

        self.engine.setOutputWarningsToStandardError(False)
        self.engine.objectCreated.connect(self.onQmlObjectCreated)
        self.engine.warnings.connect(self.onQmlWarnings)
        
        self.root_context_items = create_and_populate_root_context(self.engine, self.global_args, self.additional_root_context)
        
        self.engine.load(filename)
    
    def exit(self, retcode):
        if self.nsm_client:
            self.logger.debug(lambda: "Requesting exit from NSM.")
            self.nsm_client.serverSendExitToSelf()
        else:
            self.logger.debug(lambda: "Exiting.")
            self.really_exit()

    def really_exit(self, retcode):
        super(Application, self).exit(retcode)
    
    # Ensure that we forward any terminating signals to our child
    # processes
    def exit_signal_handler(self, sig, frame):
        self.logger.debug(lambda: 'Got signal {}.'.format(sig))    
        self.logger.info(lambda: 'Exiting due to signal.')
        self.exit_handler()
    
    def exit_handler(self):
        if self.engine:
            self.logger.warning(lambda: "EXIT HANDLER")
            QMetaObject.invokeMethod(self.engine, 'quit')
            self.exit_handler_called.emit()
    
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
            self.logger.info(lambda: "Requesting exit from NSM.")
            self.nsm_client.serverSendExitToSelf()
        else:
            self.logger.info(lambda: "Exiting.")
            self.really_exit()

    def really_exit(self, retcode):
        super(Application, self).exit(retcode)
    
    def nsm_save_session(self, path):
        self.logger.info(lambda: "NSM: save session {}".format(path))
        session = self.engine.rootObjects()[0].findChild(QObject, 'session')
        if session:
            QMetaObject.invokeMethod(session, "save_session", Qt.ConnectionType.DirectConnection, Q_ARG('QVariant', path))
            while session.property("saving"):
                time.sleep(0.01)
                self.processEvents()
        else:
            self.logger.error(lambda: "No active session object found, ignoring save session.")
        self.logger.info(lambda: "NSM: save session finished.")
    
    def nsm_load_session(self, path):
        self.logger.info(lambda: "NSM: load session {}".format(path))
        session = self.engine.rootObjects()[0].findChild(QObject, 'session')
        if session:
            QMetaObject.invokeMethod(session, "load_session", Qt.ConnectionType.DirectConnection, Q_ARG('QVariant', path))
            while session.property("loading"):
                time.sleep(0.01)
                self.processEvents()
        else:
            self.logger.error(lambda: "No active session object found, ignoring load session.")
        self.logger.info(lambda: "NSM: load session finished.")

    def save_session_handler(self, path, session, client):
        self.logger.debug(lambda: 'save_session_handler')
        self.nsm_save_session(path)

    def load_session_handler(self, path, session, client):
        self.logger.debug(lambda: 'load_session_handler')
        if os.path.isfile(path):
            self.logger.info(lambda: "NSM: load session {}".format(path))
            self.nsm_load_session(path)
        else:
            self.logger.info(lambda: "NSM: create session {}".format(path))
            self.nsm_save_session(path)
    
    def nsm_exit_handler(self):
        self.logger.info(lambda: 'Exiting due to NSM request.')
        self.exit_handler()
        self.logger.info(lambda: "Exiting.")
        sys.exit(0)
    
    def exec(self):
        return super(Application, self).exec()
    
    def eventFilter(self, source, event):
        if event.type() in [QEvent.KeyPress, QEvent.KeyRelease]:
            self.root_context_items['key_modifiers'].process(event)
        return False
    
    @Slot('QVariant', 'QVariant')
    def onQmlObjectCreated(self, obj, url):
        if obj:
            self.logger.debug(lambda: "Created QML object: {}".format(url))
        else:
            self.logger.error(lambda: "Failed to create QML object: {}".format(url))
    
    @Slot('QVariant')
    def onQmlWarnings(self, warnings):            
        for warning in warnings:
            msgtype = warning.messageType()
            msg = warning.toString()
            if msgtype in [QtMsgType.QtDebugMsg]:
                self.logger.debug(lambda: msg)
            elif msgtype in [QtMsgType.QtWarningMsg]:
                self.logger.warning(lambda: msg)
            elif msgtype in [QtMsgType.QtInfoMsg, QtMsgType.QtSystemMsg]:
                self.logger.info(lambda: msg)
            else:
                self.logger.error(lambda: msg)
    
    @Slot(int)
    def wait(self, ms):
        end = time.time() + ms * 0.001
        while time.time() < end:
            self.processEvents()
