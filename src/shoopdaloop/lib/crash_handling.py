import tempfile
import faulthandler
import ctypes
import os
import traceback

from PySide6.QtQml import QJSValue
from shiboken6 import Shiboken

# Keep track of active javascript engines in order to attempt to get tracebacks from them
g_active_js_engines = set()

from shoopdaloop.lib.init_dynlibs import init_dynlibs
init_dynlibs()

# In the crash callback, attempt to gather additional information about the Python and QML state.
def crashed_callback(filename):
    global g_active_js_engines
    
    more_info_filename = str(filename + '.info')
    
    try:
        with open(more_info_filename, 'w') as f:
            f.write("=====================================\n")
            f.write("Python traceback\n")
            f.write("=====================================\n\n")
            traceback.print_stack(file=f)
            f.write("\n")
    except Exception as e:
        pass

    try:
        js_stacks = []
        for idx, eng in enumerate(g_active_js_engines):
            if eng and Shiboken.isValid(eng):
                eng.throwError("Crash handler stack retrieval helper error")
                err = eng.catchError()
                if err:
                    stack = err.property('stack')
                    if stack:
                        stack_str = stack.toString()
                        js_stacks.append(stack_str)
        
        if len(js_stacks) > 0:
            with open(more_info_filename, 'a') as f:
                f.write("=====================================\n")
                f.write("Javascript/QML tracebacks\n")
                f.write("=====================================\n\n")
                for idx, stack in enumerate(js_stacks):
                    f.write(f'JS Stack {idx}:\n{stack}\n\n')
    except Exception as e:
        pass
    
    print("  - Python/QML crash info saved @ {}".format(more_info_filename))
    
c_crashed_callback = None

def init_crash_handling():
    global c_crashed_callback
    td = os.environ.get("SHOOP_CRASH_DUMP_DIR")
    if not td:
        td = tempfile.gettempdir()
    try:
        import shoopdaloop.shoop_crashhandling
        c_crashed_callback = shoopdaloop.shoop_crashhandling.CrashedCallback(crashed_callback)
        shoopdaloop.shoop_crashhandling.shoop_init_crashhandling_with_cb(td, c_crashed_callback)
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

def register_js_engine(eng):
    global g_active_js_engines
    g_active_js_engines.add(eng)