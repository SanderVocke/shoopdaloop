#include "LoggingBackend.h"
#include <map>
#include <spdlog/sinks/stdout_color_sinks.h>
#include <mutex>

std::map<std::string, std::shared_ptr<spdlog::logger>> g_loggers;
std::mutex g_loggers_mutex;

logger &get_logger(std::string name) {
    std::lock_guard<std::mutex> guard(g_loggers_mutex);
    
    if (g_loggers.find(name) == g_loggers.end()) {
        g_loggers[name] = spdlog::stdout_color_mt(name);
    }
    return *g_loggers.at(name);
};