#pragma once
#include "LoggingBackend.h"
#include <spdlog/common.h>
#include <boost/assert/source_location.hpp>

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
    void log(spdlog::format_string_t<Args...> fmt, Args &&... args) const {
        if (!m_logger) { throw std::runtime_error("Uninitialized, ensure you have called log_init()." ); }
        if (Level >= LevelFilter) {
            m_logger->log(Level, fmt, std::forward<Args>(args)...);
        }
    }

    template<typename Exception, typename ...Args>
    void throw_error(spdlog::format_string_t<Args...> fmt, Args &&... args) const {
        if (!m_logger) { throw std::runtime_error("Uninitialized, ensure you have called log_init()." ); }
        log<logging::LogLevel::err>(fmt, std::forward<Args>(args)...);
        throw Exception("Error");
    }

    void log_trace(const char* fn = BOOST_CURRENT_LOCATION.function_name(),
                   const char* fl = BOOST_CURRENT_LOCATION.file_name(),
                   uint32_t ln = BOOST_CURRENT_LOCATION.line()) const {
        if (!m_logger) { throw std::runtime_error("Uninitialized, ensure you have called log_init()." ); }
        if (logging::LogLevel::trace >= LevelFilter) {
            auto _t = fmt::ptr((void*)this);
            log<logging::LogLevel::trace>("[@{}] {}:{} - {}", _t, fl, ln, fn);
        }
    }

    LoggingEnabled() { };
    virtual ~LoggingEnabled() {}
};

using ModuleLoggingEnabled = LoggingEnabled<logging::CompileTimeLogLevel>;