import sys
import os
import signal
import time

from PySide6.QtQml import QQmlApplicationEngine
from PySide6.QtCore import QTimer, QObject, Q_ARG, QMetaObject, Qt, QEvent, Slot, QtMsgType, Signal, SIGNAL, SLOT
from PySide6.QtGui import QIcon
from PySide6.QtQml import QQmlDebuggingEnabler
from PySide6.QtQuick import QQuickWindow

from .ShoopPyObject import *

have_nsm = os.name == 'posix'
if have_nsm:
    from ...third_party.pynsm.nsmclient import NSMClient, NSMNotRunningError

from ..qml_helpers import *
from shoop_rust import shoop_rust_init, shoop_rust_make_qml_application_engine, shoop_rust_set_crash_json_tag

from ..logging import *
from shoop_config import shoop_version, shoop_resource_dir

from ..engine_update_thread import get_engine_update_thread_wrapper

from shiboken6 import Shiboken, getCppPointer

class Application(ShoopQApplication):
    exit_handler_called = ShoopSignal()

    def __init__(self,
                 title,
                 main_qml,
                 global_args = dict(),
                 additional_root_context = dict(),
                 qml_debug_port=None,
                 qml_debug_wait=False,
                 nsm=False,
                 refresh_backend_on_frontend_refresh=True,
                 backend_backup_refresh_interval_ms=25
                 ):
        super(Application, self).__init__([])

        self._quitting = False
        self.refresh_backend_on_frontend_refresh = refresh_backend_on_frontend_refresh
        self.backend_backup_refresh_interval_ms = backend_backup_refresh_interval_ms

        pkg_version = shoop_version

        self.setApplicationName('ShoopDaLoop')
        self.setApplicationVersion(pkg_version)
        self.setOrganizationName('ShoopDaLoop')

        self.logger = Logger("Frontend.Application")

        self.nsm_client = None
        self.title = title
        signal.signal(signal.SIGINT, self.exit_signal_handler)
        if hasattr(signal, 'SIGQUIT'):
            signal.signal(signal.SIGQUIT, self.exit_signal_handler)
        signal.signal(signal.SIGTERM, self.exit_signal_handler)

        self.nsmtimer = QTimer(parent=self)
        self.nsmtimer.start(100)
        self.nsmtimer.timeout.connect(self.tick)

        if qml_debug_port:
            mode = QQmlDebuggingEnabler.StartMode.WaitForClient if qml_debug_wait else QQmlDebuggingEnabler.StartMode.DoNotWaitForClient
            self.logger.info(lambda: "Enabling QML debugging on port {}. Wait on connection: {}.".format(qml_debug_port, qml_debug_wait))
            dbg = QQmlDebuggingEnabler(True)
            QQmlDebuggingEnabler.startTcpDebugServer(qml_debug_port, mode)

        shoop_rust_init()
        register_shoopdaloop_qml_classes()
        self.global_args = global_args
        self.additional_root_context = additional_root_context
        self.additional_root_context['application'] = self

        self.engine = None

        if main_qml:
            self.reload_qml(main_qml)

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

        self.setWindowIcon(QIcon(os.path.join(shoop_resource_dir, 'iconset', 'icon_128x128.png')))

    def unload_qml(self):
        if self.engine:
            self.logger.debug("Unloading QML.")
            self.engine.collectGarbage()
            for obj in self.engine.rootObjects():
                if isinstance(obj, QQuickWindow):
                    self.logger.debug(f"close on {obj}")
                    obj.close()
                else:
                    self.logger.debug(f"deleteLater on {obj}")
                    obj.deleteLater()
            self.engine.deleteLater()
            self.engine = None

    def load_qml(self, filename, quit_on_quit=True):
        def maybe_install_event_filter(obj):
            if obj and isinstance(obj, QQuickWindow):
                self.logger.debug("Window created, installing event filter")
                obj.installEventFilter(self)

        self_addr = getCppPointer(self)[0]
        eng_addr = shoop_rust_make_qml_application_engine(self_addr)
        self.engine = Shiboken.wrapInstance(eng_addr, QQmlApplicationEngine)

        qml_paths = os.getenv("SHOOP_QML_PATHS")
        qml_paths = qml_paths.split(';' if os.name == "nt" else ":") if qml_paths else []
        for path in qml_paths:
            self.engine.addImportPath(path)

        self.logger.debug(f"QML engine path list: {self.engine.importPathList()}")

        self.engine.destroyed.connect(lambda: self.logger.debug("QML engine being destroyed."))
        self.engine.objectCreated.connect(lambda obj, _: maybe_install_event_filter(obj))

        if quit_on_quit:
            self.engine.quit.connect(self.do_quit)
        self.aboutToQuit.connect(self.do_quit)

        self.engine.setOutputWarningsToStandardError(False)
        self.engine.objectCreated.connect(self.onQmlObjectCreated)
        self.engine.warnings.connect(self.onQmlWarnings)

        self.root_context_items = create_and_populate_root_context(self.engine, self.global_args, self.additional_root_context)

        self.engine.load(filename)

        engine_update_thread_wrapper = get_engine_update_thread_wrapper()
        engine_update_thread_wrapper.setProperty('trigger_update_on_frame_swapped', self.refresh_backend_on_frontend_refresh)
        engine_update_thread_wrapper.setProperty('backup_timer_interval_ms', self.backend_backup_refresh_interval_ms)
        for obj in self.engine.rootObjects():
            if(isinstance(obj, QQuickWindow)):
                # This connection ensure back-end state updates happen in lock-step with
                # GUI refreshes.
                QObject.connect(obj, SIGNAL("frameSwapped()"),
                                engine_update_thread_wrapper, SLOT("frontend_frame_swapped()"),
                                Qt.QueuedConnection)
            else:
                self.logger.warning(lambda: "Couldn't find top-level QQuickWindow to lock back-end refresh to GUI refresh")

    def reload_qml(self, filename, quit_on_quit=True):
        self.unload_qml()
        self.load_qml(filename, quit_on_quit)

    def exit(self):
        if self.nsm_client:
            self.logger.debug(lambda: "Requesting exit from NSM.")
            self.nsm_client.serverSendExitToSelf()
        else:
            self.logger.debug(lambda: "Exiting.")
            self.do_quit()

    # Ensure that we forward any terminating signals to our child
    # processes
    def exit_signal_handler(self, sig, frame):
        self.logger.info(lambda: f'Exiting due to signal {sig}.')
        if sig == signal.SIGTERM or ('SIGQUIT' in dir(signal) and sig == signal.SIGQUIT):
            # on these more "severe" signals, unregister our signal handler.
            # that way when the same signal comes again, the OS can terminate the process
            # more strictly.
            signal.signal(sig, signal.SIG_DFL)
        self.exit_handler()

    def exit_handler(self):
        if self.engine:
            self.logger.debug(lambda: "Quit via exit handler")
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
            self.really_exit(retcode)

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

    @ShoopSlot('QVariant', 'QVariant')
    def onQmlObjectCreated(self, obj, url):
        if obj:
            self.logger.debug(lambda: "Created QML object: {}".format(url))
        else:
            self.logger.error(lambda: "Failed to create QML object: {}".format(url))

    @ShoopSlot('QVariant')
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

    @ShoopSlot(int)
    def wait(self, ms):
        end = time.time() + ms * 0.001
        while time.time() < end:
            self.processEvents()
            self.sendPostedEvents()

    @ShoopSlot()
    def do_quit(self):
        self.logger.debug("Quit requested")
        shoop_rust_set_crash_json_tag("shoop_phase", "quit")
        if not self._quitting:
            self._quitting = True
            if self.engine:
                self.unload_qml()
            else:
                self.logger.debug("Quitting")
                QTimer.singleShot(1, lambda: self.quit())
                self.exec()


