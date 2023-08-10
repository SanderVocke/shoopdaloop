#include <boost/ut.hpp>
#include "DummyAudioSystem.h"
#include "PortInterface.h"
#include <functional>
#include <thread>
#include <chrono>
#include <set>

using namespace boost::ut;

struct Tracker {
    std::atomic<size_t> total_samples_processed;
    std::vector<size_t> each_n_samples_processed;

    void process(size_t n_samples) {
        each_n_samples_processed.push_back(n_samples);
        total_samples_processed += n_samples;
    }

    void reset() {
        total_samples_processed = 0;
        each_n_samples_processed.clear();
    }
};

template <typename Time, typename Size>
struct TrackedDummyAudioSystem : public DummyAudioSystem<Time, Size> {
    Tracker tracker;

    TrackedDummyAudioSystem(
        std::string client_name,
        DummyAudioSystemMode mode,
        std::function<void(size_t)> process_cb = nullptr,
        size_t sample_rate = 48000,
        size_t buffer_size = 256) : DummyAudioSystem<Time, Size>(
            client_name,
            [process_cb, this](size_t n) {
                if (process_cb) {
                    process_cb(n);
                }
                tracker.process(n);
            },
            mode,
            sample_rate,
            buffer_size
            ) {}
    
    std::set<size_t> get_unique_n_samples_processed() { return std::set (tracker.each_n_samples_processed.begin(), tracker.each_n_samples_processed.end()); }
};

suite DummyAudioSystem_tests = []() {
    "da_1_automatic"_test = []() {
        TrackedDummyAudioSystem<uint32_t, uint32_t> dut (
            "test",
            DummyAudioSystemMode::Automatic,
            nullptr,
            48000,
            256
        );
       
        dut.start();
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
        dut.close();

        expect(gt(dut.tracker.total_samples_processed.load(), 0));
        expect(eq(dut.get_unique_n_samples_processed().size(), 1));
        expect(eq(*dut.get_unique_n_samples_processed().begin(), 256));
    };

    "da_2_controlled"_test = []() {
        TrackedDummyAudioSystem<uint32_t, uint32_t> dut (
            "test",
            DummyAudioSystemMode::Controlled,
            nullptr,
            48000,
            256
        );
       
        dut.start();
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
        dut.pause();
        
        expect(eq(dut.tracker.total_samples_processed.load(), 0));
        expect(eq(dut.get_unique_n_samples_processed(), std::set<size_t>({0})));

        dut.controlled_mode_request_samples(64);
        dut.tracker.reset();
        expect(eq(dut.get_controlled_mode_samples_to_process(), 64));

        dut.resume();
        std::this_thread::sleep_for(std::chrono::milliseconds(60));
        dut.pause();

        expect(eq(dut.tracker.total_samples_processed.load(), 64));
        expect(eq(dut.get_unique_n_samples_processed(), std::set<size_t>({64, 0})));
        expect(eq(dut.get_controlled_mode_samples_to_process(), 0));

        dut.close();
    };

    "da_3_inputport_default"_test = []() {
        DummyAudioPort put("test_in", PortDirection::Input);

        auto buf = put.PROC_get_buffer(8);
        auto bufvec = std::vector<float>(buf, buf+8);
        expect(eq(bufvec, std::vector<float>({0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f})));
    };

    "da_3_inputport_queue"_test = []() {
        DummyAudioPort put("test_in", PortDirection::Input);
        std::vector<float> data({1, 2, 3, 4, 5, 6, 7, 8});
        put.queue_data(8, data.data());

        auto buf = put.PROC_get_buffer(8);
        auto bufvec = std::vector<float>(buf, buf+8);
        expect(eq(bufvec, data));
    };

    "da_3_inputport_queue_consume_multiple"_test = []() {
        DummyAudioPort put("test_in", PortDirection::Input);
        std::vector<float> data({1, 2, 3, 4, 5, 6, 7, 8});
        put.queue_data(8, data.data());

        {
            auto buf = put.PROC_get_buffer(4);
            auto bufvec = std::vector<float>(buf, buf+4);
            expect(eq(bufvec, std::vector<float>({1, 2, 3, 4})));
        }
        {
            auto buf = put.PROC_get_buffer(8);
            auto bufvec = std::vector<float>(buf, buf+8);
            expect(eq(bufvec, std::vector<float>({5, 6, 7, 8, 0, 0, 0, 0})));
        }
    };

    "da_4_inputport_queue_consume_combine"_test = []() {
        DummyAudioPort put("test_in", PortDirection::Input);
        std::vector<float> data({1, 2, 3, 4});
        put.queue_data(4, data.data());
        put.queue_data(4, data.data());

        {
            auto buf = put.PROC_get_buffer(10);
            auto bufvec = std::vector<float>(buf, buf+10);
            expect(eq(bufvec, std::vector<float>({1, 2, 3, 4, 1, 2, 3, 4, 0, 0})));
        }
    };
};