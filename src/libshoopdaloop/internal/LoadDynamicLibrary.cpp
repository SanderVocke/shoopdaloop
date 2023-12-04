#include "LoadDynamicLibrary.h"
#include "LoggingBackend.h"
#include <stdexcept>
#include <fmt/core.h>

_dylib_handle load_dylib(const char* identifier) {
    logging::log<"Backend.LoadDynamicLibrary", log_debug>(std::nullopt, std::nullopt, "Loading {}", identifier);
#ifdef _WIN32
    return LoadLibrary(identifier);
#else
    return dlopen(identifier, RTLD_LAZY);
#endif
}

void throw_if_dylib_error() {
#ifdef _WIN32
    DWORD error = GetLastError();
    if (error) {
        LPVOID msg;
        FormatMessage(
            FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            NULL,
            error,
            MAKELANGID(LANG_NEUTRAL, SUBLANG_DEFAULT),
            (LPSTR)&msg,
            0,
            NULL
        );
        auto _msg = fmt::format("Could not load shared library: code {}, msg {}", error, (LPSTR)msg);
        LocalFree(msg);
        throw std::runtime_error(_msg);
    }
    SetLastError(0);
#else
    auto error = dlerror();
    if (error != NULL) {
        auto _msg = fmt::format("Could not load JACK: code {}", error);
        throw std::runtime_error(_msg);
    }
#endif  
}

void* get_dylib_fn(_dylib_handle handle, const char* symbol_name) {
    logging::log<"Backend.LoadDynamicLibrary", log_trace>(std::nullopt, std::nullopt, "Loading symbol {}", symbol_name);
#ifdef _WIN32
    return (void*) GetProcAddress (handle, symbol_name);
#else
    return dlsym(handle, symbol_name);
#endif
}