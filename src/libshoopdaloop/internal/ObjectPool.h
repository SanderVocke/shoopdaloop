#pragma once
#include <boost/lockfree/queue.hpp>
#include <memory>
#include <thread>
#include <atomic>
#include <iostream>

// A class which manages a queue of audio objects which can be
// consumed lock-free. The queue is continuously replenished with newly allocated
// objects asynchronously.
template<typename Object>
class ObjectPool {
    boost::lockfree::queue<Object*> m_queue;
    const unsigned m_target_n_objects;
    unsigned m_objects_size;
    std::atomic<unsigned> m_actual_n_objects;
    std::atomic<bool> m_finish;
    std::thread m_replenish_thread;
    std::atomic_flag m_replenish_flag;
    std::atomic_flag m_none_available_flag;

public:
    ObjectPool(size_t target_n_objects, size_t objects_size) :
        m_target_n_objects(target_n_objects),
        m_objects_size(objects_size),
        m_actual_n_objects(0),
        m_finish(false),
        m_queue(target_n_objects)
    {
        fill();
        m_replenish_flag.clear();
        m_none_available_flag.clear();

        // Start auto-replenishment
        m_replenish_thread = std::thread(
            [this]() { this->replenish_thread_fn(); });
    }

    ~ObjectPool() {
        Object *buf;
        while(m_queue.pop(buf)) {
            delete buf;
        }
        m_finish = true;
        m_replenish_flag.test_and_set();
        m_replenish_flag.notify_all();
        m_replenish_thread.join();
    }

    // Get an object.
    // The object is taken lock-free from the pool. Should the pool be
    // empty, it is allocated in-place.
    Object* get_object() {
        Object* buf = nullptr;
        if (m_queue.pop(buf)) {
            m_actual_n_objects--;
            m_replenish_flag.test_and_set();
            m_replenish_flag.notify_all();
            return buf;
        }
        else {
            m_none_available_flag.test_and_set();
            return allocate();
        }
    }

    size_t object_size() const { return m_objects_size; }

protected:
    void push() {
        m_queue.push(allocate());
        m_actual_n_objects++;
    }

    void fill() {
        auto n_replenish = m_target_n_objects - m_actual_n_objects;
        for(size_t idx=0; idx<n_replenish; idx++) {
            push();
        }
    }

    void replenish_thread_fn() {
        while(true) {
            if (this->m_finish) { break; }
            m_replenish_flag.wait(false);
            m_replenish_flag.clear();
            if (m_none_available_flag.test()) {
                m_none_available_flag.clear();
                std::cerr << "Warning: one or more audio objects were allocated on the processing thread because "
                             "no pre-allocated objects were available from the pool." << std::endl;
            }
            this->fill();
        }
    }

    Object *allocate() {
        return new Object(m_objects_size);
    }
};