#pragma once
#include <spdlog/spdlog.h>
#include <string_view>
#include <optional>

#define LOG_LEVEL_TRACE logging::LogLevel::trace;
#define LOG_LEVEL_DEBUG logging::LogLevel::debug;
#define LOG_LEVEL_INFO logging::LogLevel::info;
#define LOG_LEVEL_WARNING logging::LogLevel::warn;
#define LOG_LEVEL_ERROR logging::LogLevel::err;

#ifndef COMPILE_LOG_LEVEL
#define COMPILE_LOG_LEVEL logging::LogLevel::debug
#else

// #define HELPER(x) #x
// #define STR(x) HELPER(x)
// #pragma message "Using externally defined log level " STR(COMPILE_LOG_LEVEL)

#endif


namespace logging {

using LogLevel = spdlog::level::level_enum;
auto constexpr CompileTimeLogLevel = COMPILE_LOG_LEVEL;

struct logger {
    std::shared_ptr<spdlog::logger> m_logger;
    std::recursive_mutex m_mutex;

    logger(std::shared_ptr<spdlog::logger> l) : m_logger(l) {}

    template<typename... Args>
    void log(spdlog::source_loc loc, spdlog::level::level_enum lvl, spdlog::format_string_t<Args...> fmt, Args &&... args) {
        if (lvl >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            m_logger->log(loc, lvl, fmt, args...);
        }
    }

    template<typename... Args>
    void log(spdlog::level::level_enum lvl, spdlog::format_string_t<Args...> fmt, Args &&... args) {
        if (lvl >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            m_logger->log(lvl, fmt, args...);
        }
    }

    template<typename T>
    void log(spdlog::level::level_enum lvl, const T &msg) {
        if (lvl >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            m_logger->log(lvl, msg);
        }
    }

    template<typename T>
    void log_no_filter(spdlog::level::level_enum lvl, const T &msg) {
        std::lock_guard<std::recursive_mutex> guard(m_mutex);
        m_logger->log(lvl, msg);
    }

    bool should_log(spdlog::level::level_enum lvl) {
        if (lvl >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            return m_logger->should_log(lvl);
        }
        return false;
    }

    template<class T, typename std::enable_if<!spdlog::is_convertible_to_any_format_string<const T &>::value, int>::type = 0>
    void log(spdlog::source_loc loc, spdlog::level::level_enum lvl, const T &msg) {
        if (lvl >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            m_logger->log(loc, lvl, msg);
        }
    }

    template<typename... Args>
    void trace(spdlog::format_string_t<Args...> fmt, Args &&... args) {
        if (LogLevel::trace >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            m_logger->trace(fmt, args...);
        }
    }

    template<typename... Args>
    void debug(spdlog::format_string_t<Args...> fmt, Args &&... args) {
        if (LogLevel::debug >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            m_logger->debug(fmt, args...);
        }
    }

    template<typename... Args>
    void info(spdlog::format_string_t<Args...> fmt, Args &&... args) {
        if (LogLevel::info >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            m_logger->info(fmt, args...);
        }
    }

    template<typename... Args>
    void warn(spdlog::format_string_t<Args...> fmt, Args &&... args) {
        if (LogLevel::warn >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            m_logger->warn(fmt, args...);
        }
    }

    template<typename... Args>
    void error(spdlog::format_string_t<Args...> fmt, Args &&... args) {
        if (LogLevel::err >= CompileTimeLogLevel) {
            std::lock_guard<std::recursive_mutex> guard(m_mutex);
            m_logger->error(fmt, args...);
        }
    }

    std::string_view name() const { return m_logger->name(); }
};

// Get/create a logger.
logger & get_logger(std::string name);

// Set the global runtime filter level.
// Does not override already set module filter levels.
void set_filter_level(LogLevel level);

// Set a module filter level.
// Always overrides the global level - reset by passing nullopt.
void set_module_filter_level(std::string name, std::optional<LogLevel> level);

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