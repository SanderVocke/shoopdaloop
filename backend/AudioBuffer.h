#pragma once
#include <cstring>
#include <stdexcept>
#include <vector>
#include <stdio.h>

template<typename SampleT> class AudioBuffer {
    std::vector<SampleT> m_data;
    unsigned m_head;

public:
    AudioBuffer(size_t size) :
        m_data(std::vector<SampleT>(size)),
        m_head(0)
    {}

    size_t size() const { return m_data.size(); }
    size_t head() const { return m_head; }
    size_t space() const { return size() - head(); }
    SampleT* data() { return m_data.data(); }
    SampleT* data_at(size_t pos) { return m_data.data() + pos; }
    SampleT const& at(size_t pos) const { return m_data.at(pos); }

    void record(SampleT *source, size_t n_samples) {
        if(n_samples > space()) {
            throw std::runtime_error("Attempting to record out of bounds.");
        }
        memcpy((void*)(data_at(head())), (void*)source, n_samples*sizeof(SampleT));
        m_head += n_samples;
    }
};