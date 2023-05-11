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
        //onData_as_qimageChanged: shader_source.scheduleUpdate()
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
        live: true
    }

    // Render the shader.
    ScrollView {
        anchors.fill: parent
        id: scroll

        ScrollBar.horizontal.policy: ScrollBar.AlwaysOn

        contentWidth: shader.width

        Rectangle {
            width: shader.width
            height: 10
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: "red" }
                GradientStop { position: 0.33; color: "yellow" }
                GradientStop { position: 1.0; color: "green" }
            }
        }

        Item {
            y: 10
            height: scroll.height
            width: fetcher.channel_data ? fetcher.channel_data.length / root.samples_per_pixel : 1

            ShaderEffect {
                id: shader
                anchors.fill: parent
                fragmentShader: '../../../build/shoopdaloop/lib/qml/shaders/loop_channel.frag.qsb'

                // Shader inputs
                property var audio: shader_source
                property int samples_per_pixel: root.samples_per_pixel
                property int samples_offset: 0
                property int pixels_offset: 0
            }
        }
    }
}