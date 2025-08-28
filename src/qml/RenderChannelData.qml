import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import ShoopDaLoop.Rust

Item {
    id: root

    property var send_audio_to: render_audio
    property var send_midi_to: render_midi
    property var send_data_to: is_audio ? send_audio_to : send_midi_to

    property real samples_per_bin : 1.0
    property int samples_offset : 0
    property var channel : null

    property bool have_data : render_audio.have_data || render_midi.have_data

    property bool is_audio : root.channel && root.channel.descriptor.type == "audio"
    property bool is_midi : root.channel && root.channel.descriptor.type == "midi"

    ShoopRenderAudioWaveform {
        id: render_audio

        samples_per_bin: root.samples_per_bin
        samples_offset: root.samples_offset
        width: root.width
        height: root.height
    }

    ShoopRenderMidiSequence {
        id: render_midi
        
        samples_per_bin: root.samples_per_bin
        samples_offset: root.samples_offset
        width: root.width
        height: root.height
    }
}