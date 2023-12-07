#pragma once
#include "LoopInterface.h"
#include "WithCommandQueue.h"
#include "LoggingEnabled.h"
#include <deque>
#include <atomic>

#ifdef BASICLOOP_EXPOSE_ALL_FOR_TEST
#define private public
#define protected public
#endif

enum BasicPointOfInterestFlags {
    Trigger = 1,
    LoopEnd = 2,
    ChannelPOI = 4,
    BasicPointOfInterestFlags_End = 8,
};

// The basic loop implements common semantics shared by all loop types.
// That includes keeping track of the first upcoming point of interest,
// logic of handling points of interest while processing, planning
// mode transitions etc.
class BasicLoop : public LoopInterface,
                  protected WithCommandQueue,
                  protected ModuleLoggingEnabled<"Backend.Loop"> {

public:
    struct PointOfInterest {
        uint32_t when;
        unsigned type_flags; // To be defined per implementation
    };

protected:
    std::optional<PointOfInterest> mp_next_poi;
    std::optional<uint32_t> mp_next_trigger;
    std::shared_ptr<LoopInterface> mp_sync_source;
    std::deque<shoop_loop_mode_t> mp_planned_states;
    std::deque<int> mp_planned_state_countdowns;

    std::atomic<shoop_loop_mode_t> ma_mode;
    std::atomic<bool> ma_triggering_now;
    std::atomic<bool> ma_already_triggered;
    std::atomic<uint32_t> ma_length;
    std::atomic<uint32_t> ma_position;
    
    // Cached state for easy lock-free reading.
    std::atomic<shoop_loop_mode_t> ma_maybe_next_planned_mode;
    std::atomic<int> ma_maybe_next_planned_delay;

public:

    BasicLoop();
    ~BasicLoop() override;

    // We always assume that the POI has been properly updated by whatever calls were
    // made in the past. Simply return.
    std::optional<uint32_t> PROC_get_next_poi() const override;
    std::optional<uint32_t> PROC_predicted_next_trigger_eta() const override;
    void PROC_update_trigger_eta();
    virtual void PROC_update_poi();
    void PROC_handle_poi() override;
    bool PROC_is_triggering_now() override;
    virtual void PROC_process_channels(
        shoop_loop_mode_t mode,
        std::optional<shoop_loop_mode_t> maybe_next_mode,
        std::optional<uint32_t> maybe_next_mode_delay_cycles,
        std::optional<uint32_t> maybe_next_mode_eta,
        uint32_t n_samples,
        uint32_t pos_before,
        uint32_t pos_after,
        uint32_t length_before,
        uint32_t length_after
    );
    void PROC_process(uint32_t n_samples) override;
    void set_sync_source(std::shared_ptr<LoopInterface> const& src, bool thread_safe=true) override;
    std::shared_ptr<LoopInterface> get_sync_source(bool thread_safe = true) override;
    void PROC_update_planned_transition_cache();
    void PROC_trigger(bool propagate=true) override;
    void PROC_handle_sync() override;
    void PROC_handle_transition(shoop_loop_mode_t new_state);
    uint32_t get_n_planned_transitions(bool thread_safe=true) override;
    uint32_t get_planned_transition_delay(uint32_t idx, bool thread_safe=true) override;
    shoop_loop_mode_t get_planned_transition_state(uint32_t idx, bool thread_safe=true) override;
    void clear_planned_transitions(bool thread_safe) override;
    void plan_transition(shoop_loop_mode_t mode, uint32_t n_cycles_delay = 0, bool wait_for_sync = true, bool thread_safe=true) override;
    uint32_t get_position() const override;
    void set_position(uint32_t position, bool thread_safe=true) override;
    uint32_t get_length() const override;
    shoop_loop_mode_t get_mode() const override;
    void get_first_planned_transition(shoop_loop_mode_t &maybe_mode_out, uint32_t &delay_out) override;
    void set_length(uint32_t len, bool thread_safe=true) override;
    void set_mode(shoop_loop_mode_t mode, bool thread_safe=true) override;

protected:
    static std::optional<PointOfInterest> dominant_poi(std::optional<PointOfInterest> const& a, std::optional<PointOfInterest> const& b);
    static std::optional<PointOfInterest> dominant_poi(std::vector<std::optional<PointOfInterest>> const& pois);
};

#ifdef BASICLOOP_EXPOSE_ALL_FOR_TEST
#undef private
#undef protected
#endif