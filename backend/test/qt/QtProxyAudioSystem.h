#pragma once
#include "AudioSystemInterface.h"
#include "MidiPortInterface.h"
#include "AudioPortInterface.h"
#include "PortInterface.h"
#include <stdexcept>
#include <memory>

#include <QObject>
#include <QProperty>
#include <QString>
#include <memory>

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
};

template<typename SampleT>
class QtProxyAudioSystem : public AudioSystemInterface<SampleT, size_t>, public _QtProxyAudioSystem {
    Q_OBJECT

    std::function<void(size_t)> m_process_cb;

public:
    QtProxyAudioSystem(
        std::string client_name,
        std::function<void(size_t)> process_cb
    ) : AudioSystemInterface<SampleT, size_t>(client_name, process_cb),
        _QtProxyAudioSystem(),
        m_process_cb(process_cb)
    {
        m_client_name = QString::fromStdString(client_name);
        m_started = false;
    }

    void start() override {
    }

    ~QtProxyAudioSystem() override {}

    std::shared_ptr<AudioPortInterface<SampleT>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override {
    }

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override {
    }

    size_t get_sample_rate() const override {
    }

    void* get_client() const {
        return nullptr;
    }

    const char* get_client_name() const {
        return "Qt test client";
    }
};