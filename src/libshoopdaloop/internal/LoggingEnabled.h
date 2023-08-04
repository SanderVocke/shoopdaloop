#pragma once
#include "LoggingBackend.h"
#include <source_location>

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
        if (!m_logger) { throw std::runtime_error("Uninitialized, ensure you have called log_init()." ); }
        if (Level >= LevelFilter) {
            m_logger->log(Level, fmt, std::forward<Args>(args)...);
        }
    }

    template<typename Exception, typename ...Args>
    void throw_error(fmt::format_string<Args...> fmt, Args &&... args) const {
        if (!m_logger) { throw std::runtime_error("Uninitialized, ensure you have called log_init()." ); }
        log<logging::LogLevel::err>(fmt, std::forward<Args>(args)...);
        throw Exception("Error");
    }

    void log_trace(const char* fn = std::source_location::current().function_name(),
                   const char* fl = std::source_location::current().file_name(),
                   uint32_t ln = std::source_location::current().line()) const {
        if (!m_logger) { throw std::runtime_error("Uninitialized, ensure you have called log_init()." ); }
        log<logging::LogLevel::trace>("{}:{} - {}", fl, ln, fn);
    }

    LoggingEnabled() { };
    virtual ~LoggingEnabled() {}
};

using ModuleLoggingEnabled = LoggingEnabled<logging::CompileTimeLogLevel>;