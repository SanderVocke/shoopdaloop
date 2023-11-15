#pragma once
#include "LoggingBackend.h"
#include <boost/assert/source_location.hpp>
#include <string_view>

template<logging::ModuleName Name, log_level_t LevelFilter>
class LoggingEnabled {
public:
    template<log_level_t Level, typename First, typename... Args>
    void log(std::format_string<First, Args...>&& fmt, First && first, Args &&... args) const
    {
        if constexpr (Level >= LevelFilter) {
            logging::log<Name, Level>(std::nullopt, std::nullopt, "[@{}] {}", (void*)this, std::format(fmt, std::forward<First>(first), std::forward<Args>(args)...));
        }
    }

    template<log_level_t Level>
    void log(std::string const& str) const {
        if constexpr (Level >= LevelFilter) {
            logging::log<Name, Level>(std::nullopt, std::nullopt, "[@{}] {}", (void*)this, str);
        }
    }

    template<typename Exception, typename First, typename... Args>
    void throw_error(std::format_string<First, Args...>&& fmt, First && first, Args &&... args) const
    {
        logging::log<Name, error>(std::nullopt, std::nullopt, "[@{}] {}", (void*)this, std::format(fmt, std::forward<First>(first), std::forward<Args>(args)...));
        throw Exception("");
    }

    template<typename Exception>
    void throw_error(std::string const& str) const {
        logging::log<Name, error>(std::nullopt, std::nullopt, "[@{}] {}", (void*)this, str);
        throw Exception("");
    }

    void log_trace(const char* fn = BOOST_CURRENT_LOCATION.function_name(),
                   const char* fl = BOOST_CURRENT_LOCATION.file_name(),
                   uint32_t ln = BOOST_CURRENT_LOCATION.line()) const
    {
        if (trace >= LevelFilter) {
            log<trace>("{}:{} - {}", fl, ln, fn);
        }
    }

    LoggingEnabled() { };
    virtual ~LoggingEnabled() {}
};

template<logging::ModuleName Name>
using ModuleLoggingEnabled = LoggingEnabled<Name, logging::CompileTimeLogLevel>;