#pragma once
#include <vector>
#include <stdio.h>
#include <functional>

template<typename S>
std::vector<S> create_buf(size_t size, std::function<S(size_t)> elem_fn) {
    std::vector<S> buf(size);
    for(size_t idx=0; idx < buf.size(); idx++) {
        buf[idx] = elem_fn(idx);
    }
    return buf;
}

template<typename Buf, typename S>
void for_buf_elems(Buf const& buf, std::function<void(size_t,S const&)> fn,
                   int start=0, int n=-1) {
    if(n < 0) { n = buf.head() - start; }
    for(size_t idx=start; idx < (start+n); idx++) {
        fn(idx, buf.at(idx));
    }
}

template<typename Loop, typename S>
void for_loop_elems(Loop const& loop, std::function<void(size_t,S const&)> fn,
                   int start=0, int n=-1) {
    if(n < 0) { n = loop.get_length() - start; }
    for(size_t idx=start; idx < (start+n); idx++) {
        fn(idx, loop.at(idx));
    }
}