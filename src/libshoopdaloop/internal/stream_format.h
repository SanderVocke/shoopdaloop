#pragma once

#include <ostream>
#include <iostream>
#include <string_view>
#include <tuple>
#include <format>

template <typename String>
void stream_format(std::ostream &out, String str) {
    out << str;
}

template <typename String, typename ...Args>
void stream_format(std::ostream &out, String str, Args... args) {
    out << std::format(str, args...);
    //size_t pos = 10; //str.find_first_of("{}");
    // std::cerr << "str " << str << ", pos " << pos << std::endl;
    //std::cerr << "str " << 10;
    // std::cerr << "pos " << pos;
    // std::cerr << "first " << first;
    // printf("str %s\n", str.data());
    // printf("pos %zu\n", pos);
    //out << str.substr(0, pos) << first;
    //stream_format(out, str.substr(pos+2), args...);
}