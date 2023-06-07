#include <spdlog/logger.h>
#include <spdlog/spdlog.h>
#include "spdlog/sinks/stdout_color_sinks.h"

using logger = spdlog::logger;

// Get/create a logger.
logger & get_logger(std::string name);