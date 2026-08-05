import QtQuick 6.6

Item {
    id: root

    visible: false

    property var backend: null
    property var stateSink: null
    readonly property var state: ({
        version: global_args.version_string,
        dspLoadPercent: backend ? backend.dsp_load : 0.0,
        xruns: backend ? backend.xruns : 0,
        bufferSize: backend ? backend.buffer_size : 0,
        sampleRate: backend ? backend.sample_rate : 0,
        defaultRecordingAction: AppRegistries.state_registry.default_recording_action === "grab" ? 1 : 0,
        playAfterRecord: AppRegistries.state_registry.play_after_record_active,
        sync: AppRegistries.state_registry.sync_active,
        solo: AppRegistries.state_registry.solo_active,
        applyNCycles: AppRegistries.state_registry.apply_n_cycles
    })

    function updateState() {
        if (stateSink) stateSink(state)
    }

    onStateChanged: updateState()
    Component.onCompleted: updateState()
}
