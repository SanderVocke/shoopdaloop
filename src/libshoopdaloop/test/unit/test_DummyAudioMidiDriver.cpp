#include <catch2/catch_test_macros.hpp>
#include "DummyAudioMidiDriver.h"
#include "PortInterface.h"
#include "AudioMidiDriver.h"
#include <functional>
#include <thread>
#include <chrono>
#include <set>

struct Tracker : public HasAudioProcessingFunction {
    std::atomic<uint32_t> total_samples_processed = 0;
    std::vector<uint32_t> each_n_samples_processed;

    Tracker() {
        total_samples_processed = 0;
    }
    
    void PROC_process(uint32_t n_samples) override {
        each_n_samples_processed.push_back(n_samples);
        total_samples_processed += n_samples;
    }

    void reset() {
        total_samples_processed = 0;
        each_n_samples_processed.clear();
    }
};

template <typename Time, typename Size>
struct TrackedDummyAudioMidiDriver : public DummyAudioMidiDriver<Time, Size> {
    Tracker tracker;

    TrackedDummyAudioMidiDriver(
        std::string client_name,
        DummyAudioMidiDriverMode mode,
        std::function<void(uint32_t)> process_cb = nullptr,
        uint32_t sample_rate = 48000,
        uint32_t buffer_size = 256) :
        DummyAudioMidiDriver<Time, Size>(),
        tracker(Tracker())
    {
        AudioMidiDriver::add_processor(tracker);
        DummyAudioMidiDriverSettings settings;
        settings.buffer_size = buffer_size;
        settings.client_name = client_name;
        settings.sample_rate = sample_rate;
        DummyAudioMidiDriver<Time, Size>::enter_mode(mode);
        DummyAudioMidiDriver<Time, Size>::start(settings);
    }
    
    std::set<uint32_t> get_unique_n_samples_processed() { return std::set (tracker.each_n_samples_processed.begin(), tracker.each_n_samples_processed.end()); }
};

TEST_CASE("DummyAudioMidiDriver - Automatic", "[DummyAudioMidiDriver]") {
    TrackedDummyAudioMidiDriver<uint32_t, uint32_t> dut (
        "test",
        DummyAudioMidiDriverMode::Automatic,
        nullptr,
        48000,
        256
    );
    
    dut.wait_process();\
    dut.close();

    REQUIRE(dut.tracker.total_samples_processed.load() > 0);
    REQUIRE(dut.get_unique_n_samples_processed().size() == 1);
    REQUIRE(*dut.get_unique_n_samples_processed().begin() == 256);
};

TEST_CASE("DummyAudioMidiDriver - Controlled", "[DummyAudioMidiDriver]") {
    TrackedDummyAudioMidiDriver<uint32_t, uint32_t> dut (
        "test",
        DummyAudioMidiDriverMode::Controlled,
        nullptr,
        48000,
        256
    );
    
    dut.wait_process();
    dut.pause();
    
    REQUIRE(dut.tracker.total_samples_processed.load() == 0);
    REQUIRE(dut.get_unique_n_samples_processed() == std::set<uint32_t>({0}));

    dut.controlled_mode_request_samples(64);
    dut.tracker.reset();
    REQUIRE(dut.get_controlled_mode_samples_to_process() == 64);

    dut.resume();
    dut.wait_process();
    dut.pause();

    REQUIRE(dut.tracker.total_samples_processed.load() == 64);
    REQUIRE(dut.get_unique_n_samples_processed() == std::set<uint32_t>({64, 0}));
    REQUIRE(dut.get_controlled_mode_samples_to_process() == 0);

    dut.close();
};

TEST_CASE("DummyAudioMidiDriver - Input port default", "[DummyAudioMidiDriver][audio]") {
    DummyAudioPort put("test_in", shoop_port_direction_t::Input);

    auto buf = put.PROC_get_buffer(8);
    auto bufvec = std::vector<float>(buf, buf+8);
    REQUIRE(bufvec == std::vector<float>({0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f}));
};

TEST_CASE("DummyAudioMidiDriver - Input port queue", "[DummyAudioMidiDriver][audio]") {
    DummyAudioPort put("test_in", shoop_port_direction_t::Input);
    std::vector<float> data({1, 2, 3, 4, 5, 6, 7, 8});
    put.queue_data(8, data.data());

    put.PROC_prepare(8);
    put.PROC_process(8);
    auto buf = put.PROC_get_buffer(8);
    auto bufvec = std::vector<float>(buf, buf+8);
    REQUIRE(bufvec == data);
};

TEST_CASE("DummyAudioMidiDriver - Input port queue consume multiple", "[DummyAudioMidiDriver][audio]") {
    DummyAudioPort put("test_in", shoop_port_direction_t::Input);
    std::vector<float> data({1, 2, 3, 4, 5, 6, 7, 8});
    put.queue_data(8, data.data());

    {
        put.PROC_prepare(4);
        put.PROC_process(4);
        auto buf = put.PROC_get_buffer(4);
        auto bufvec = std::vector<float>(buf, buf+4);
        REQUIRE(bufvec == std::vector<float>({1, 2, 3, 4}));
    }
    {
        put.PROC_prepare(8);
        put.PROC_process(8);
        auto buf = put.PROC_get_buffer(8);
        auto bufvec = std::vector<float>(buf, buf+8);
        REQUIRE(bufvec == std::vector<float>({5, 6, 7, 8, 0, 0, 0, 0}));
    }
};

TEST_CASE("DummyAudioMidiDriver - Input port queue consume combine", "[DummyAudioMidiDriver][audio]") {
    DummyAudioPort put("test_in", shoop_port_direction_t::Input);
    std::vector<float> data({1, 2, 3, 4});
    put.queue_data(4, data.data());
    put.queue_data(4, data.data());

    {
        put.PROC_prepare(10);
        put.PROC_process(10);
        auto buf = put.PROC_get_buffer(10);
        auto bufvec = std::vector<float>(buf, buf+10);
        REQUIRE(bufvec == std::vector<float>({1, 2, 3, 4, 1, 2, 3, 4, 0, 0}));
    }
};