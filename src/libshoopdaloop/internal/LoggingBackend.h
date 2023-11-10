#pragma once
#include <mutex>
#include <string_view>
#include <optional>
#include <memory>
#include <atomic>
#include <map>
#include "stream_format.h"
#include "types.h"
#include <algorithm>

#define LOG_LEVEL_TRACE trace;
#define LOG_LEVEL_DEBUG debug;
#define LOG_LEVEL_INFO info;
#define LOG_LEVEL_WARNING warning;
#define LOG_LEVEL_ERROR error;

#ifndef COMPILE_LOG_LEVEL
#define COMPILE_LOG_LEVEL info
#else

// #define HELPER(x) #x
// #define STR(x) HELPER(x)
// #pragma message "Using externally defined log level " STR(COMPILE_LOG_LEVEL)

#endif
namespace logging {

template<size_t N>
struct ModuleName {
    constexpr ModuleName(const char (&str)[N]) {
        std::copy_n(str, N, value);
    }
    char value[N];
};

auto constexpr CompileTimelog_level_t = COMPILE_LOG_LEVEL;

extern std::recursive_mutex g_log_mutex;
extern std::unique_ptr<log_level_t> g_maybe_global_level;
extern std::map<std::string, std::unique_ptr<log_level_t>> g_module_log_levels;

namespace internal {
template<typename ...Args>
void do_log(std::string_view module_name, Args &&... args) {
    std::cout << "[" << module_name << "] ";
    stream_format(std::cout, args...);
    std::cout << std::endl;
}
}

// Passing module name as an argument works, but is slower than using a template
// argument because a map lookup is needed.
template<log_level_t level, typename ...Args>
void log(std::string module_name, std::string_view str, Args &&... args) {
    if constexpr (level >= logging::CompileTimelog_level_t) {
        std::lock_guard<std::recursive_mutex> lock(g_log_mutex);
        auto maybe_module_level = g_module_log_levels.find(module_name);
        if ( (maybe_module_level != g_module_log_levels.end() && level >= *maybe_module_level->second.get()) ||
            (g_maybe_global_level && level >= *g_maybe_global_level.get())) {
            internal::do_log(module_name, str, args...);
        }
    }
}

template<ModuleName Name, log_level_t level, typename ...Args>
void log(std::string_view str, Args &&... args) {
    static log_level_t* _level = nullptr;
    if constexpr (level >= logging::CompileTimelog_level_t) {
        std::lock_guard<std::recursive_mutex> lock(g_log_mutex);
        if (!_level) {
            auto maybe_module_level = g_module_log_levels.find(std::string(Name.value));
            if (maybe_module_level != g_module_log_levels.end()) {
                _level = maybe_module_level->second.get();
            }
        }
        if ( (_level && level >= *_level) ||
            (g_maybe_global_level && level >= *g_maybe_global_level.get())) {
            internal::do_log(Name.value, str, args...);
        }
    }
}

// Set the global runtime filter level.
// Does not override already set module filter levels.
inline void set_filter_level(log_level_t level) {
    std::lock_guard<std::recursive_mutex> lock(g_log_mutex);
    if (!g_maybe_global_level) { g_maybe_global_level = std::make_unique<log_level_t>(level); }
    else { *g_maybe_global_level = level; }
}

// Set a module filter level.
// Always overrides the global level - reset by passing nullopt.
inline void set_module_filter_level(const char* name, std::optional<log_level_t> level) {
    std::lock_guard<std::recursive_mutex> lock(g_log_mutex);
    auto &maybe_level = g_module_log_levels[name];
    if (!maybe_level) { maybe_level = std::make_unique<log_level_t>(level.value()); }
    else { *maybe_level = level.value(); }
}

// Configure using a configure string.
// String format example:
//
// info,MidiChannel=trace
//
// The string should be a comma-separated set of arguments.
// Per argument:
// - if there is no =, sets the global logging level.
// - if there is a =, overrides the logging level for a particular module.
//
// Logging levels:
// trace, debug, info, warning, error
//
// Modules are registered by the code doing the logging, but the Logging
// module has debug statements by the logging framework itself. For example:
// each logger that is registered produces a debug message.
void parse_conf_string(std::string s);

// Parse configure string from the SHOOP_LOG env variable.
void parse_conf_from_env();

}