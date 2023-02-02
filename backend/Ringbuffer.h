#pragma once
#include <cstddef>
#include <cstdint>
#include <vector>
#include <stdexcept>
#include <optional>

template<typename T>
struct Ringbuffer {
    std::vector<T> buf;
    size_t head, tail;

    Ringbuffer(size_t size) :
        head(0), tail(0), buf(std::vector<T>(size)) {}

    size_t size() const {
        return head >= tail ?
            (head - tail) : ((head + buf.size()) - tail);
    }

    T &at(size_t idx) {
        if (idx > size()) { throw std::runtime_error("Ringbuf access out of bounds"); }
        size_t mapped = (tail + idx) % buf.size();
        return buf.at(mapped);
    }

    size_t space() const {
        return buf.size() - size();
    }

    void push_back(T const& elem) {
        if (space() == 0) { throw std::runtime_error{"Ringbuffer overflow"} }
        buf[head] = elem;
        head = (head + 1) % buf.size();
    }

    void push_back(T* elems, size_t n) {
        if (space() < n) { throw std::runtime_error{"Ringbuffer overflow"}; }
        for (size_t idx = 0; idx < n; idx++) {
            push_back(elems[idx]);
        }
    }

    void insert_front(T const& elem) {
        if (space() == 0) { throw std::runtime_error{"Ringbuffer overflow"}; }
        tail = (tail == 0) ? buf.size() - 1 : tail - 1;
        buf[tail] = elem;
    }

    void insert_front(T* elems, size_t n) {
        if (space() < n) { throw std::runtime_error{"Ringbuffer overflow"}; }
        for (size_t idx = 0; idx < n; idx++) {
            insert_front(elems[idx]);
        }
    }

    void insert(T const& elem, size_t pos) {
        if (pos >= size() || space() == 0) { throw std::runtime_error{"Ringbuffer overflow"} }
        if (pos == 0) { insert_front(elem); return; }
        if (pos == size()) { push_back(elem); return; }
        // Move elements to make space
        head = (head+1) % buf.size();
        for (size_t idx = size()-2; idx >= pos; idx--) {
            at(idx+1) = at(idx);
        }
        at(pos) = elem;
    }

    void insert(T *elems, size_t n, size_t pos) {
        if (pos >= size() || space() < n ) { throw std::runtime_error{"Ringbuffer overflow"} }
        if (pos == 0) { insert_front(elems, n); return; }
        if (pos == size()) { push_back(elems, n); return; }
        // Move elements to make space
        head = (head+n) % buf.size();
        for (size_t idx = size()-1-n; idx >= pos; idx--) {
            at(idx+n) = at(idx);
        }
        for (size_t idx = 0; idx < n; idx++) {
            at(pos+idx) = elems[idx];
        }
    }

    template<typename Fn>
    bool insert(T const& elem, Fn predicate) {
        for(size_t idx=0; idx<=size(); idx++) {
            std::optional<T> before = (idx == 0) ?
                std::nullopt : at(idx - 1);
            std::optional<T> after = (idx == size()) ?
                std::nullopt : at(idx);
            if (predicate(before, after)) {
                insert(elem, idx);
                return true;
            }
        }
        return false;
    }

    template<typename Fn>
    bool insert(T* elems, size_t n, Fn predicate) {
        for(size_t idx=0; idx<=size(); idx++) {
            std::optional<T> before = (idx == 0) ?
                std::nullopt : at(idx - 1);
            std::optional<T> after = ((idx+n) < size()) ?
                std::nullopt : at(idx+n-1);
            if (predicate(before, after)) {
                insert(elems, n, idx);
                return true;
            }
        }
        return false;
    }

    void clear() {
        head = tail = 0;
    }
};