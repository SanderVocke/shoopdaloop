#pragma once
#include "ChannelInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
#include "WithCommandQueue.h"
#include "ProcessProfiling.h"
#include "LoggingEnabled.h"

template<typename SampleT>
class AudioChannel : public ChannelInterface,
                            private WithCommandQueue<20, 1000, 1000>,
                            private ModuleLoggingEnabled {
public:
    typedef AudioBuffer<SampleT> BufferObj;
    typedef ObjectPool<BufferObj> BufferPool;
    typedef std::shared_ptr<BufferObj> Buffer;
    
private:
    std::string log_module_name() const override;

    struct Buffers;

    // Members which may be accessed from any thread (ma prefix)
    std::shared_ptr<BufferPool> ma_buffer_pool;
    const size_t ma_buffer_size;
    std::atomic<int> ma_start_offset;
    std::atomic<size_t> ma_pre_play_samples;
    std::atomic<float> ma_output_peak;
    std::atomic<float> ma_volume;
    std::atomic<channel_mode_t> ma_mode;
    std::atomic<unsigned> ma_data_seq_nr;
    std::atomic<int> ma_last_played_back_sample; // -1 is none

    // Members which may be accessed from the process thread only (mp prefix)
    Buffers mp_buffers; // Buffers holding main audio data
    std::atomic<size_t> ma_buffers_data_length;
    Buffers mp_prerecord_buffers; // For temporarily holding pre-recorded data before fully entering record mode
    std::atomic<size_t> mp_prerecord_buffers_data_length;
    std::shared_ptr<profiling::ProfilingItem> mp_profiling_item;

    SampleT *mp_playback_target_buffer;
    size_t   mp_playback_target_buffer_size;
    SampleT *mp_recording_source_buffer;
    size_t   mp_recording_source_buffer_size;

    unsigned mp_prev_process_flags;

    enum class ProcessingCommandType {
        RawCopy,
        AdditiveCopy
    };

    struct RawCopyDetails {
        void* src;
        void* dst;
        size_t sz;
    };

    struct AdditiveCopyDetails {
        SampleT* src;
        SampleT* dst;
        float multiplier;
        size_t n_elems;
        bool update_absmax;
    };

    struct Buffers : private ModuleLoggingEnabled {
        std::string log_module_name() const override;

        size_t buffers_size;
        std::vector<Buffer> buffers;
        std::shared_ptr<BufferPool> pool;

        Buffers();
        Buffers(std::shared_ptr<BufferPool> pool, size_t initial_max_buffers);
        
        void reset();
        SampleT &at(size_t offset) const;
        bool ensure_available(size_t offset, bool use_pool=true);
        Buffer create_new_buffer() const;
        Buffer get_new_buffer() const;
        size_t n_buffers() const;
        size_t n_samples() const;
        size_t buf_space_for_sample(size_t offset) const;
    };

    union ProcessingCommandDetails {
        RawCopyDetails raw_copy_details;
        AdditiveCopyDetails additive_copy_details;
    };

    struct ProcessingCommand {
        ProcessingCommandType cmd_type;
        ProcessingCommandDetails details;
    };

    using ProcessingQueue = boost::lockfree::spsc_queue<ProcessingCommand, boost::lockfree::capacity<16>>;
    ProcessingQueue mp_proc_queue;

    void throw_if_commands_queued() const;

public:

    AudioChannel(
            std::shared_ptr<BufferPool> buffer_pool,
            size_t initial_max_buffers,
            channel_mode_t mode,
            std::shared_ptr<profiling::Profiler> maybe_profiler=nullptr);

    virtual void set_pre_play_samples(size_t samples) override;
    virtual size_t get_pre_play_samples() const override;

    // NOTE: only use on process thread!
    AudioChannel<SampleT>& operator= (AudioChannel<SampleT> const& other);

    AudioChannel();
    ~AudioChannel() override;

    void data_changed();

    void PROC_process(
        loop_mode_t mode,
        std::optional<loop_mode_t> maybe_next_mode,
        std::optional<size_t> maybe_next_mode_delay_cycles,
        std::optional<size_t> maybe_next_mode_eta,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
    ) override;

    void PROC_exec_cmd(ProcessingCommand cmd);
    void PROC_queue_memcpy(void* dst, void* src, size_t sz);
    void PROC_queue_additivecpy(SampleT* dst, SampleT* src, size_t n_elems, float mult, bool update_absmax);
    void PROC_finalize_process() override;

    // Load data into the loop. Should always be called outside
    // the processing thread.
    void load_data(SampleT* samples, size_t len, bool thread_safe = true);

    // Get loop data. Should always be called outside
    // the processing thread.
    std::vector<SampleT> get_data(bool thread_safe = true);

    void PROC_process_record(
        size_t n_samples,
        size_t record_from,
        Buffers &buffers,
        std::atomic<size_t> &buffers_data_length,
        SampleT *record_buffer,
        size_t record_buffer_size
    );

    void PROC_process_replace(size_t data_position, size_t length, size_t n_samples,
                              SampleT *record_buffer, size_t record_buffer_size);

    size_t get_length() const override;

    void PROC_set_length(size_t length) override;

    SampleT const& PROC_at(size_t position) const;

    void PROC_process_playback(int data_position, size_t length, size_t n_samples, bool muted, SampleT *playback_buffer, size_t playback_buffer_size);

    std::optional<size_t> PROC_get_next_poi(loop_mode_t mode,
                                       std::optional<loop_mode_t> maybe_next_mode,
                                       std::optional<size_t> maybe_next_mode_delay_cycles,
                                       std::optional<size_t> maybe_next_mode_eta,
                                       size_t length,
                                       size_t position) const override;

    void PROC_handle_poi(loop_mode_t mode,
                            size_t length,
                            size_t position) override;

    void PROC_set_playback_buffer(SampleT *buffer, size_t size);
    void PROC_set_recording_buffer(SampleT *buffer, size_t size);

    void PROC_clear(size_t length);

    float get_output_peak() const;

    void reset_output_peak();

    void set_mode(channel_mode_t mode) override;

    channel_mode_t get_mode() const override;

    void set_volume(float volume);

    float get_volume();
    
    void set_start_offset(int offset) override;

    int get_start_offset() const override;

    unsigned get_data_seq_nr() const override;

    std::optional<size_t> get_played_back_sample() const override;

protected:
    Buffer get_new_buffer() const;
};

#ifndef IMPLEMENT_AUDIOCHANNEL_H
extern template class AudioChannel<float>;
extern template class AudioChannel<int>;
#endif