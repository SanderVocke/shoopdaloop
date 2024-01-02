#pragma once
#include "ChannelInterface.h"
#include "ObjectPool.h"
#include "AudioBuffer.h"
#include "WithCommandQueue.h"
#include "LoggingEnabled.h"
#include <stdint.h>

template<typename SampleT>
class AudioChannel : public ChannelInterface,
                            private WithCommandQueue,
                            private ModuleLoggingEnabled<"Backend.AudioChannel"> {
public:
    typedef AudioBuffer<SampleT> BufferObj;
    typedef ObjectPool<BufferObj> BufferPool;
    typedef std::shared_ptr<BufferObj> Buffer;
    
private:

    struct Buffers;

    // Members which may be accessed from any thread (ma prefix)
    std::shared_ptr<BufferPool> ma_buffer_pool;
    const uint32_t ma_buffer_size;
    std::atomic<int> ma_start_offset;
    std::atomic<uint32_t> ma_pre_play_samples;
    std::atomic<float> ma_output_peak;
    std::atomic<float> ma_gain;
    std::atomic<shoop_channel_mode_t> ma_mode;
    std::atomic<unsigned> ma_data_seq_nr;
    std::atomic<int> ma_last_played_back_sample; // -1 is none

    // Members which may be accessed from the process thread only (mp prefix)
    Buffers mp_buffers; // Buffers holding main audio data
    std::atomic<uint32_t> ma_buffers_data_length;
    Buffers mp_prerecord_buffers; // For temporarily holding pre-recorded data before fully entering record mode
    std::atomic<uint32_t> mp_prerecord_buffers_data_length;

    SampleT *mp_playback_target_buffer;
    uint32_t   mp_playback_target_buffer_size;
    SampleT *mp_recording_source_buffer;
    uint32_t   mp_recording_source_buffer_size;

    unsigned mp_prev_process_flags;

    enum class ProcessingCommandType {
        RawCopy,
        AdditiveCopy
    };

    struct RawCopyDetails {
        void* src;
        void* dst;
        uint32_t sz;
    };

    struct AdditiveCopyDetails {
        SampleT* src;
        SampleT* dst;
        float multiplier;
        uint32_t n_elems;
        bool update_absmax;
    };

    struct Buffers : private ModuleLoggingEnabled<"Backend.AudioChannel.Buffers"> {

        uint32_t buffers_size;
        std::vector<Buffer> buffers;
        std::shared_ptr<BufferPool> pool;

        Buffers();
        Buffers(std::shared_ptr<BufferPool> pool, uint32_t initial_max_buffers);
        
        void reset();
        SampleT &at(uint32_t offset) const;
        bool ensure_available(uint32_t offset, bool use_pool=true);
        Buffer create_new_buffer() const;
        Buffer get_new_buffer() const;
        uint32_t n_buffers() const;
        uint32_t n_samples() const;
        uint32_t buf_space_for_sample(uint32_t offset) const;
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
            uint32_t initial_max_buffers,
            shoop_channel_mode_t mode);

    virtual void set_pre_play_samples(uint32_t samples) override;
    virtual uint32_t get_pre_play_samples() const override;

    // NOTE: only use on process thread!
    AudioChannel<SampleT>& operator= (AudioChannel<SampleT> const& other);

    AudioChannel();
    ~AudioChannel() override;

    void data_changed();

    void PROC_process(
        shoop_loop_mode_t mode,
        std::optional<shoop_loop_mode_t> maybe_next_mode,
        std::optional<uint32_t> maybe_next_mode_delay_cycles,
        std::optional<uint32_t> maybe_next_mode_eta,
        uint32_t n_samples,
        uint32_t pos_before,
        uint32_t pos_after,
        uint32_t length_before,
        uint32_t length_after
    ) override;

    void PROC_exec_cmd(ProcessingCommand cmd);
    void PROC_queue_memcpy(void* dst, void* src, uint32_t sz);
    void PROC_queue_additivecpy(SampleT* dst, SampleT* src, uint32_t n_elems, float mult, bool update_absmax);
    void PROC_finalize_process() override;

    // Load data into the loop. Should always be called outside
    // the processing thread.
    void load_data(SampleT* samples, uint32_t len, bool thread_safe = true);

    // Get loop data. Should always be called outside
    // the processing thread.
    std::vector<SampleT> get_data(bool thread_safe = true);

    void PROC_process_record(
        uint32_t n_samples,
        uint32_t record_from,
        Buffers &buffers,
        std::atomic<uint32_t> &buffers_data_length,
        SampleT *record_buffer,
        uint32_t record_buffer_size
    );

    void PROC_process_replace(uint32_t data_position, uint32_t length, uint32_t n_samples,
                              SampleT *record_buffer, uint32_t record_buffer_size);

    uint32_t get_length() const override;

    void PROC_set_length(uint32_t length) override;

    SampleT const& PROC_at(uint32_t position) const;

    void PROC_process_playback(int data_position, uint32_t length, uint32_t n_samples, bool muted, SampleT *playback_buffer, uint32_t playback_buffer_size);

    std::optional<uint32_t> PROC_get_next_poi(shoop_loop_mode_t mode,
                                       std::optional<shoop_loop_mode_t> maybe_next_mode,
                                       std::optional<uint32_t> maybe_next_mode_delay_cycles,
                                       std::optional<uint32_t> maybe_next_mode_eta,
                                       uint32_t length,
                                       uint32_t position) const override;

    void PROC_handle_poi(shoop_loop_mode_t mode,
                            uint32_t length,
                            uint32_t position) override;

    void PROC_set_playback_buffer(SampleT *buffer, uint32_t size);
    void PROC_set_recording_buffer(SampleT *buffer, uint32_t size);

    void PROC_clear(uint32_t length);

    float get_output_peak() const;

    void reset_output_peak();

    void set_mode(shoop_channel_mode_t mode) override;

    shoop_channel_mode_t get_mode() const override;

    void set_gain(float gain);

    float get_gain();
    
    void set_start_offset(int offset) override;

    int get_start_offset() const override;

    unsigned get_data_seq_nr() const override;

    std::optional<uint32_t> get_played_back_sample() const override;

protected:
    Buffer get_new_buffer() const;
};

#ifndef IMPLEMENT_AUDIOCHANNEL_H
extern template class AudioChannel<float>;
extern template class AudioChannel<int>;
#endif