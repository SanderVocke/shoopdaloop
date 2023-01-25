#include <vector>
#include <stdio.h>

template<class SampleT> class AudioBuffer {
    std::vector<SampleT> m_data;
    unsigned m_head;

    AudioBuffer(size_t size) :
        m_data(std::vector<SampleT>(size)),
        m_head(0)
    {}

    size_t size() const { return m_data.size(); }
    size_t head() const { return m_head; }
    size_t space() const { return size() - head(); }
    SampleT* data() const { return m_data.data(); }
    SampleT* data_at(size_t pos) const { return &(m_data[pos]); }
    void increment_head(size_t amount) { m_head += amount; }
};