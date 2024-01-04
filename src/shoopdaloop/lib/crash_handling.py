def init_crash_handling():
    try:
        import shoop_crashhandling
        shoop_crashhandling.shoop_init_crashhandling('/tmp')
    except Exception as e:
        from .logging import Logger
        logger = Logger('Frontend.InitCrashHandling')
        logger.warning("Unable to load back-end extensions for crash handling. Waveforms will not be visible. Error: {}".format(e))

def test_exception():
    import shoop_crashhandling
    shoop_crashhandling.shoop_test_crash_exception()

def test_segfault():
    import shoop_crashhandling
    shoop_crashhandling.shoop_test_crash_segfault()

def test_abort():
    import shoop_crashhandling
    shoop_crashhandling.shoop_test_crash_abort()