#include <optional>
#include <spdlog/logger.h>
#include <spdlog/spdlog.h>
#include "spdlog/sinks/stdout_color_sinks.h"

namespace logging {

using logger = spdlog::logger;
using LogLevel = spdlog::level::level_enum;

#define LOG_LEVEL_TRACE logging::LogLevel::trace;
#define LOG_LEVEL_DEBUG logging::LogLevel::debug;
#define LOG_LEVEL_INFO logging::LogLevel::info;
#define LOG_LEVEL_WARNING logging::LogLevel::warn;
#define LOG_LEVEL_ERROR logging::LogLevel::err;

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