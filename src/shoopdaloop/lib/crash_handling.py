import tempfile
import faulthandler
import ctypes

def crash_dumped_cb(path):
    print("Hello world: {}".format(path))
c_crash_dumped_cb = None

def init_crash_handling():
    td = tempfile.gettempdir()
    try:
        import shoop_crashhandling
        c_crash_dumped_cb = shoop_crashhandling.CrashedCallback(crash_dumped_cb)
        shoop_crashhandling.shoop_init_crashhandling(td, c_crash_dumped_cb)
    except Exception as e:
        from .logging import Logger
        logger = Logger('Frontend.CrashHandling')
        logger.warning("Unable to load back-end extensions for crash handling. Waveforms will not be visible. Error: {}".format(e))
        return
    from .logging import Logger
    logger = Logger('Frontend.CrashHandling')
    logger.debug("Initialized Breakpad crash handler @ {}.".format(td))
    if(faulthandler.is_enabled()):
        logger.warning("Python built-in fault handling is enabled. This may prevent generation of dump files from back-end faults.")

def test_exception():
    import shoop_crashhandling
    shoop_crashhandling.shoop_test_crash_exception()

def test_segfault():
    import shoop_crashhandling
    shoop_crashhandling.shoop_test_crash_segfault()

def test_abort():
    import shoop_crashhandling
    shoop_crashhandling.shoop_test_crash_abort()