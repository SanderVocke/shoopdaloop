#pragma once
#include "LoggingBackend.h"
#include <boost/assert/source_location.hpp>
#include <string_view>

template<logging::ModuleName Name, shoop_log_level_t LevelFilter>
class LoggingEnabled {
public:
    template<shoop_log_level_t Level, typename First, typename... Args>
    void log(fmt::format_string<First, Args...>&& fmt, First && first, Args &&... args) const
    {
        if constexpr (Level >= LevelFilter) {
            logging::log<Name, Level>(std::nullopt, std::nullopt, "[@{}] {}", (void*)this, fmt::format(fmt, std::forward<First>(first), std::forward<Args>(args)...));
        }
    }

    template<shoop_log_level_t Level>
    void log(std::string const& str) const {
        if constexpr (Level >= LevelFilter) {
            logging::log<Name, Level>(std::nullopt, std::nullopt, "[@{}] {}", (void*)this, str);
        }
    }

    template<typename Exception, typename First, typename... Args>
    void throw_error(fmt::format_string<First, Args...>&& fmt, First && first, Args &&... args) const
    {
        logging::log<Name, log_error>(std::nullopt, std::nullopt, "[@{}] {}", (void*)this, fmt::format(fmt, std::forward<First>(first), std::forward<Args>(args)...));
        throw Exception("");
    }

    template<typename Exception>
    void throw_error(std::string const& str) const {
        logging::log<Name, log_error>(std::nullopt, std::nullopt, "[@{}] {}", (void*)this, str);
        throw Exception("");
    }

    void log_trace(const char* fn = BOOST_CURRENT_LOCATION.function_name(),
                   const char* fl = BOOST_CURRENT_LOCATION.file_name(),
                   uint32_t ln = BOOST_CURRENT_LOCATION.line()) const
    {
        if (log_trace >= LevelFilter) {
            log<log_trace>("{}:{} - {}", fl, ln, fn);
        }
    }

    LoggingEnabled() { };
    virtual ~LoggingEnabled() {}
};

template<logging::ModuleName Name>
using ModuleLoggingEnabled = LoggingEnabled<Name, logging::CompileTimeLogLevel>;