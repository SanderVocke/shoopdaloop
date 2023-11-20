#pragma once

#include <optional>

#ifdef _WIN32
    #define NOMINMAX
    #include <windows.h>
    #undef min
    #undef max
    typedef HMODULE _dylib_handle;
#else
    #include <dlfcn.h>
    typedef void* _dylib_handle;
#endif

_dylib_handle load_dylib(const char* identifier);
void throw_if_dylib_error();
void* get_dylib_fn(_dylib_handle handle, const char* symbol_name);