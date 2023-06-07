#pragma once
#include "LoggingBackend.h"
#include <spdlog/common.h>
#include <spdlog/spdlog.h>
#include <fmt/core.h>
#include <source_location>

#ifndef MODULE_LOG_LEVEL
#define MODULE_LOG_LEVEL LogLevel::debug
#else
#warning Using externally defined log level
#endif

auto constexpr ModuleLogLevel = MODULE_LOG_LEVEL;

template<LogLevel LevelFilter>
class LoggingEnabled {
private:
    spdlog::logger *m_logger = nullptr;
    std::string m_logger_name = "";

protected:
    template<LogLevel Level, typename... Args>
    void log(spdlog::source_loc loc, fmt::format_string<Args...> fmt, Args &&... args) {
        if (Level >= LevelFilter) {
            m_logger->log(loc, Level, fmt, std::forward<Args>(args)...);
        }
    }

    template<LogLevel Level, typename... Args>
    void log(fmt::format_string<Args...> fmt, Args &&... args) {
        log<Level, Args...>(spdlog::source_loc{}, fmt, args...);
    }

    template<typename Exception, typename ...Args>
    void throw_error(fmt::format_string<Args...> fmt, Args &&... args, spdlog::source_loc loc=spdlog::source_loc{}) {
        log<LogLevel::err>(fmt, std::forward<Args>(args)...);
        throw Exception(loc.filename);
    }

    void log_trace(spdlog::source_loc loc=spdlog::source_loc{}) {
        log<LogLevel::trace>("{} @ {}:{}", loc.funcname, loc.filename, loc.line);
    }

public:
    LoggingEnabled(std::string name) : m_logger(&get_logger(name)), m_logger_name(name) {
        m_logger->set_pattern("[%H:%M:%S %z] [%n] [%^---%L---%$] [thread %t] %v");
    }
};

using ModuleLoggingEnabled = LoggingEnabled<ModuleLogLevel>;