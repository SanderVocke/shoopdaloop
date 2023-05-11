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

    // Reduce the audio input to the intended samples per pixel and calculate
    // power in dB
    ShaderEffect {
        y: 200
        id: reduce_shader
        width: shader_source.width
        property int n: shader_source.width * shader_source.height / root.samples_per_pixel
        height: Math.ceil(n / width)
        visible: true
        fragmentShader: '../../../build/shoopdaloop/lib/qml/shaders/audio_power_reduce.frag.qsb'

        onHeightChanged: console.log("reducer: ", width, height)

        // Shader inputs
        property var waveform: shader_source
        property int samples_per_pixel: root.samples_per_pixel
    }

    // Provide the reduced audio waveform as an input to rendering shader
    ShaderEffectSource {
        id: reduced_source
        width: reduce_shader.width
        height: reduce_shader.height
        sourceItem: reduce_shader
        format: ShaderEffectSource.RGBA8
        live: true
    }

    // Render the reduced data.
    ShaderEffect {
        visible: true

        anchors.fill: parent
        fragmentShader: '../../../build/shoopdaloop/lib/qml/shaders/render_waveform.frag.qsb'

        // Shader inputs
        property var waveform: reduced_source
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