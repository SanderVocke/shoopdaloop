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
                  protected WithCommandQueue<100, 1000, 1000>,
                  private ModuleLoggingEnabled {

    std::string log_module_name() const override;

public:
    struct PointOfInterest {
        size_t when;
        unsigned type_flags; // To be defined per implementation
    };

protected:
    std::optional<PointOfInterest> mp_next_poi;
    std::optional<size_t> mp_next_trigger;
    std::shared_ptr<LoopInterface> mp_sync_source;
    std::deque<loop_mode_t> mp_planned_states;
    std::deque<int> mp_planned_state_countdowns;

    std::atomic<loop_mode_t> ma_mode;
    std::atomic<bool> ma_triggering_now;
    std::atomic<bool> ma_already_triggered;
    std::atomic<size_t> ma_length;
    std::atomic<size_t> ma_position;
    
    // Cached state for easy lock-free reading.
    std::atomic<loop_mode_t> ma_maybe_next_planned_mode;
    std::atomic<int> ma_maybe_next_planned_delay;

public:

    BasicLoop();
    ~BasicLoop() override;

    // We always assume that the POI has been properly updated by whatever calls were
    // made in the past. Simply return.
    std::optional<size_t> PROC_get_next_poi() const override;
    std::optional<size_t> PROC_predicted_next_trigger_eta() const override;
    void PROC_update_trigger_eta();
    virtual void PROC_update_poi();
    void PROC_handle_poi() override;
    bool PROC_is_triggering_now() override;
    virtual void PROC_process_channels(
        loop_mode_t mode,
        std::optional<loop_mode_t> maybe_next_mode,
        std::optional<size_t> maybe_next_mode_delay_cycles,
        std::optional<size_t> maybe_next_mode_eta,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
    );
    void PROC_process(size_t n_samples) override;
    void set_sync_source(std::shared_ptr<LoopInterface> const& src, bool thread_safe=true) override;
    std::shared_ptr<LoopInterface> get_sync_source(bool thread_safe = true) override;
    void PROC_update_planned_transition_cache();
    void PROC_trigger(bool propagate=true) override;
    void PROC_handle_sync() override;
    void PROC_handle_transition(loop_mode_t new_state);
    size_t get_n_planned_transitions(bool thread_safe=true) override;
    size_t get_planned_transition_delay(size_t idx, bool thread_safe=true) override;
    loop_mode_t get_planned_transition_state(size_t idx, bool thread_safe=true) override;
    void clear_planned_transitions(bool thread_safe) override;
    void plan_transition(loop_mode_t mode, size_t n_cycles_delay = 0, bool wait_for_sync = true, bool thread_safe=true) override;
    size_t get_position() const override;
    void set_position(size_t position, bool thread_safe=true) override;
    size_t get_length() const override;
    loop_mode_t get_mode() const override;
    void get_first_planned_transition(loop_mode_t &maybe_mode_out, size_t &delay_out) override;
    void set_length(size_t len, bool thread_safe=true) override;
    void set_mode(loop_mode_t mode, bool thread_safe=true) override;

protected:
    static std::optional<PointOfInterest> dominant_poi(std::optional<PointOfInterest> const& a, std::optional<PointOfInterest> const& b);
    static std::optional<PointOfInterest> dominant_poi(std::vector<std::optional<PointOfInterest>> const& pois);
};

#ifdef BASICLOOP_EXPOSE_ALL_FOR_TEST
#undef private
#undef protected
#endif