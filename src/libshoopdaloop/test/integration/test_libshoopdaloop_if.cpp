#include <catch2/catch_test_macros.hpp>
#include "libshoopdaloop_test_if.h"
#include "libshoopdaloop.h"
#include <memory>
#include "types.h"


TEST_CASE("LibShoopdaloop - Create destroy back-end", "[LibShoopdaloop]") {
    shoop_backend_session_t * c_backend = create_backend (Dummy, "dummy", "");
    auto weak_backend = std::weak_ptr<BackendSession>(internal_backend_session(c_backend));
    REQUIRE(weak_backend.lock() != nullptr);
    terminate_backend(c_backend);
    REQUIRE(weak_backend.lock() == nullptr);
};

TEST_CASE("LibShoopdaloop - Create destroy loop", "[LibShoopdaloop]") {
    shoop_backend_session_t * c_backend = create_backend (Dummy, "dummy", "");
    {
        auto c_loop = create_loop(c_backend);
        auto weak_loop = std::weak_ptr<ConnectedLoop>(internal_loop(c_loop));
        REQUIRE(weak_loop.lock() != nullptr);
        destroy_loop(c_loop);
        REQUIRE(weak_loop.lock() == nullptr);
    }
    terminate_backend(c_backend);
};

TEST_CASE("LibShoopdaloop - Create destroy back-end loop destroyed", "[LibShoopdaloop]") {
    shoop_backend_session_t * c_backend = create_backend (Dummy, "dummy", "");
    {
        auto c_loop = create_loop(c_backend);
        auto weak_loop = std::weak_ptr<ConnectedLoop>(internal_loop(c_loop));
        REQUIRE(weak_loop.lock() != nullptr);
        terminate_backend(c_backend);
        REQUIRE(weak_loop.lock() == nullptr);
    }
};

TEST_CASE("LibShoopdaloop - Channels not destroyed with loop", "[LibShoopdaloop]") {
    shoop_backend_session_t * c_backend = create_backend (Dummy, "dummy", "");
    {
        auto c_loop = create_loop(c_backend);
        shoopdaloop_loop_audio_channel_t* c_chan;
        {
            auto loop = internal_loop(c_loop);
            c_chan = add_audio_channel(c_loop, ChannelMode_Direct);
        }
        auto weak_chan = std::weak_ptr<ConnectedChannel>(internal_audio_channel(c_chan));
        auto weak_loop = std::weak_ptr<ConnectedLoop>(internal_loop(c_loop));
        REQUIRE(weak_chan.lock() != nullptr);
        REQUIRE(weak_loop.lock() != nullptr);
        destroy_loop(c_loop);
        REQUIRE(weak_loop.lock() == nullptr);
        REQUIRE(weak_chan.lock() == nullptr);
        terminate_backend(c_backend);
    }
};