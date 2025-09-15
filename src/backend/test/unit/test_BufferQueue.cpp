#include "BufferQueue.h"
#include "BufferPool.h"

#include <catch2/catch_test_macros.hpp>

TEST_CASE("BufferQueue - Starting state", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>(10, 5, 10);
    BufferQueue<int> q(pool, 10);

    CHECK(q.n_samples() == 0);
    CHECK(q.PROC_get().n_samples == 0);
    CHECK(q.PROC_get().data->size() == 0);
};

TEST_CASE("BufferQueue - Single Buf Full", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>(10, 5, 10);
    BufferQueue<int> q(pool, 10);

    std::vector<int> data = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};

    q.PROC_put(data.data(), data.size());

    CHECK(q.n_samples() == 10);
    CHECK(q.PROC_get().n_samples == 10);
    CHECK(q.PROC_get().data->size() == 1);
    CHECK(q.PROC_get().data->at(0)->at(0) == data[0]);
    CHECK(q.PROC_get().data->at(0)->at(1) == data[1]);
    CHECK(q.PROC_get().data->at(0)->at(2) == data[2]);
    CHECK(q.PROC_get().data->at(0)->at(3) == data[3]);
    CHECK(q.PROC_get().data->at(0)->at(4) == data[4]);
    CHECK(q.PROC_get().data->at(0)->at(5) == data[5]);
    CHECK(q.PROC_get().data->at(0)->at(6) == data[6]);
    CHECK(q.PROC_get().data->at(0)->at(7) == data[7]);
    CHECK(q.PROC_get().data->at(0)->at(8) == data[8]);
    CHECK(q.PROC_get().data->at(0)->at(9) == data[9]);
};

TEST_CASE("BufferQueue - Single Buf Partial", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>(10, 5, 10);
    BufferQueue<int> q(pool, 10);

    std::vector<int> data = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};

    q.PROC_put(data.data(), 3);

    CHECK(q.n_samples() == 3);
    CHECK(q.PROC_get().n_samples == 3);
    CHECK(q.PROC_get().data->size() == 1);
    CHECK((q.PROC_get().data->at(0)->at(0)) == data[0]);
    CHECK((q.PROC_get().data->at(0)->at(1)) == data[1]);
    CHECK((q.PROC_get().data->at(0)->at(2)) == data[2]);
};

TEST_CASE("BufferQueue - Two bufs full", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>(10, 5, 4);
    BufferQueue<int> q(pool, 4);

    std::vector<int> data = {1, 2, 3, 4, 5, 6, 7, 8};
    std::vector<int> firstpart = {1, 2, 3, 4};
    std::vector<int> secondpart = {5, 6, 7, 8};

    q.PROC_put(data.data(), data.size());

    CHECK(q.n_samples() == 8);
    CHECK(q.PROC_get().n_samples == 8);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((q.PROC_get().data->at(0)->at(0)) == firstpart[0]);
    CHECK((q.PROC_get().data->at(0)->at(1)) == firstpart[1]);
    CHECK((q.PROC_get().data->at(0)->at(2)) == firstpart[2]);
    CHECK((q.PROC_get().data->at(0)->at(3)) == firstpart[3]);
    CHECK((q.PROC_get().data->at(1)->at(0)) == secondpart[0]);
    CHECK((q.PROC_get().data->at(1)->at(1)) == secondpart[1]);
    CHECK((q.PROC_get().data->at(1)->at(2)) == secondpart[2]); 
    CHECK((q.PROC_get().data->at(1)->at(3)) == secondpart[3]);
};

