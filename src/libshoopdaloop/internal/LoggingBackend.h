#pragma once
#include <mutex>
#include <string_view>
#include <optional>
#include <memory>
#include <atomic>
#include <map>
#include <iostream>
#include "types.h"
#include <algorithm>
#include <array>
#include <fmt/core.h>
#include <utility>

#ifdef __INTELLISENSE__
  #pragma diag_suppress 441
  #pragma diag_suppress 304
#endif

#define LOG_LEVEL_TRACE log_trace;
#define LOG_LEVEL_DEBUG log_debug;
#define LOG_LEVEL_INFO log_info;
#define LOG_LEVEL_WARNING log_warning;
#define LOG_LEVEL_ERROR log_error;

#ifndef COMPILE_LOG_LEVEL
#define COMPILE_LOG_LEVEL log_debug
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

auto constexpr CompileTimeLogLevel = COMPILE_LOG_LEVEL;

extern std::recursive_mutex* g_log_mutex;
extern std::unique_ptr<shoop_log_level_t>* g_maybe_global_level;
extern std::map<std::string, std::unique_ptr<shoop_log_level_t>>* g_module_log_levels;
extern std::atomic<bool> g_log_initialized;

// Configure using a configure string.
// String format example:
//
// log_info,MidiChannel=log_trace
//
// The string should be a comma-separated set of arguments.
// Per argument:
// - if there is no =, sets the global logging level.
// - if there is a =, overrides the logging level for a particular module.
//
// Logging levels:
// log_trace, log_debug, log_info, log_warning, log_error
//
// Modules are registered by the code doing the logging, but the Logging
// module has log_debug statements by the logging framework itself. For example:
// each logger that is registered produces a log_debug message.
void parse_conf_string(std::string s);

// Parse configure string from the SHOOP_LOG env variable.
// Does nothing if this was already done.
void parse_conf_from_env();

namespace internal {

constexpr std::array<const char*, log_error+1> level_indicators = {
    "[log_trace] ",
    "[\033[36mdebug\033[0m] ", // cyan
    "[\033[32minfo\033[0m] ",  // green
    "[\033[33mwarning\033[0m] ",  // yellow
    "[\033[31merror\033[0m] ",  // red
};               

template<typename Name, typename Level>
bool should_log_impl(Name name, Level level) {
    std::lock_guard<std::recursive_mutex> lock((*g_log_mutex));
    auto compile_time_allows = (level >= logging::CompileTimeLogLevel);
    auto maybe_module_level = (*g_module_log_levels).find(std::string(name));
    auto module_level_set = maybe_module_level != (*g_module_log_levels).end();
    auto global_allows =
        (!(*g_maybe_global_level)) || (level >= *(*g_maybe_global_level).get());
    auto module_allows =
            (!module_level_set) ||
            (level >= *maybe_module_level->second.get());
    
    return (module_level_set ? module_allows : global_allows) && compile_time_allows;
}

inline bool should_log(std::string module_name, shoop_log_level_t level) {
    return should_log_impl(module_name, level);
}

template<shoop_log_level_t level>
inline bool should_log(std::string module_name) {
    constexpr shoop_log_level_t _level = level;
    return should_log_impl(module_name, _level);
}

template<ModuleName Name>
inline bool should_log(shoop_log_level_t level) {
    constexpr std::string_view name = Name.value;
    return should_log_impl(name, level);
}

template<ModuleName Name, shoop_log_level_t level>
inline bool should_log() {
    constexpr std::string_view name = Name.value;
    constexpr shoop_log_level_t _level = level;
    return should_log_impl(name, _level);
}

} // namespace internal

inline bool should_log(std::string module_name, shoop_log_level_t level) {
    return internal::should_log(module_name, level);
}

// NEW
template<bool UseCompileTimeModuleName,
         bool UseCompileTimeLevel,
         ModuleName MaybeName,
         shoop_log_level_t MaybeLevel>
void log_impl(std::optional<shoop_log_level_t> maybe_log_level,
              std::optional<std::string_view> maybe_module_name,
              std::string_view str)
{
    auto _log_level = maybe_log_level.value_or(log_info);
    parse_conf_from_env();
    if(UseCompileTimeLevel && UseCompileTimeModuleName && !internal::should_log<MaybeName, MaybeLevel>()) { return; }
    if(UseCompileTimeLevel && !UseCompileTimeModuleName && !internal::should_log<MaybeLevel>(std::string(*maybe_module_name))) { return; }
    if(!UseCompileTimeLevel && UseCompileTimeModuleName && !internal::should_log<MaybeName>(_log_level)) { return; }
    if(!UseCompileTimeLevel && !UseCompileTimeModuleName && !internal::should_log(std::string(*maybe_module_name), _log_level)) { return; }

    if(UseCompileTimeModuleName) {
        std::cout << "[\033[35m" << MaybeName.value << "\033[0m] ";
    } else {
        std::cout << "[\033[35m" << *maybe_module_name << "\033[0m] ";
    }
    if(UseCompileTimeLevel) {
        std::cout << internal::level_indicators[MaybeLevel];
    } else {
        std::cout << internal::level_indicators[*maybe_log_level];
    }

    std::cout << str << std::endl;
}

