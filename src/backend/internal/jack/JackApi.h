#pragma once
#include <jack/types.h>
#include <jack_wrappers.h>
#include <stdarg.h>
#include <stdexcept>

// Runtime interface for the JACK API. The production implementation delegates to
// libjack through jack_wrappers; tests can provide a mock implementation.
class IJackApi {
public:
    virtual ~IJackApi() = default;

    virtual bool supports_processing() const = 0;

    virtual const char** get_ports(jack_client_t*, const char*, const char*, unsigned long) = 0;
    virtual jack_port_t* port_by_name(jack_client_t*, const char*) = 0;
    virtual int port_flags(const jack_port_t*) = 0;
    virtual const char* port_type(const jack_port_t*) = 0;
    virtual const char** port_get_all_connections(const jack_client_t*, const jack_port_t*) = 0;
    virtual const char** port_get_connections(const jack_port_t*) = 0;
    virtual void* port_get_buffer(jack_port_t*, jack_nframes_t) = 0;
    virtual void port_get_latency_range(jack_port_t*, jack_latency_callback_mode_t, jack_latency_range_t*) = 0;
    virtual jack_nframes_t port_get_latency(jack_port_t*) = 0;
    virtual const char* get_client_name(jack_client_t*) = 0;
    virtual jack_nframes_t get_sample_rate(jack_client_t*) = 0;
    virtual jack_nframes_t get_buffer_size(jack_client_t*) = 0;
    virtual int activate(jack_client_t*) = 0;
    virtual int deactivate(jack_client_t*) = 0;
    virtual int client_close(jack_client_t*) = 0;
    virtual jack_time_t get_time() = 0;
    virtual int set_process_callback(jack_client_t*, JackProcessCallback, void*) = 0;
    virtual int set_xrun_callback(jack_client_t*, JackXRunCallback, void*) = 0;
    virtual int set_port_connect_callback(jack_client_t*, JackPortConnectCallback, void*) = 0;
    virtual int set_port_registration_callback(jack_client_t*, JackPortRegistrationCallback, void*) = 0;
    virtual int set_port_rename_callback(jack_client_t*, JackPortRenameCallback, void*) = 0;
    virtual void set_error_function(void (*)(const char*)) = 0;
    virtual void set_info_function(void (*)(const char*)) = 0;
    virtual jack_port_t* port_register(jack_client_t*, const char*, const char*, unsigned long, unsigned long) = 0;
    virtual int port_unregister(jack_client_t*, jack_port_t*) = 0;
    virtual const char* port_name(const jack_port_t*) = 0;
    virtual int connect(jack_client_t*, const char*, const char*) = 0;
    virtual int disconnect(jack_client_t*, const char*, const char*) = 0;
    virtual uint32_t midi_get_event_count(void*) = 0;
    virtual int midi_event_get(jack_midi_event_t*, void*, uint32_t) = 0;
    virtual void midi_clear_buffer(void*) = 0;
    virtual int midi_event_write(void*, jack_nframes_t, const jack_midi_data_t*, size_t) = 0;
    virtual float cpu_load(jack_client_t*) = 0;
    virtual bool port_is_mine(jack_client_t*, jack_port_t*) = 0;
    virtual void free(void*) = 0;
    virtual jack_client_t* client_open(const char*, jack_options_t, jack_status_t*) = 0;
    virtual void init() = 0;
};

class JackApi : public IJackApi {
public:
    bool supports_processing() const override { return true; }

