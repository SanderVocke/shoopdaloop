#include "BufferQueue.h"
#include "BufferPool.h"

#include <catch2/catch_test_macros.hpp>

TEST_CASE("BufferQueue - Starting state", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>("Test", 10, 10);
    BufferQueue<int> q(pool, 10);

    CHECK(q.n_samples() == 0);
    CHECK(q.PROC_get().n_samples == 0);
    CHECK(q.PROC_get().data->size() == 0);
};

TEST_CASE("BufferQueue - Single Buf Full", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>("Test", 10, 10);
    BufferQueue<int> q(pool, 10);

    std::vector<int> data = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};

    q.PROC_put(data.data(), data.size());

    CHECK(q.n_samples() == 10);
    CHECK(q.PROC_get().n_samples == 10);
    CHECK(q.PROC_get().data->size() == 1);
    CHECK(*q.PROC_get().data->at(0) == data);
};

TEST_CASE("BufferQueue - Single Buf Partial", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>("Test", 10, 10);
    BufferQueue<int> q(pool, 10);

    std::vector<int> data = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};

    q.PROC_put(data.data(), 3);

    CHECK(q.n_samples() == 3);
    CHECK(q.PROC_get().n_samples == 3);
    CHECK(q.PROC_get().data->size() == 1);
    CHECK((*q.PROC_get().data->at(0))[0] == data[0]);
    CHECK((*q.PROC_get().data->at(0))[1] == data[1]);
    CHECK((*q.PROC_get().data->at(0))[2] == data[2]);
};

TEST_CASE("BufferQueue - Two bufs full", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>("Test", 10, 4);
    BufferQueue<int> q(pool, 4);

    std::vector<int> data = {1, 2, 3, 4, 5, 6, 7, 8};
    std::vector<int> firstpart = {1, 2, 3, 4};
    std::vector<int> secondpart = {5, 6, 7, 8};

    q.PROC_put(data.data(), data.size());

    CHECK(q.n_samples() == 8);
    CHECK(q.PROC_get().n_samples == 8);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((*q.PROC_get().data->at(0)) == firstpart);
    CHECK((*q.PROC_get().data->at(1)) == secondpart);
};

TEST_CASE("BufferQueue - Two bufs partial", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>("Test", 10, 4);
    BufferQueue<int> q(pool, 4);

    std::vector<int> data = {1, 2, 3, 4, 5, 6};
    std::vector<int> firstpart = {1, 2, 3, 4};
    std::vector<int> secondpart = {5, 6};

    q.PROC_put(data.data(), data.size());

    CHECK(q.n_samples() == 6);
    CHECK(q.PROC_get().n_samples == 6);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((*q.PROC_get().data->at(0)) == firstpart);
    CHECK((*q.PROC_get().data->at(1))[0] == secondpart[0]);
    CHECK((*q.PROC_get().data->at(1))[1] == secondpart[1]);
};

TEST_CASE("BufferQueue - drop buffer", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>("Test", 2, 2);
    BufferQueue<int> q(pool, 2);
    using Vec = std::vector<int>;

    q.PROC_put({1, 2, 3, 4});

    CHECK(q.n_samples() == 4);
    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((*q.PROC_get().data->at(0)) == Vec({1, 2}));
    CHECK((*q.PROC_get().data->at(1)) == Vec({3, 4}));

    q.PROC_put({5, 6});
    
    CHECK(q.n_samples() == 4);
    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((*q.PROC_get().data->at(0)) == Vec({3, 4}));
    CHECK((*q.PROC_get().data->at(1)) == Vec({5, 6}));
};

TEST_CASE("BufferQueue - drop buffer then change max to drop buffer", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>("Test", 2, 2);
    BufferQueue<int> q(pool, 2);
    using Vec = std::vector<int>;

    q.PROC_put({1, 2, 3, 4});

    CHECK(q.n_samples() == 4);
    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((*q.PROC_get().data->at(0)) == Vec({1, 2}));
    CHECK((*q.PROC_get().data->at(1)) == Vec({3, 4}));

    q.PROC_put({5, 6});
    
    CHECK(q.n_samples() == 4);
    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((*q.PROC_get().data->at(0)) == Vec({3, 4}));
    CHECK((*q.PROC_get().data->at(1)) == Vec({5, 6}));

    q.set_max_buffers(1);
    q.PROC_process();

    q.PROC_put({7, 8, 9, 10});
    
    CHECK(q.PROC_get().n_samples == 2);
    CHECK(q.PROC_get().data->size() == 1);
    CHECK((*q.PROC_get().data->at(0)) == Vec({9, 10}));
};

TEST_CASE("BufferQueue - clone then drop", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>("Test", 2, 2);
    BufferQueue<int> q(pool, 2);
    using Vec = std::vector<int>;

    q.PROC_put({1, 2, 3, 4});
    auto clone = q.PROC_get();
    q.PROC_put({5, 6});

    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((*q.PROC_get().data->at(0)) == Vec({3, 4}));
    CHECK((*q.PROC_get().data->at(1)) == Vec({5, 6}));

    CHECK(clone.n_samples == 4);
    CHECK(*clone.data->at(0) == Vec({1, 2}));
    CHECK(*clone.data->at(1) == Vec({3, 4}));
};