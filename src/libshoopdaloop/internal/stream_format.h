#pragma once

#include <ostream>
#include <iostream>
#include <string_view>
#include <tuple>

template<typename T>
void stream_format(std::ostream &out, T arg) {
    out << arg;
}

template <typename T, typename ...Args>
void stream_format(std::ostream &out, std::string_view str, T first, Args... args) {
    auto pos = str.find_first_of("{}");
    out << str.substr(0, pos) << first;
    stream_format(out, str.substr(pos+2), args...);
}