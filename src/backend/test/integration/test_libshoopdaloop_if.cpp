#include <catch2/catch_test_macros.hpp>
#include "libshoopdaloop_test_if.h"
#include "libshoopdaloop_backend.h"
#include <memory>
#include "types.h"


TEST_CASE("LibShoopdaloop - Create destroy back-end session", "[LibShoopdaloop]") {
    shoop_backend_session_t * c_backend = create_backend_session ();
    auto weak_backend = shoop_weak_ptr<BackendSession>(internal_backend_session(c_backend));
    REQUIRE(weak_backend.lock() != nullptr);
    destroy_backend_session(c_backend);
    REQUIRE(weak_backend.lock() == nullptr);
};

TEST_CASE("LibShoopdaloop - Create destroy loop", "[LibShoopdaloop]") {
    shoop_backend_session_t * c_backend = create_backend_session ();
    {
        auto c_loop = create_loop(c_backend);
        auto weak_loop = shoop_weak_ptr<GraphLoop>(internal_loop(c_loop));
        REQUIRE(weak_loop.lock() != nullptr);
        destroy_loop(c_loop);
        REQUIRE(weak_loop.lock() == nullptr);
    }
    destroy_backend_session(c_backend);
};

TEST_CASE("LibShoopdaloop - Create destroy back-end loop destroyed", "[LibShoopdaloop]") {
    shoop_backend_session_t * c_backend = create_backend_session ();
    {
        auto c_loop = create_loop(c_backend);
        auto weak_loop = shoop_weak_ptr<GraphLoop>(internal_loop(c_loop));
        REQUIRE(weak_loop.lock() != nullptr);
        destroy_backend_session(c_backend);
        REQUIRE(weak_loop.lock() == nullptr);
    }
};

TEST_CASE("LibShoopdaloop - Channels not destroyed with loop", "[LibShoopdaloop]") {
    shoop_backend_session_t * c_backend = create_backend_session ();
    {
        auto c_loop = create_loop(c_backend);
        shoopdaloop_loop_audio_channel_t* c_chan;
        {
            auto loop = internal_loop(c_loop);
            c_chan = add_audio_channel(c_loop, ChannelMode_Direct);
        }
        auto weak_chan = shoop_weak_ptr<GraphLoopChannel>(internal_audio_channel(c_chan));
        auto weak_loop = shoop_weak_ptr<GraphLoop>(internal_loop(c_loop));
        REQUIRE(weak_chan.lock() != nullptr);
        REQUIRE(weak_loop.lock() != nullptr);
        destroy_loop(c_loop);
        REQUIRE(weak_loop.lock() == nullptr);
        REQUIRE(weak_chan.lock() == nullptr);
        destroy_backend_session(c_backend);
    }
};

TEST_CASE("LibShoopdaloop - Decoupled MIDI send path is operational", "[LibShoopdaloop][midi][decoupled]") {
    shoop_audio_driver_t *c_driver = create_audio_driver(Dummy, nullptr);
    shoop_dummy_audio_driver_settings_t settings{};
    settings.sample_rate = 48000;
    settings.buffer_size = 256;
    settings.client_name = "test";
    start_dummy_driver(c_driver, settings);

    auto c_port = open_decoupled_midi_port(c_driver, "decoupled", ShoopPortDirection_Output);
    REQUIRE(c_port != nullptr);

    unsigned char msg[3] = {0x90, 60, 100};
    send_decoupled_midi(c_port, 3, msg);

    // Output decoupled ports do not expose incoming messages.
    auto ev = maybe_next_message(c_port);
    CHECK(ev == nullptr);

    auto name = get_decoupled_midi_port_name(c_port);
    CHECK(name != nullptr);

    close_decoupled_midi_port(c_port);
    destroy_shoopdaloop_decoupled_midi_port(c_port);
    destroy_audio_driver(c_driver);
}

TEST_CASE("LibShoopdaloop - Decoupled MIDI stale handle access is safe", "[LibShoopdaloop][midi][decoupled]") {
    shoop_audio_driver_t *c_driver = create_audio_driver(Dummy, nullptr);
    shoop_dummy_audio_driver_settings_t settings{};
    settings.sample_rate = 48000;
    settings.buffer_size = 256;
    settings.client_name = "test";
    start_dummy_driver(c_driver, settings);

    auto c_port = open_decoupled_midi_port(c_driver, "decoupled", ShoopPortDirection_Output);
    REQUIRE(c_port != nullptr);

    close_decoupled_midi_port(c_port);

    // Stale-handle API calls should not crash and should return safe defaults.
    auto ev = maybe_next_message(c_port);
    CHECK(ev == nullptr);

    auto name = get_decoupled_midi_port_name(c_port);
    CHECK(name != nullptr);

    auto state = get_decoupled_midi_port_connections_state(c_port);
    CHECK(state != nullptr);
    destroy_port_connections_state(state);

    destroy_shoopdaloop_decoupled_midi_port(c_port);
    destroy_audio_driver(c_driver);
}
