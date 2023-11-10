#pragma once

#ifdef _WIN32
    #define SHOOP_EXPORT __declspec(dllexport)
#else
    #define SHOOP_EXPORT __attribute__((visibility("default")))
#endif