TEST_CASE("BufferQueue - Two bufs partial", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>(10, 5, 4);
    BufferQueue<int> q(pool, 4);

    std::vector<int> data = {1, 2, 3, 4, 5, 6};
    std::vector<int> firstpart = {1, 2, 3, 4};
    std::vector<int> secondpart = {5, 6};

    q.PROC_put(data.data(), data.size());

    CHECK(q.n_samples() == 6);
    CHECK(q.PROC_get().n_samples == 6);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((q.PROC_get().data->at(0)->at(0)) == firstpart[0]);
    CHECK((q.PROC_get().data->at(0)->at(1)) == firstpart[1]);
    CHECK((q.PROC_get().data->at(0)->at(2)) == firstpart[2]);
    CHECK((q.PROC_get().data->at(0)->at(3)) == firstpart[3]);
    CHECK((q.PROC_get().data->at(1)->at(0)) == secondpart[0]);
    CHECK((q.PROC_get().data->at(1)->at(1)) == secondpart[1]);
};

TEST_CASE("BufferQueue - drop buffer", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>(2, 1, 2);
    BufferQueue<int> q(pool, 2);

    q.PROC_put({1, 2, 3, 4});

    CHECK(q.n_samples() == 4);
    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((q.PROC_get().data->at(0)->at(0)) == 1);
    CHECK((q.PROC_get().data->at(0)->at(1)) == 2); 
    CHECK((q.PROC_get().data->at(1)->at(0)) == 3);
    CHECK((q.PROC_get().data->at(1)->at(1)) == 4);

    q.PROC_put({5, 6});
    
    CHECK(q.n_samples() == 4);
    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((q.PROC_get().data->at(0)->at(0)) == 3);
    CHECK((q.PROC_get().data->at(0)->at(1)) == 4);
    CHECK((q.PROC_get().data->at(1)->at(0)) == 5);
    CHECK((q.PROC_get().data->at(1)->at(1)) == 6);
};

TEST_CASE("BufferQueue - drop buffer then change max to drop buffer", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>(2, 1, 2);
    BufferQueue<int> q(pool, 2);

    q.PROC_put({1, 2, 3, 4});

    CHECK(q.n_samples() == 4);
    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((q.PROC_get().data->at(0)->at(0)) == 1);
    CHECK((q.PROC_get().data->at(0)->at(1)) == 2);
    CHECK((q.PROC_get().data->at(1)->at(0)) == 3);
    CHECK((q.PROC_get().data->at(1)->at(1)) == 4);

    q.PROC_put({5, 6});
    
    CHECK(q.n_samples() == 4);
    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((q.PROC_get().data->at(0)->at(0)) == 3);
    CHECK((q.PROC_get().data->at(0)->at(1)) == 4);
    CHECK((q.PROC_get().data->at(1)->at(0)) == 5);
    CHECK((q.PROC_get().data->at(1)->at(1)) == 6);

    q.set_max_buffers(1);
    q.PROC_process();

    q.PROC_put({7, 8, 9, 10});
    
    CHECK(q.PROC_get().n_samples == 2);
    CHECK(q.PROC_get().data->size() == 1);
    CHECK((q.PROC_get().data->at(0)->at(0)) == 9);
    CHECK((q.PROC_get().data->at(0)->at(1)) == 10);
};

TEST_CASE("BufferQueue - clone then drop", "[BufferQueue][audio]") {
    auto pool = shoop_make_shared<BufferQueue<int>::UsedBufferPool>(2, 1, 2);
    BufferQueue<int> q(pool, 2);

    q.PROC_put({1, 2, 3, 4});
    auto clone = q.PROC_get();
    q.PROC_put({5, 6});

    CHECK(q.PROC_get().n_samples == 4);
    CHECK(q.PROC_get().data->size() == 2);
    CHECK((q.PROC_get().data->at(0)->at(0)) == 3);
    CHECK((q.PROC_get().data->at(0)->at(1)) == 4);
    CHECK((q.PROC_get().data->at(1)->at(0)) == 5);
    CHECK((q.PROC_get().data->at(1)->at(1)) == 6);

    CHECK(clone.n_samples == 4);
    CHECK(clone.data->at(0)->at(0) == 1);
    CHECK(clone.data->at(0)->at(1) == 2);
    CHECK(clone.data->at(1)->at(0) == 3);
    CHECK(clone.data->at(1)->at(1) == 4);
};