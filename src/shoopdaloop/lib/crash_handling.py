import tempfile
import faulthandler
import ctypes
import os

from shoopdaloop.lib.init_dynlibs import init_dynlibs
init_dynlibs()

def init_crash_handling():
    td = os.environ.get("SHOOP_CRASH_DUMP_DIR")
    if not td:
        td = tempfile.gettempdir()
    try:
        import shoopdaloop.shoop_crashhandling
        shoopdaloop.shoop_crashhandling.shoop_init_crashhandling(td)
    except Exception as e:
        from shoopdaloop.lib.logging import Logger
        logger = Logger('Frontend.CrashHandling')
        logger.warning("Unable to load back-end extensions for crash handling. Waveforms will not be visible. Error: {}".format(e))
        return
    from shoopdaloop.lib.logging import Logger
    logger = Logger('Frontend.CrashHandling')
    logger.debug("Initialized Breakpad crash handler @ {}.".format(td))
    if(faulthandler.is_enabled()):
        logger.warning("Python built-in fault handling is enabled. This may prevent generation of dump files from back-end faults.")

def test_exception():
    import shoopdaloop.shoop_crashhandling
    shoopdaloop.shoop_crashhandling.shoop_test_crash_exception()

def test_segfault():
    import shoopdaloop.shoop_crashhandling
    shoopdaloop.shoop_crashhandling.shoop_test_crash_segfault()

def test_abort():
    import shoopdaloop.shoop_crashhandling
    shoopdaloop.shoop_crashhandling.shoop_test_crash_abort()