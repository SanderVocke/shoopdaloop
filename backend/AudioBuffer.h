#pragma once
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
    void increment_head(size_t amount) { m_head += amount; }
};