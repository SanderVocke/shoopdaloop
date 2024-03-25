import tempfile
import faulthandler
import ctypes
import os
import traceback
import threading
import time
import sys

from PySide6.QtQml import QJSValue
from PySide6.QtCore import QThread
from shiboken6 import Shiboken

# Keep track of active javascript engines in order to attempt to get tracebacks from them
g_active_js_engines = set()

from shoopdaloop.lib.init_dynlibs import init_dynlibs
init_dynlibs()

# In the crash callback, attempt to gather additional information about the Python and QML state.
def crashed_callback(filename):
    global g_active_js_engines

    more_info_filename = str(filename + '.moreinfo')
    print("  - Attempting to write supplemental crash info @ {}".format(more_info_filename))

    try:
        command = ' '.join(sys.argv)
        with open(more_info_filename, 'w') as f:
            f.write("=====================================\n")
            f.write("Command\n")
            f.write("=====================================\n\n")
            f.write(command)
            f.write('\n\n')
    except Exception as e:
        pass
    
    try:
        with open(more_info_filename, 'a') as f:
            f.write("=====================================\n")
            f.write("Python traceback\n")
            f.write("=====================================\n\n")
            traceback.print_stack(file=f)
            f.write("\n")
    except Exception as e:
        pass

    try:
        js_stacks = []
        failed_to_get_stack_engs = []
        other_thread_engs = []
        for idx, eng in enumerate(g_active_js_engines):
            if eng and Shiboken.isValid(eng):
                if eng.thread() == QThread.currentThread():
                    stack_str = None
                    # use an error throwing mechanism to get a stack trace from the engine
                    eng.throwError("Crash handler stack retrieval helper error")
                    err = eng.catchError()
                    if err:
                        stack = err.property('stack')
                        if stack:
                            stack_str = stack.toString()
                    if stack_str:
                        js_stacks.append(stack_str)
                    else:
                        failed_to_get_stack_engs.append(eng)
                else:
                    other_thread_engs.append(eng)                
        
        if (len(js_stacks) + len(failed_to_get_stack_engs) + len(other_thread_engs)) > 0:
            with open(more_info_filename, 'a') as f:
                f.write("=====================================\n")
                f.write("Javascript/QML tracebacks\n")
                f.write("=====================================\n\n")
                idx = 1
                for stack in enumerate(js_stacks):
                    f.write(f'Stack for JS engine {idx}:\n{stack}\n\n')
                    idx += 1
                for eng in other_thread_engs:
                    f.write(f'JS engine {idx} is not on the crashing thread - no stack.\n\n')
                    idx += 1
                for eng in failed_to_get_stack_engs:
                    f.write(f'JS engine {idx} is on the crashing thread, but failed to get a stack.\n\n')
    except Exception as e:
        pass
    
    print("  - Done.")
    
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