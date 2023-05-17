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
            samples_offset: scroll.position * input_data.length
            width: root.width
            height: root.height
        }
    }
}