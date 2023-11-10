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

std::recursive_mutex g_log_mutex;
std::unique_ptr<log_level_t> g_maybe_global_level;
std::map<std::string, std::unique_ptr<log_level_t>> g_module_log_levels;

const std::map<std::string, log_level_t> level_names = {
    {"trace", trace},
    {"debug", debug},
    {"info",  info},
    {"warning", warning},
    {"error",   error}
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
        else if (subparts.size() == 2) { set_module_filter_level(subparts[0].c_str(), level_names.at(subparts[1])); }
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