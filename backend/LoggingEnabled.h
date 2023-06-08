#pragma once
#include "LoggingBackend.h"
#include <spdlog/common.h>
#include <spdlog/spdlog.h>
#include <fmt/core.h>
#include <source_location>

#ifndef MODULE_LOG_LEVEL
#define MODULE_LOG_LEVEL logging::LogLevel::debug
#else
#define HELPER(x) #x
#define STR(x) HELPER(x)
#pragma message "Using externally defined log level " STR(MODULE_LOG_LEVEL)
#endif

auto constexpr ModuleLogLevel = MODULE_LOG_LEVEL;

template<logging::LogLevel LevelFilter>
class LoggingEnabled {
private:
    spdlog::logger *m_logger = nullptr;

    virtual std::string log_module_name() const = 0;

protected:
    void log_init() {
        m_logger = &logging::get_logger(std::string(log_module_name()));
    }

    template<logging::LogLevel Level, typename... Args>
    void log(fmt::format_string<Args...> fmt, Args &&... args) const {
        if (Level >= LevelFilter) {
            m_logger->log(Level, fmt, std::forward<Args>(args)...);
        }
    }

    template<typename Exception, typename ...Args>
    void throw_error(fmt::format_string<Args...> fmt, Args &&... args, spdlog::source_loc loc=spdlog::source_loc{}) const {
        log<logging::LogLevel::err>(fmt, std::forward<Args>(args)...);
        throw Exception(loc.filename);
    }

    void log_trace(std::source_location loc=std::source_location::current()) const {
        log<logging::LogLevel::trace>("{} @ {}:{}", loc.function_name(), loc.file_name(), loc.line());
    }

public:
    LoggingEnabled() {};
    virtual ~LoggingEnabled() {}
};

using ModuleLoggingEnabled = LoggingEnabled<ModuleLogLevel>;