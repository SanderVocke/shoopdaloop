#include <catch2/catch_test_macros.hpp>
#include "ObjectPool.h"
#include <memory>

using namespace std::chrono_literals;

TEST_CASE("ObjectPool - Many objects", "[ObjectPool]") {
    ObjectPool<int> pool(10, 256);

    for(uint32_t idx=0; idx<100; idx++) {
        delete pool.get_object();
    }
};