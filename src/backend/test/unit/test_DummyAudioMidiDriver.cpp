#include <catch2/catch_test_macros.hpp>
#include "DummyAudioMidiDriver.h"
#include "PortInterface.h"
#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"
#include "BridgeObject.h"
#include <functional>
#include <thread>
#include <chrono>
#include <set>
#include <algorithm>

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
    std::shared_ptr<Tracker> tracker;

    TrackedDummyAudioMidiDriver(
        std::string client_name,
        DummyAudioMidiDriverMode mode,
        std::function<void(uint32_t)> process_cb = nullptr,
        uint32_t sample_rate = 48000,
        uint32_t buffer_size = 256) :
        DummyAudioMidiDriver<Time, Size>(),
        tracker(std::make_shared<Tracker>())
    {
        this->add_processor(std::static_pointer_cast<HasAudioProcessingFunction>(tracker));
        DummyAudioMidiDriverSettings settings;
        settings.buffer_size = buffer_size;
        settings.client_name = client_name;
        settings.sample_rate = sample_rate;
        DummyAudioMidiDriver<Time, Size>::enter_mode(mode);
        DummyAudioMidiDriver<Time, Size>::start(settings);
    }
    
    std::set<uint32_t> get_unique_n_samples_processed() { return std::set (tracker->each_n_samples_processed.begin(), tracker->each_n_samples_processed.end()); }
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

    REQUIRE(dut.tracker->total_samples_processed.load() > 0);
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
    
    REQUIRE(dut.tracker->total_samples_processed.load() == 0);
    REQUIRE(dut.get_unique_n_samples_processed() == std::set<uint32_t>({0}));

    dut.controlled_mode_request_samples(64);
    dut.tracker->reset();
    REQUIRE(dut.get_controlled_mode_samples_to_process() == 64);

    dut.resume();
    dut.wait_process();
    dut.pause();

    REQUIRE(dut.tracker->total_samples_processed.load() == 64);
    REQUIRE(dut.get_unique_n_samples_processed() == std::set<uint32_t>({64, 0}));
    REQUIRE(dut.get_controlled_mode_samples_to_process() == 0);

    dut.close();
};

TEST_CASE("DummyAudioMidiDriver - Input port default", "[DummyAudioMidiDriver][audio]") {
    DummyAudioPort put("test_in", shoop_port_direction_t::ShoopPortDirection_Input, nullptr);

    auto buf = put.PROC_get_buffer(8);
    auto bufvec = std::vector<float>(buf, buf+8);
    REQUIRE(bufvec == std::vector<float>({0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f}));
};

TEST_CASE("DummyAudioMidiDriver - Input port queue", "[DummyAudioMidiDriver][audio]") {
    DummyAudioPort put("test_in", shoop_port_direction_t::ShoopPortDirection_Input, nullptr);
    std::vector<float> data({1, 2, 3, 4, 5, 6, 7, 8});
    put.queue_data(8, data.data());

    put.PROC_prepare(8);
    put.PROC_process(8);
    auto buf = put.PROC_get_buffer(8);
    auto bufvec = std::vector<float>(buf, buf+8);
    REQUIRE(bufvec == data);
};

