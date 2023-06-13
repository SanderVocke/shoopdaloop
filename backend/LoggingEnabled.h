#pragma once
#include "LoggingBackend.h"
#include <spdlog/common.h>
#include <spdlog/spdlog.h>
#include <fmt/core.h>
#include <source_location>
#include <string_view>

template<logging::LogLevel LevelFilter>
class LoggingEnabled {
private:
    logging::logger *m_logger = nullptr;

    virtual std::string log_module_name() const = 0;

protected:
    void log_init() {
        m_logger = &logging::get_logger(std::string(log_module_name()));
    }

public:
    template<logging::LogLevel Level, typename... Args>
    void log(fmt::format_string<Args...> fmt, Args &&... args) const {
        if (Level >= LevelFilter) {
            m_logger->log(Level, fmt, std::forward<Args>(args)...);
        }
    }

    template<typename Exception, typename ...Args>
    void throw_error(fmt::format_string<Args...> fmt, Args &&... args) const {
        log<logging::LogLevel::err>(fmt, std::forward<Args>(args)...);
        throw Exception("Error");
    }

    void log_trace(const char* fn = std::source_location::current().function_name(),
                   const char* fl = std::source_location::current().file_name(),
                   uint32_t ln = std::source_location::current().line()) const {
        log<logging::LogLevel::trace>("{}:{} - {}", fl, ln, fn);
    }

    LoggingEnabled() {};
    virtual ~LoggingEnabled() {}
};

using ModuleLoggingEnabled = LoggingEnabled<logging::CompileTimeLogLevel>;