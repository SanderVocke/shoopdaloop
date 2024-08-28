import QtQuick 6.6
import ShoopDaLoop.PythonLogger

import ShoopConstants

Item {
    property bool monitor
    property var unique_loop_modes : []
    property var unique_next_cycle_loop_modes : []

    readonly property bool any_loop_playing: {
        return unique_loop_modes.includes(ShoopConstants.LoopMode.Playing)
    }
    readonly property bool any_loop_playing_dry: unique_loop_modes.includes(ShoopConstants.LoopMode.PlayingDryThroughWet)
    readonly property bool any_loop_recording: unique_loop_modes.includes(ShoopConstants.LoopMode.Recording) || unique_loop_modes.includes(ShoopConstants.LoopMode.Replacing)
    readonly property bool any_loop_pre_recording: unique_next_cycle_loop_modes.includes(ShoopConstants.LoopMode.Recording) || unique_next_cycle_loop_modes.includes(ShoopConstants.LoopMode.Replacing)
    readonly property bool any_loop_rerecording_dry: unique_loop_modes.includes(ShoopConstants.LoopMode.RecordingDryIntoWet)
    readonly property bool any_loop_pre_rerecording_dry: unique_next_cycle_loop_modes.includes(ShoopConstants.LoopMode.RecordingDryIntoWet)

    // The properties above determine the readonlies below.
    // To understand why, please refer to "States And Connections"
    // in the developer documentation.
    readonly property bool mute_drywet_input_passthrough:  (!monitor && any_loop_playing_dry) || (any_loop_pre_rerecording_dry || any_loop_rerecording_dry)
    readonly property bool mute_drywet_output_passthrough: !(monitor || any_loop_playing_dry || any_loop_pre_rerecording_dry || any_loop_rerecording_dry)
    readonly property bool mute_direct_passthrough:        !monitor       
    readonly property bool enable_fx:                       monitor || any_loop_recording || any_loop_pre_recording || any_loop_playing_dry || any_loop_rerecording_dry || any_loop_pre_rerecording_dry
    readonly property bool force_monitoring_off:            any_loop_rerecording_dry || any_loop_pre_rerecording_dry        

    function trigger_signals() {
        any_loop_playingChanged()
        any_loop_playing_dryChanged()
        any_loop_recordingChanged()
        any_loop_pre_recordingChanged()
        any_loop_pre_rerecording_dryChanged()
        mute_drywet_input_passthroughChanged()
        mute_drywet_output_passthroughChanged()
        mute_direct_passthroughChanged()
        enable_fxChanged()
        force_monitoring_offChanged()
    }

    Component.onCompleted: trigger_signals()

    property PythonLogger logger: PythonLogger { name: "Frontend.Qml.TrackControlLogic" }

    onMute_drywet_input_passthroughChanged: logger.debug(() => ("mute drywet in passthrough: " + mute_drywet_input_passthrough))
    onMute_drywet_output_passthroughChanged: logger.debug(() => ("mute drywet fx out passthrough: " + mute_drywet_output_passthrough))
    onMute_direct_passthroughChanged: logger.debug(() => ("mute monitor: " + mute_direct_passthrough))
    onEnable_fxChanged: logger.debug(() => ("enable fx: " + enable_fx))
    onForce_monitoring_offChanged: logger.debug(() => ("force monitoring off: " + force_monitoring_off))
}