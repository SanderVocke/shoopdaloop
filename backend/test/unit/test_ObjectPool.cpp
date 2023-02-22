#include <boost/ut.hpp>
#include "ObjectPool.h"
#include <memory>

using namespace boost::ut;
using namespace std::chrono_literals;

suite ObjectPool_tests = []() {
    "op_1_many_objects"_test = []() {
        ObjectPool<int> pool(10, 256);

        for(size_t idx=0; idx<100; idx++) {
            delete pool.get_object();
        }
    };
};