#ifdef _WIN32
    #define SHOOP_EXPORT __declspec(dllexport)
#else
    #define SHOOP_EXPORT __attribute__((visibility("default")))
#endif

SHOOP_EXPORT void install_custom_qt_msg_handler(unsigned abort_on_fatal);