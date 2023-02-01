#include <boost/ut.hpp>
#include "AudioBufferPool.h"
#include <memory>

using namespace boost::ut;
using namespace std::chrono_literals;

suite AudioBufferPool_tests = []() {
    "1_many_buffers"_test = []() {
        AudioBufferPool<int> pool(10, 256);

        for(size_t idx=0; idx<100; idx++) {
            delete pool.get_buffer();
        }
    };
};