inline void log(std::optional<shoop_log_level_t> maybe_log_level,
         std::optional<std::string_view> maybe_module_name,
         std::string_view &&str)
{
    log_impl<false, false, "", log_info>(maybe_log_level, maybe_module_name, str);
}
template<typename FirstFormatArg, typename ...RestFormatArgs>
void log(std::optional<shoop_log_level_t> maybe_log_level,
         std::optional<std::string_view> maybe_module_name,
         fmt::format_string<FirstFormatArg, RestFormatArgs...>&& str,
         FirstFormatArg &&first,
         RestFormatArgs &&... rest)
{
    log_impl<false, false, "", log_info>(maybe_log_level, maybe_module_name, fmt::format(str, std::forward<FirstFormatArg>(first), std::forward<RestFormatArgs>(rest)...));
}
template<ModuleName Name>
void log(std::optional<shoop_log_level_t> maybe_log_level,
         std::optional<std::string_view> maybe_module_name,
         std::string_view &&str)
{
    log_impl<true, false, Name, log_info>(maybe_log_level, maybe_module_name, str);
}
template<ModuleName Name, typename FirstFormatArg, typename ...RestFormatArgs>
void log(std::optional<shoop_log_level_t> maybe_log_level,
         std::optional<std::string_view> maybe_module_name,
         fmt::format_string<FirstFormatArg, RestFormatArgs...>&& str,
         FirstFormatArg &&first,
         RestFormatArgs &&... rest)
{
    log_impl<true, false, Name, log_info>(maybe_log_level, maybe_module_name, fmt::format(str, std::forward<FirstFormatArg>(first), std::forward<RestFormatArgs>(rest)...));
}
template<shoop_log_level_t Level>
void log(std::optional<shoop_log_level_t> maybe_log_level,
         std::optional<std::string_view> maybe_module_name,
         std::string_view&& str)
{
    log_impl<false, true, "", Level>(maybe_log_level, maybe_module_name, str);
}
template<shoop_log_level_t Level, typename FirstFormatArg, typename ...RestFormatArgs>
void log(std::optional<shoop_log_level_t> maybe_log_level,
         std::optional<std::string_view> maybe_module_name,
         fmt::format_string<FirstFormatArg, RestFormatArgs...>&& str,
         FirstFormatArg &&first,
         RestFormatArgs &&... rest)
{
    log_impl<false, true, "", Level>(maybe_log_level, maybe_module_name, fmt::format(str, std::forward<FirstFormatArg>(first), std::forward<RestFormatArgs>(rest)...));
}
template<ModuleName Name, shoop_log_level_t Level>
void log(std::optional<shoop_log_level_t> maybe_log_level,
         std::optional<std::string_view> maybe_module_name,
         std::string_view&& str)
{
    log_impl<true, true, Name, Level>(maybe_log_level, maybe_module_name, str);
}
template<ModuleName Name, shoop_log_level_t Level, typename FirstFormatArg, typename ...RestFormatArgs>
void log(std::optional<shoop_log_level_t> maybe_log_level,
         std::optional<std::string_view> maybe_module_name,
         fmt::format_string<FirstFormatArg, RestFormatArgs...>&& str,
         FirstFormatArg &&first,
         RestFormatArgs &&... rest)
{
    log_impl<true, true, Name, Level>(maybe_log_level, maybe_module_name, fmt::format(str, std::forward<FirstFormatArg>(first), std::forward<RestFormatArgs>(rest)...));
}

// Set the global runtime filter level.
// Does not override already set module filter levels.
inline void set_filter_level(shoop_log_level_t level) {
    std::lock_guard<std::recursive_mutex> lock((*g_log_mutex));
    if (!(*g_maybe_global_level)) { (*g_maybe_global_level) = std::make_unique<shoop_log_level_t>(level); }
    else { *(*g_maybe_global_level) = level; }
    log<"Backend.Logging", log_debug>(std::nullopt, std::nullopt, "Set global filter level to {}", (int)level);
}

// Set a module filter level.
// Always overrides the global level - reset by passing nullopt.
inline void set_module_filter_level(std::string name, std::optional<shoop_log_level_t> level) {
    std::lock_guard<std::recursive_mutex> lock((*g_log_mutex));
    auto &maybe_level = (*g_module_log_levels)[name];
    if (!maybe_level) { maybe_level = std::make_unique<shoop_log_level_t>(level.value()); }
    else { *maybe_level = level.value(); }
    log<"Backend.Logging", log_debug>(std::nullopt, std::nullopt, "Set module filter level for {} to {}", name, (int)level.value());
}

}