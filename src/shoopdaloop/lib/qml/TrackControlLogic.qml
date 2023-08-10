import QtQuick 6.3
import ShoopDaLoop.PythonLogger

import '../generated/types.js' as Types

Item {
    property bool monitor
    property var unique_loop_modes : []
    property var unique_next_cycle_loop_modes : []

    readonly property bool any_loop_playing: unique_loop_modes.includes(Types.LoopMode.Playing)
    readonly property bool any_loop_playing_dry: unique_loop_modes.includes(Types.LoopMode.PlayingDryThroughWet)
    readonly property bool any_loop_recording: unique_loop_modes.includes(Types.LoopMode.Recording) || unique_loop_modes.includes(Types.LoopMode.Replacing)
    readonly property bool any_loop_pre_recording: unique_next_cycle_loop_modes.includes(Types.LoopMode.Recording) || unique_next_cycle_loop_modes.includes(Types.LoopMode.Replacing)
    readonly property bool any_loop_rerecording_dry: unique_loop_modes.includes(Types.LoopMode.RecordingDryIntoWet)
    readonly property bool any_loop_pre_rerecording_dry: unique_next_cycle_loop_modes.includes(Types.LoopMode.RecordingDryIntoWet)

    // The properties above determine the readonlies below.
    // To understand why, please refer to "States And Connections"
    // in the developer documentation.
    readonly property bool mute_drywet_input_passthrough:  !any_loop_pre_recording &&  any_loop_playing_dry && !any_loop_recording && !monitor
    readonly property bool mute_drywet_output_passthrough: !any_loop_playing_dry   && !monitor
    readonly property bool mute_direct_passthrough:        !monitor       
    readonly property bool disable_fx:                     !any_loop_pre_recording && !any_loop_playing_dry && !monitor
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
        disable_fxChanged()
        force_monitoring_offChanged()
    }

    Component.onCompleted: trigger_signals()

    property PythonLogger logger : PythonLogger { name: default_logger.name + ".TrackControlLogic" }

    onMute_drywet_input_passthroughChanged: logger.debug("mute fx in monitor: " + mute_drywet_input_passthrough)
    onMute_drywet_output_passthroughChanged: logger.debug("mute fx out monitor: " + mute_drywet_output_passthrough)
    onMute_direct_passthroughChanged: logger.debug("mute monitor: " + mute_direct_passthrough)
    onDisable_fxChanged: logger.debug("disable fx: " + disable_fx)
    onForce_monitoring_offChanged: logger.debug("force monitoring off: " + force_monitoring_off)
}