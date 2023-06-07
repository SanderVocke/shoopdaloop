#include <spdlog/logger.h>
#include <spdlog/spdlog.h>
#include "spdlog/sinks/stdout_color_sinks.h"

using logger = spdlog::logger;
using LogLevel = spdlog::level::level_enum;

#define LOG_LEVEL_TRACE LogLevel::trace;
#define LOG_LEVEL_DEBUG LogLevel::debug;
#define LOG_LEVEL_INFO LogLevel::info;
#define LOG_LEVEL_WARNING LogLevel::warn;
#define LOG_LEVEL_ERROR LogLevel::err;

// Get/create a logger.
logger & get_logger(std::string name);

// Set the global runtime filter level.
void set_filter_level(LogLevel level);