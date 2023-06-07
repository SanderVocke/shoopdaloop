#include "LoggingBackend.h"
#include <map>
#include <spdlog/common.h>
#include <spdlog/sinks/stdout_color_sinks.h>
#include <spdlog/logger.h>
#include <mutex>
#include <iostream>
#include <algorithm>

std::map<std::string, std::pair<spdlog::sink_ptr, std::shared_ptr<spdlog::logger>>> g_loggers;
std::mutex g_loggers_mutex;
spdlog::level::level_enum g_loggers_level = spdlog::level::level_enum::info;

logger &get_logger(std::string name) {
    std::lock_guard<std::mutex> guard(g_loggers_mutex);

    if (g_loggers.find(name) == g_loggers.end()) {
        spdlog::sink_ptr s = std::make_shared<spdlog::sinks::stdout_color_sink_mt>();
        g_loggers[name] = std::make_pair(
            s,
            std::make_shared<spdlog::logger>(name, s)
        );
    }

    auto rval = g_loggers.at(name).second;
    rval->set_level(g_loggers_level);
    return *rval;
};

void set_filter_level(LogLevel level) {
    std::lock_guard<std::mutex> guard(g_loggers_mutex);

    g_loggers_level = level;
    for (auto &pair : g_loggers) {
        pair.second.second->set_level(level);
    }
}