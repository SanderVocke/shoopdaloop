#include "LoggingBackend.h"
#include <cstdlib>
#include <map>
#include <optional>
#include <spdlog/common.h>
#include <spdlog/sinks/stdout_color_sinks.h>
#include <spdlog/logger.h>
#include <mutex>
#include <iostream>
#include <algorithm>
#include <mutex>
#include <optional>
#include <spdlog/logger.h>
#include "spdlog/sinks/stdout_color_sinks.h"

namespace logging {

logger &logging_logger();

spdlog::level::level_enum g_loggers_level = spdlog::level::level_enum::info;
std::map<std::string, LogLevel> g_maybe_module_levels;
struct Logger {
    spdlog::sink_ptr m_stdout;
    std::shared_ptr<logger> m_logger;

    void update_level() {
        auto maybe_override = g_maybe_module_levels.find(m_logger->m_logger->name());
        m_logger->m_logger->set_level(
            maybe_override != g_maybe_module_levels.end() ?
            maybe_override->second :
            g_loggers_level);
    }

    LogLevel get_level() const {
        return m_logger->m_logger->level();
    }
};

std::map<std::string, Logger> g_loggers;
std::recursive_mutex g_loggers_mutex;

logger &get_logger_impl(std::string name) {
    std::lock_guard<std::recursive_mutex> guard(g_loggers_mutex);

    if (g_loggers.find(name) == g_loggers.end()) {
        spdlog::sink_ptr _stdout = std::make_shared<spdlog::sinks::stdout_color_sink_mt>();
        g_loggers[name] = Logger {
            .m_stdout = _stdout,
            .m_logger = std::make_shared<logger>(std::make_shared<spdlog::logger>(name, _stdout))
        };
        g_loggers.at(name).update_level();
    }

    auto rval = g_loggers.at(name).m_logger;
    return *rval;
};

logger &logging_logger() {
    return get_logger_impl("Logging");
}

logger &get_logger(std::string name) {
    return get_logger_impl(name);
}

void set_filter_level(LogLevel level) {
    std::lock_guard<std::recursive_mutex> guard(g_loggers_mutex);

    int _level = (int)level;
    logging_logger().debug("Set global logging level to {}", _level);
    if (level < CompileTimeLogLevel) {
        int _compiletime = (int)CompileTimeLogLevel;
        logging_logger().warn("Compile-time log filtering is set at {}. To get level {} messages of back-end, please compile a debug version.", _compiletime, _level);
    }

    g_loggers_level = level;
    for (auto &l : g_loggers) {
        l.second.update_level();
    }
}

void set_module_filter_level(std::string name, std::optional<LogLevel> level) {
    std::lock_guard<std::recursive_mutex> guard(g_loggers_mutex);

    if (level.has_value()) {
        int _level = (int)level.value();
        logging_logger().debug("Set logging level override of module {} to {}", name, _level);
        if (level.value() < CompileTimeLogLevel) {
            int _compiletime = (int)CompileTimeLogLevel;
            logging_logger().warn("Compile-time log filtering is set at {}. To get level {} messages of back-end, please compile a debug version.", _compiletime, _level);
        }
    } else {
        logging_logger().debug("Remove logging level override of module {}", name);
    }

    if (level.has_value()) {
        g_maybe_module_levels[name] = level.value();
    } else {
        g_maybe_module_levels.erase(name);
    }
    auto maybe_logger = g_loggers.find(name);
    if (maybe_logger != g_loggers.end()) { maybe_logger->second.update_level(); }
}

const std::map<std::string, LogLevel> level_names = {
    {"trace", LogLevel::trace},
    {"debug", LogLevel::debug},
    {"info",  LogLevel::info},
    {"warning", LogLevel::warn},
    {"error",   LogLevel::err}
};

void parse_conf_string(std::string s) {
    // remove spaces
    s.erase(std::remove_if(s.begin(), s.end(), [](char c) { return c == ' '; }), s.end());

    auto split = [](std::string s, std::string delimiter) {
        size_t pos = 0;
        std::string token;
        std::vector<std::string> out;
        bool done = false;
        while (!done) {
            if((pos = s.find(delimiter)) == std::string::npos) {
                token = s;
                done = true;
            } else {
                token = s.substr(0, pos);
            }
            if (token.length() > 0) {
                out.push_back(token);
            }
            if (!done) {
                s.erase(0, pos + delimiter.length());
            }
        }
        return out;
    };

    auto parts = split(s, ",");

    for (auto &part : parts) {
        auto subparts = split(part, "=");
        if (subparts.size() == 1) { set_filter_level(level_names.at(subparts[0])); }
        else if (subparts.size() == 2) { set_module_filter_level(subparts[0], level_names.at(subparts[1])); }
        else {
            throw std::runtime_error("Invalid logging config: more than two dot separators in part");
        }
    }
}

void parse_conf_from_env() {
    auto e = std::getenv("SHOOP_LOG");
    if (e) {
        parse_conf_string(e);
    }
}

}