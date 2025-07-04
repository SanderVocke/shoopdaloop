#pragma once
#include <boost/lockfree/queue.hpp>
#include <thread>
#include <atomic>
#include <condition_variable>
#include <mutex>

#include "LoggingEnabled.h"

#ifdef _WIN32
    #include <windows.h>
#elif defined(__linux__) || defined(__APPLE__)
    #include <pthread.h>
#endif


// A class which manages a queue of audio objects which can be
// consumed lock-free. The queue is continuously replenished with newly allocated
// objects asynchronously.
template<typename Object>
class ObjectPool : public ModuleLoggingEnabled<"Backend.ObjectPool"> {
    boost::lockfree::queue<Object*> m_queue;
    const unsigned m_target_n_objects;
    unsigned m_objects_size = 0;
    std::atomic<unsigned> m_actual_n_objects = 0;
    std::atomic<bool> m_finish = false;
    std::thread m_replenish_thread;
    std::atomic<bool> m_replenish = false;
    std::atomic<bool> m_none_available = false;
    std::mutex m_mutex;
    std::condition_variable m_cv;
    std::string m_name;

public:
    ObjectPool(std::string name, size_t target_n_objects, size_t objects_size) :
        m_target_n_objects(target_n_objects),
        m_objects_size(objects_size),
        m_actual_n_objects(0),
        m_finish(false),
        m_queue(target_n_objects),
        m_name(name)
    {
        fill();
        m_replenish = false;
        m_none_available = false;

        // Start auto-replenishment
        m_replenish_thread = std::thread(
            [this]() { this->replenish_thread_fn(); });
        
#ifdef _WIN32
        HANDLE handle = m_replenish_thread.native_handle();
        int winPriority = THREAD_PRIORITY_ABOVE_NORMAL;
        SetThreadPriority(handle, winPriority);
#elif defined(__linux__) || defined(__APPLE__)
        pthread_t handle = m_replenish_thread.native_handle();
        struct sched_param sch;
        int policy;
        pthread_getschedparam(handle, &policy, &sch);
        sch.sched_priority = 4; // "above normal" priority
        pthread_setschedparam(handle, SCHED_FIFO, &sch);
#endif
    }

    ~ObjectPool() {
        m_finish = true;
        {
            std::lock_guard lk(m_mutex);
            m_replenish = true;
        }
        m_cv.notify_one();
        m_replenish_thread.join();

        Object *buf;
        while(m_queue.pop(buf)) {
            delete buf;
        }
    }

    // Get an object.
    // The object is taken lock-free from the pool. Should the pool be
    // empty, it is allocated in-place.
    Object* get_object() {
        Object* buf = nullptr;
        if (m_queue.pop(buf)) {
            {
                std::unique_lock lk(m_mutex, std::defer_lock);
                if (lk.try_lock()) {
                    m_actual_n_objects--;
                    m_replenish = true;
                }
            }
            m_cv.notify_one();
            return buf;
        }
        else {
            m_none_available = true;
            return allocate();
        }
    }

    size_t object_size() const { return m_objects_size; }

protected:
    void push() {
        m_queue.push(allocate());
        m_actual_n_objects++;
        m_none_available = false;
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
            std::unique_lock lk(m_mutex);
            m_cv.wait(lk, [&] { return m_replenish.load(); });
            m_replenish = false;
            if (m_none_available) {
                m_none_available = false;
                log<log_level_warning>(m_name + ": One or more objects were allocated on the processing thread because "
                             "no pre-allocated objects were available from the pool.");
            }
            this->fill();
        }
    }

    Object *allocate() {
        return new Object(m_objects_size);
    }
};