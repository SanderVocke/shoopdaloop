#include "AudioBuffer.h"
#include <memory>

// Provides an interface into the internals of an audio loop, for testing
// purposes.
template<typename BufferPtr>
class AudioLoopTestInterface {
public:
    virtual std::vector<BufferPtr> &buffers() = 0;
    virtual size_t get_current_buffer_idx() const = 0;
    virtual size_t get_position_in_current_buffer() const = 0;

    AudioLoopTestInterface() = default;
    virtual ~AudioLoopTestInterface() {}
};