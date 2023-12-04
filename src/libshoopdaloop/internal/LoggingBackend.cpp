#include "LoggingBackend.h"
#include <cstdlib>
#include <map>
#include <optional>
#include <mutex>
#include <iostream>
#include <algorithm>
#include <mutex>
#include <optional>
#include <vector>
namespace logging {

//TODO intentionally leaky globals because of de-initialization problems. Find a better solution.
std::recursive_mutex* g_log_mutex = new std::recursive_mutex();
std::unique_ptr<shoop_log_level_t>* g_maybe_global_level = new std::unique_ptr<shoop_log_level_t>(std::make_unique<shoop_log_level_t>(log_info));
std::map<std::string, std::unique_ptr<shoop_log_level_t>>* g_module_log_levels = new std::map<std::string, std::unique_ptr<shoop_log_level_t>>();
std::atomic<bool> g_log_initialized = false;

const std::map<std::string, shoop_log_level_t> level_names = {
    {"log_trace", log_trace},
    {"log_debug", log_debug},
    {"log_info",  log_info},
    {"log_warning", log_warning},
    {"error",   log_error}
};

void parse_conf_string(std::string s) {
    // remove spaces
    s.erase(std::remove_if(s.begin(), s.end(), [](char c) { return c == ' '; }), s.end());

    auto split = [](std::string_view s, std::string delimiter) {
        size_t pos = 0;
        std::vector<std::string> out;
        while (s.length() > 0) {
            pos = s.find(delimiter);
            if(pos == std::string::npos) {
                if (s.length()) { out.push_back(std::string(s)); }
                break;
            } else {
                if (pos > 0) { out.push_back(std::string(s.substr(0, pos))); }
                s = s.substr(pos + 1);
            }
        }
        return out;
    };
    auto parts = split(s, ",");

    for (auto &part : parts) {
        auto subparts = split(part, "=");
        if (subparts.size() == 1) { set_filter_level(level_names.at(subparts[0])); }
        else if (subparts.size() == 2) { set_module_filter_level(subparts[0].c_str(), level_names.at(subparts[1])); }
        else {
            throw std::runtime_error("Invalid logging config: more than two dot separators in part");
        }
    }
}

void parse_conf_from_env() {
    if (g_log_initialized) { return; }
    g_log_initialized = true;
    auto e = std::getenv("SHOOP_LOG");
    if (e) {
        parse_conf_string(e);
        log<"Backend.Logging", log_debug>(std::nullopt, std::nullopt, "Parsed logging config from environment: {}", e);
    }
}

}