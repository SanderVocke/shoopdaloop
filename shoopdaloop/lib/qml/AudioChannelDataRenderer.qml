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
    property int samples_per_pixel: 1
    
    // Will repeatedly fetch channel data when changed.
    FetchChannelData {
        id: fetcher
        channel: root.channel
        active: true
        onData_as_qimageChanged: shader_source.scheduleUpdate()
    }

    // Render to 2D image
    RenderAudioWaveform {
        id: render
        input_data: fetcher.channel_data ? fetcher.channel_data : []
        samples_per_bin: root.samples_per_pixel
        anchors.fill: parent
    }

    // // Render a scroll bar
    // ScrollBar {
    //     id: scroll
    //     anchors {
    //         bottom: parent.bottom
    //         left: parent.left
    //         right: parent.right
    //     }
    //     size: {
    //         if(!fetcher.channel_data) { return 1.0 }
    //         var window_in_samples = width * root.samples_per_pixel
    //         return window_in_samples / fetcher.channel_data.length
    //     }
    //     minimumSize: 0.05
    //     onSizeChanged: console.log(size)
    //     onWidthChanged: console.log("w", width)
    //     orientation: Qt.Horizontal
    //     policy: ScrollBar.AlwaysOn
    //     visible: size < 1.0
    // }
}