#pragma once
#include "LoggingBackend.h"
#include <boost/assert/source_location.hpp>
#include <string_view>

template<logging::ModuleName Name, log_level_t LevelFilter>
class LoggingEnabled {
public:
    template<log_level_t Level, typename... Args>
    void log(std::string_view fmt, Args &&... args) const
    {
        if constexpr (Level >= LevelFilter) {
            logging::log_with_ptr<Name, Level>(this, fmt, args...);
        }
    }

    template<typename Exception, typename ...Args>
    void throw_error(std::string_view fmt, Args &&... args) const
    {
        log<error>(fmt, std::forward<Args>(args)...);
        throw Exception("");
    }

    void log_trace(const char* fn = BOOST_CURRENT_LOCATION.function_name(),
                   const char* fl = BOOST_CURRENT_LOCATION.file_name(),
                   uint32_t ln = BOOST_CURRENT_LOCATION.line()) const
    {
        if (trace >= LevelFilter) {
            log<trace>( "{}:{} - {}", fl, ln, fn);
        }
    }

    LoggingEnabled() { };
    virtual ~LoggingEnabled() {}
};

template<logging::ModuleName Name>
using ModuleLoggingEnabled = LoggingEnabled<Name, logging::CompileTimelog_level_t>;