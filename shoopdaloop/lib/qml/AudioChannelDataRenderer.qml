import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import FetchChannelData
import QImageRenderer

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

    // Render, invisibly, the audio data mapped as a 2D image.
    QImageRenderer {
        image: fetcher.data_as_qimage ? fetcher.data_as_qimage : null
        id: audio_renderer
        width: image_width
        height: image_height
        visible: false
    }

    // Provide the invisible audio data image as a shader input.
    ShaderEffectSource {
        id: shader_source
        width: audio_renderer.width
        height: audio_renderer.height
        sourceItem: audio_renderer
        format: ShaderEffectSource.RGBA8
        live: false
    }

    // Render the shader.
    ShaderEffect {
        anchors.fill: parent
        fragmentShader: '../../../build/shoopdaloop/lib/qml/shaders/loop_channel.frag.qsb'

        // Shader inputs
        property var audio: shader_source
        property int samples_per_pixel: root.samples_per_pixel
        property int samples_offset: fetcher.channel_data ? scroll.position * fetcher.channel_data.length : 0
        property int pixels_offset: 0
    }

    // Render a scroll bar
    ScrollBar {
        id: scroll
        anchors {
            bottom: parent.bottom
            left: parent.left
            right: parent.right
        }
        size: {
            if(!fetcher.channel_data) { return 1.0 }
            var window_in_samples = width * root.samples_per_pixel
            return window_in_samples / fetcher.channel_data.length
        }
        minimumSize: 0.05
        onSizeChanged: console.log(size)
        onWidthChanged: console.log("w", width)
        orientation: Qt.Horizontal
        policy: ScrollBar.AlwaysOn
        visible: size < 1.0
    }
}