TEST_CASE("DummyAudioMidiDriver - Input port queue consume multiple", "[DummyAudioMidiDriver][audio]") {
    DummyAudioPort put("test_in", shoop_port_direction_t::ShoopPortDirection_Input, nullptr);
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
    DummyAudioPort put("test_in", shoop_port_direction_t::ShoopPortDirection_Input, nullptr);
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

TEST_CASE("DummyAudioMidiDriver - processors API reflects Rust registrations", "[DummyAudioMidiDriver][processor]") {
    DummyAudioMidiDriver<uint32_t, uint32_t> dut;
    DummyAudioMidiDriverSettings settings;
    settings.buffer_size = 256;
    settings.client_name = "test";
    settings.sample_rate = 48000;
    dut.enter_mode(DummyAudioMidiDriverMode::Controlled);
    dut.start(settings);

    REQUIRE(dut.processors().empty());

    auto first = std::make_shared<Tracker>();
    auto second = std::make_shared<Tracker>();
    dut.add_processor(std::static_pointer_cast<HasAudioProcessingFunction>(first));

    auto one = dut.processors();
    REQUIRE(one.size() == 1);
    auto first_resolved = bridge_object::lock_processor(one[0]);
    REQUIRE(first_resolved.has_value());
    REQUIRE(*first_resolved == first);

    dut.add_processor(std::static_pointer_cast<HasAudioProcessingFunction>(second));
    auto two = dut.processors();
    REQUIRE(two.size() == 2);
    std::vector<std::shared_ptr<HasAudioProcessingFunction>> locked;
    std::transform(two.begin(), two.end(), std::back_inserter(locked), [](auto const &handle) {
        auto resolved = bridge_object::lock_processor(handle);
        return resolved.value_or(nullptr);
    });
    REQUIRE(std::find(locked.begin(), locked.end(), first) != locked.end());
    REQUIRE(std::find(locked.begin(), locked.end(), second) != locked.end());

    dut.remove_processor(std::static_pointer_cast<HasAudioProcessingFunction>(first));
    auto after_remove = dut.processors();
    REQUIRE(after_remove.size() == 1);
    auto second_resolved = bridge_object::lock_processor(after_remove[0]);
    REQUIRE(second_resolved.has_value());
    REQUIRE(*second_resolved == second);

    dut.remove_processor(std::static_pointer_cast<HasAudioProcessingFunction>(second));
    REQUIRE(dut.processors().empty());

    dut.close();
}

TEST_CASE("DummyAudioMidiDriver - processor add/remove cycles", "[DummyAudioMidiDriver][processor]") {
    TrackedDummyAudioMidiDriver<uint32_t, uint32_t> dut(
        "test",
        DummyAudioMidiDriverMode::Automatic,
        nullptr,
        48000,
        256
    );

    auto extra = std::make_shared<Tracker>();
    dut.add_processor(std::static_pointer_cast<HasAudioProcessingFunction>(extra));
    dut.wait_process();
    auto processed_with_extra = extra->total_samples_processed.load();
    REQUIRE(processed_with_extra > 0);

    dut.remove_processor(std::static_pointer_cast<HasAudioProcessingFunction>(extra));
    auto processed_before = extra->total_samples_processed.load();
    for (int i = 0; i < 5; i++) { dut.wait_process(); }
    auto processed_after = extra->total_samples_processed.load();
    REQUIRE(processed_after == processed_before);

    // Re-add/remove cycles should remain stable.
    for (int i = 0; i < 20; i++) {
        auto t = std::make_shared<Tracker>();
        dut.add_processor(std::static_pointer_cast<HasAudioProcessingFunction>(t));
        dut.wait_process();
        REQUIRE(t->total_samples_processed.load() > 0);
        dut.remove_processor(std::static_pointer_cast<HasAudioProcessingFunction>(t));
        auto before = t->total_samples_processed.load();
        dut.wait_process();
        REQUIRE(t->total_samples_processed.load() == before);
    }

    dut.close();
}

TEST_CASE("DummyAudioMidiDriver - decoupled midi open/close stress", "[DummyAudioMidiDriver][midi][decoupled]") {
    TrackedDummyAudioMidiDriver<uint32_t, uint32_t> dut(
        "test",
        DummyAudioMidiDriverMode::Automatic,
        nullptr,
        48000,
        256
    );

    constexpr int n_iters = 200;
    for (int i = 0; i < n_iters; i++) {
        auto port = dut.open_decoupled_midi_port("decoupled", shoop_port_direction_t::ShoopPortDirection_Output);
        dut.wait_process();
        port->close();
        dut.unregister_decoupled_midi_port(port);
        port->forget_driver();
        dut.wait_process();
    }

    dut.close();
    REQUIRE(dut.tracker->total_samples_processed.load() > 0);
};

TEST_CASE("DummyAudioMidiDriver - decoupled midi registration keepalive until unregister", "[DummyAudioMidiDriver][midi][decoupled]") {
    TrackedDummyAudioMidiDriver<uint32_t, uint32_t> dut(
        "test",
        DummyAudioMidiDriverMode::Automatic,
        nullptr,
        48000,
        256
    );

    auto port = dut.open_decoupled_midi_port("decoupled", shoop_port_direction_t::ShoopPortDirection_Output);
    auto weak_port = std::weak_ptr<shoop_types::_DecoupledMidiPort>(port);

    // Drop local strong ref; registration should keep it alive until explicit unregister.
    port.reset();
    dut.wait_process();
    REQUIRE(weak_port.lock() != nullptr);

    auto locked = weak_port.lock();
    REQUIRE(locked != nullptr);
    locked->close();
    dut.unregister_decoupled_midi_port(locked);
    locked->forget_driver();
    locked.reset();

    dut.wait_process();
    CHECK(weak_port.lock() == nullptr);
    dut.close();
}