    const char** get_ports(jack_client_t* c, const char* n, const char* t, unsigned long f) override { return jack_get_ports(c, n, t, f); }
    jack_port_t* port_by_name(jack_client_t* c, const char* n) override { return jack_port_by_name(c, n); }
    int port_flags(const jack_port_t* p) override { return jack_port_flags(p); }
    const char* port_type(const jack_port_t* p) override { return jack_port_type(p); }
    const char** port_get_all_connections(const jack_client_t* c, const jack_port_t* p) override { return jack_port_get_all_connections(c, p); }
    const char** port_get_connections(const jack_port_t* p) override { return jack_port_get_connections(p); }
    void* port_get_buffer(jack_port_t* p, jack_nframes_t n) override { return jack_port_get_buffer(p, n); }
    void port_get_latency_range(jack_port_t* p, jack_latency_callback_mode_t m, jack_latency_range_t* r) override { jack_port_get_latency_range(p, m, r); }
    jack_nframes_t port_get_latency(jack_port_t* p) override { return jack_port_get_latency(p); }
    const char* get_client_name(jack_client_t* c) override { return jack_get_client_name(c); }
    jack_nframes_t get_sample_rate(jack_client_t* c) override { return jack_get_sample_rate(c); }
    jack_nframes_t get_buffer_size(jack_client_t* c) override { return jack_get_buffer_size(c); }
    int activate(jack_client_t* c) override { return jack_activate(c); }
    int deactivate(jack_client_t* c) override { return jack_deactivate(c); }
    int client_close(jack_client_t* c) override { return jack_client_close(c); }
    jack_time_t get_time() override { return jack_get_time(); }
    int set_process_callback(jack_client_t* c, JackProcessCallback cb, void* arg) override { return jack_set_process_callback(c, cb, arg); }
    int set_xrun_callback(jack_client_t* c, JackXRunCallback cb, void* arg) override { return jack_set_xrun_callback(c, cb, arg); }
    int set_port_connect_callback(jack_client_t* c, JackPortConnectCallback cb, void* arg) override { return jack_set_port_connect_callback(c, cb, arg); }
    int set_port_registration_callback(jack_client_t* c, JackPortRegistrationCallback cb, void* arg) override { return jack_set_port_registration_callback(c, cb, arg); }
    int set_port_rename_callback(jack_client_t* c, JackPortRenameCallback cb, void* arg) override { return jack_set_port_rename_callback(c, cb, arg); }
    void set_error_function(void (*fn)(const char*)) override { jack_set_error_function((void*)fn); }
    void set_info_function(void (*fn)(const char*)) override { jack_set_info_function((void*)fn); }
    jack_port_t* port_register(jack_client_t* c, const char* n, const char* t, unsigned long f, unsigned long b) override { return jack_port_register(c, n, t, f, b); }
    int port_unregister(jack_client_t* c, jack_port_t* p) override { return jack_port_unregister(c, p); }
    const char* port_name(const jack_port_t* p) override { return jack_port_name(p); }
    int connect(jack_client_t* c, const char* s, const char* d) override { return jack_connect(c, s, d); }
    int disconnect(jack_client_t* c, const char* s, const char* d) override { return jack_disconnect(c, s, d); }
    uint32_t midi_get_event_count(void* b) override { return jack_midi_get_event_count(b); }
    int midi_event_get(jack_midi_event_t* e, void* b, uint32_t i) override { return jack_midi_event_get(e, b, i); }
    void midi_clear_buffer(void* b) override { jack_midi_clear_buffer(b); }
    int midi_event_write(void* b, jack_nframes_t t, const jack_midi_data_t* d, size_t s) override { return jack_midi_event_write(b, t, d, s); }
    float cpu_load(jack_client_t* c) override { return jack_cpu_load(c); }
    bool port_is_mine(jack_client_t* c, jack_port_t* p) override { return jack_port_is_mine(c, p); }
    void free(void* p) override { jack_free(p); }

    jack_client_t* client_open(const char* name, jack_options_t options, jack_status_t* status) override {
        return jack_client_open(name, options, status);
    }

    void init() override {
        static bool initialized = false;
        if (!initialized) {
            if (initialize_jack_wrappers(0)) {
                throw std::runtime_error("Unable to find Jack client library.");
            }
            initialized = true;
        }
    }
};
