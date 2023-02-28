#pragma once
#include "AudioSystemInterface.h"
#include "MidiPortInterface.h"
#include "AudioPortInterface.h"
#include "PortInterface.h"
#include <chrono>
#include <ratio>
#include <stdexcept>
#include <memory>
#include <iostream>
#include <QObject>
#include <QProperty>
#include <QString>
#include <memory>
#include <set>
#include <thread>

class _QtProxyAudioSystem;

extern _QtProxyAudioSystem* g_test_audio_system;
class _QtProxyAudioSystem : public QObject {
    Q_OBJECT

protected:
    QProperty<QString> m_client_name;
    QProperty<bool> m_started;

public:
    _QtProxyAudioSystem() {
        if (g_test_audio_system) {
            throw std::runtime_error("Only one audio system may be registered.");
        }
        g_test_audio_system = this;
    }

    ~_QtProxyAudioSystem() {
        g_test_audio_system = nullptr;
    }

signals:
    void audioPortOpened(QString name, bool is_input);
    void midiPortOpened(QString name, bool is_input);
    void audioPortClosed(QString name);
    void midiPortClosed(QString name);
};

template<typename SampleT>
class QtProxyAudioSystem : public AudioSystemInterface<SampleT, size_t>, public _QtProxyAudioSystem {
    Q_OBJECT

    std::function<void(size_t)> m_process_cb;
    const size_t mc_sample_rate;
    const size_t mc_buffer_size;
    std::atomic<bool> m_finish;
    std::thread m_proc_thread;

public:
    QtProxyAudioSystem(
        std::string client_name,
        std::function<void(size_t)> process_cb
    ) : AudioSystemInterface<SampleT, size_t>(client_name, process_cb),
        _QtProxyAudioSystem(),
        m_process_cb(process_cb),
        mc_buffer_size(1024),
        mc_sample_rate(48000),
        m_finish(false)
    {
        m_client_name = QString::fromStdString(client_name);
        m_started = false;
        m_proc_thread = std::thread([this]{
            std::cout << "QtProxyAudioSystem: starting process thread" << std::endl;
            auto bufs_per_second = mc_sample_rate / mc_buffer_size;
            auto interval = 1.0f / ((float)bufs_per_second);
            auto micros = size_t(interval * 1000000.0f);
            while(!m_finish) {
                std::this_thread::sleep_for(std::chrono::microseconds(micros));
                m_process_cb(mc_buffer_size);
            }
            std::cout << "QtProxyAudioSystem: ending process thread" << std::endl;
        });
    }

    void start() override {
    }

    ~QtProxyAudioSystem() override {
        m_finish = false;
        m_proc_thread.join();
    }

    std::shared_ptr<AudioPortInterface<SampleT>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override {

        audioPortOpened(QString::fromStdString(name), direction == PortDirection::Input);
    }

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override {

        midiPortOpened(QString::fromStdString(name), direction == PortDirection::Input);
    }

    size_t get_sample_rate() const override {
        return mc_sample_rate;
    }

    void* get_client() const {
        return nullptr;
    }

    const char* get_client_name() const {
        return "Qt test client";
    }
};