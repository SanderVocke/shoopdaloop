import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import FetchChannelData
import RenderAudioWaveform

import '../mode_helpers.js' as ModeHelpers

Item {
    id: root
    property var channel
    property alias fetch_active: fetcher.active
    property real samples_per_pixel: 1.0
    property int samples_offset: 0.0
    property int loop_length: 0
    readonly property int n_samples_shown: width * samples_per_pixel
    readonly property int n_samples: fetcher.channel_data ? fetcher.channel_data.length : 0
    
    // Will repeatedly fetch channel data when changed.
    FetchChannelData {
        id: fetcher
        channel: root.channel
        active: true
    }

    // Render audio
    Loader {
        active: root.channel.descriptor.type == 'audio'

        RenderAudioWaveform {
            id: render
            input_data: fetcher.channel_data ? fetcher.channel_data : []
            samples_per_bin: root.samples_per_pixel
            samples_offset: root.samples_offset
            width: root.width
            height: root.height
            clip: true

            Rectangle {
                id: data_window_rect
                color: 'blue'
                width: root.loop_length / render.samples_per_bin
                height: parent.height
                opacity: 0.3
                x: (root.channel.start_offset - render.samples_offset) / render.samples_per_bin
                y: 0
            }

            Rectangle {
                id: preplay_window_rect
                color: 'yellow'
                width: root.channel.n_preplay_samples / render.samples_per_bin
                height: parent.height
                opacity: 0.3
                x: (root.channel.start_offset - render.samples_offset - root.channel.n_preplay_samples) / render.samples_per_bin
                y: 0
            }

            Rectangle {
                id: playback_cursor_rect
                visible: root.channel.played_back_sample != null
                color: 'green'
                width: 2
                height: parent.height
                x: (root.channel.played_back_sample - render.samples_offset) / render.samples_per_bin
                y: 0
            }
        }
    }
}