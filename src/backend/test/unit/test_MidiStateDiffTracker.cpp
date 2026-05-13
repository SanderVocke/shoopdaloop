#include "MidiStateDiffTracker.h"
#include "MidiStateTracker.h"
#include "midi_helpers.h"

#include <catch2/catch_test_macros.hpp>

TEST_CASE("MidiStateDiffTracker - channel pressure diff uses correct status byte", "[MidiStateDiffTracker][channel_pressure]") {
    // Create two state trackers that track controls (which includes channel pressure)
    auto tracker_a = shoop_make_shared<MidiStateTracker>(false, true, false);
    auto tracker_b = shoop_make_shared<MidiStateTracker>(false, true, false);

    // Create the diff tracker using default constructor
    auto diff_tracker = shoop_make_shared<MidiStateDiffTracker>();
    // Wire them up with reset
    diff_tracker->reset(tracker_a, tracker_b, StateDiffTrackerAction::None);

    // Set initial state in both trackers (same state, no diffs)
    // Use process_msg with channel pressure message: 0xD0 | channel, pressure_value
    uint8_t msg_init_a[] = {0xD0, 64};  // channel pressure on channel 0, value 64
    uint8_t msg_init_b[] = {0xD0, 64};
    tracker_a->process_msg(msg_init_a);
    tracker_b->process_msg(msg_init_b);

    // Verify no diffs exist initially
    CHECK(diff_tracker->get_diff().empty());

    // Now change only tracker_a's channel pressure
    uint8_t msg_change[] = {0xD0, 100};  // channel pressure on channel 0, value 100
    tracker_a->process_msg(msg_change);

    // Verify that a diff exists with the CORRECT status byte 0xD0 (not 0xE0 which is for pitch wheel)
    auto diffs = diff_tracker->get_diff();
    REQUIRE(diffs.size() == 1);
    
    // The status byte should be 0xD0 | channel (0xD0 for channel pressure)
    auto expected_channel_pressure_key = std::array<uint8_t, 2>{0xD0, 0};  // 0xD0 = channel pressure status
    auto pitch_wheel_key = std::array<uint8_t, 2>{0xE0, 0};  // 0xE0 = pitch wheel status (WRONG)
    
    CHECK(diffs.contains(expected_channel_pressure_key));
    CHECK_FALSE(diffs.contains(pitch_wheel_key));
}

TEST_CASE("MidiStateDiffTracker - channel pressure diff is independent from pitch wheel", "[MidiStateDiffTracker][channel_pressure][pitch_wheel]") {
    auto tracker_a = shoop_make_shared<MidiStateTracker>(false, true, false);
    auto tracker_b = shoop_make_shared<MidiStateTracker>(false, true, false);

    auto diff_tracker = shoop_make_shared<MidiStateDiffTracker>();
    diff_tracker->reset(tracker_a, tracker_b, StateDiffTrackerAction::None);

    // Set same initial state for both channel pressure and pitch wheel
    uint8_t cp_init_a[] = {0xD0, 50};  // channel pressure, channel 0, value 50
    uint8_t cp_init_b[] = {0xD0, 50};
    uint8_t pw_init_a[] = {0xE0, 0, 64};  // pitch wheel, channel 0, value 8192 (14-bit: 0x2000 = LSB=0, MSB=0x40)
    uint8_t pw_init_b[] = {0xE0, 0, 64};
    
    tracker_a->process_msg(cp_init_a);
    tracker_b->process_msg(cp_init_b);
    tracker_a->process_msg(pw_init_a);
    tracker_b->process_msg(pw_init_b);

    REQUIRE(diff_tracker->get_diff().empty());

    // Change only channel pressure on tracker_a
    uint8_t cp_change[] = {0xD0, 75};
    tracker_a->process_msg(cp_change);

    auto diffs = diff_tracker->get_diff();

    // Should have exactly 1 diff for channel pressure (0xD0), NOT for pitch wheel (0xE0)
    REQUIRE(diffs.size() == 1);
    
    // Verify it's channel pressure (0xD0) not pitch wheel (0xE0)
    CHECK(diffs.contains(std::array<uint8_t, 2>{0xD0, 0}));  // channel pressure - CORRECT
    CHECK_FALSE(diffs.contains(std::array<uint8_t, 2>{0xE0, 0}));  // pitch wheel - should NOT be present
}

TEST_CASE("MidiStateDiffTracker - check_channel_pressure uses correct byte", "[MidiStateDiffTracker][channel_pressure]") {
    auto tracker_a = shoop_make_shared<MidiStateTracker>(false, true, false);
    auto tracker_b = shoop_make_shared<MidiStateTracker>(false, true, false);

    auto diff_tracker = shoop_make_shared<MidiStateDiffTracker>();
    diff_tracker->reset(tracker_a, tracker_b, StateDiffTrackerAction::None);

    // Set different channel pressure values directly via process_msg
    uint8_t cp_a[] = {0xD5, 42};  // channel 5, value 42
    uint8_t cp_b[] = {0xD5, 0};   // channel 5, value 0 (different)
    tracker_a->process_msg(cp_a);
    tracker_b->process_msg(cp_b);

    // Use check_channel_pressure to populate diffs
    diff_tracker->check_channel_pressure(5);

    auto diffs = diff_tracker->get_diff();

    // Should have diff for channel pressure on channel 5: 0xD5
    CHECK(diffs.contains(std::array<uint8_t, 2>{0xD5, 0}));  // 0xD0 | 5
    CHECK_FALSE(diffs.contains(std::array<uint8_t, 2>{0xE5, 0}));  // 0xE0 | 5 - wrong!
}