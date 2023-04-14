import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../mode_helpers.js' as ModeHelpers

Item {
    id: root
    property var loop

    // INTERNAL

    property bool data_ready : last_requested_reqid == last_completed_reqid
    property bool data_needs_update : true

    onData_needs_updateChanged: { if(visible && data_needs_update) { request_update_data() } }
    onVisibleChanged: {  console.log("VISIBLE", visible); if(visible && data_needs_update) { request_update_data() } }

    // For layout of this data, see the worker script. Its reply is stored in here directly.
    property var channels_data : null

    property int last_requested_reqid : 0
    property int last_completed_reqid : channels_data ? channels_data.request_id : -1

    readonly property int samples_per_pixel : parseInt(zoom_combo.currentValue)
    onSamples_per_pixelChanged: data_needs_update = true

    enum Tool {
        SetStartOffset,
        SetLength
    }

    Connections {
        target: loop
        function onLengthChanged() {
            if (!ModeHelpers.is_recording_mode(loop.mode)) {
                data_needs_update = true
            }
        }
    }

    function request_update_data() {
        if (!data_worker.ready) {
            data_worker.readyChanged.connect(() => { root.request_update_data() })
            return;
        }

        last_requested_reqid++;
        var input_data = {
            'request_id': last_requested_reqid,
            'samples_per_bin': samples_per_pixel,
            'channels_data': []
        }
        var audio_channels = loop.audio_channels()
        var midi_channels = loop.midi_channels()
        var channel_audio_data, channel_midi_notes, channel_id, channel_start_offset, chan
        for (var chan_idx = 0; chan_idx < audio_channels.length; chan_idx++) {
            chan = audio_channels[chan_idx]
            channel_id = chan.obj_id
            channel_audio_data = chan.get_data()
            channel_start_offset = chan.start_offset

            input_data['channels_data'].push([
                channel_id,
                {
                    "audio": channel_audio_data,
                    "start_offset": channel_start_offset
                }
            ])
        }
        for (var chan_idx = 0; chan_idx < midi_channels.length; chan_idx++) {
            chan = midi_channels[chan_idx]
            channel_id = chan.obj_id
            channel_midi_notes = chan.get_notes()
            channel_start_offset = chan.start_offset

            input_data['channels_data'].push([
                channel_id,
                {
                    "midi_notes": channel_midi_notes,
                    "start_offset": channel_start_offset
                }
            ])
        }

        data_needs_update = false
        data_worker.sendMessage(input_data)   
    }

    WorkerScript {
        id: data_worker
        source: "workers/prepare_loop_channels_data_for_display.mjs"

        onMessage: (output_data) => {
            root.channels_data = output_data
            root.channels_dataChanged()
        }
    }

    // readonly property real zoomed_out_samples_per_waveform_pixel : length_samples / width
    // property int samples_per_waveform_pixel : Math.max(zoomed_out_samples_per_waveform_pixel * (1.0 - zoom_slider.value), 1)
    // property int length_samples: 0
    // property int length_bins: length_samples / samples_per_waveform_pixel
    // property int start_idx: 0
    // property var waveform_data : {}
    // property var midi_data : {}
    // //property alias midi_data: waveform.midi_data
    // property real min_db: 0.0
    // property real max_db: 0.0
    // //property real waveform_data_max: 0.0
    // //property alias dirty: waveform.dirty
    // property bool recording
    // property bool updating: false

    function on_waveform_clicked(waveform, channel, event, sample) {
        console.log("Waveform clicked @ ", event.x, " (sample ", sample, ")")
        var channels = loop.audio_channels()
        switch (tool_combo.currentValue) {
            case LoopContentWidget.Tool.SetStartOffset:
                for(var chan_idx =0; chan_idx < channels.length; chan_idx++) {
                    var chan_sample = sample - channels_data['channels_data'][chan_idx][1]['pre_padding']
                    channels[chan_idx].set_start_offset(chan_sample)
                }
                break;
            case LoopContentWidget.Tool.SetLength:
                for(var chan_idx = 0; chan_idx < channels.length; chan_idx++) {
                    var chan = channels[chan_idx];
                    if (chan == channel) {
                        var chan_sample = sample - channels_data['channels_data'][chan_idx][1]['pre_padding']
                        var chan_len = chan_sample - chan.start_offset
                        if (chan_len >= 0) {
                            root.loop.set_length (chan_len)
                        }
                    }
                }
                break;
        }
    }

    // onSamples_per_waveform_pixelChanged: update_data()

    // function update_data() {
    //     updating = true
    //     var audio_data = {}
    //     var audio_channels = loop.audio_channels()
    //     var midi_channels = loop.midi_channels()
    //     var min_pos = 0
    //     var max_pos = loop.length
    //     for (var idx=0; idx < audio_channels.length; idx++) {
    //         var chan = audio_channels[idx]
    //         var data = chan.get_rms_data(0, loop.length, samples_per_waveform_pixel)
    //         min_pos = Math.min(min_pos, -chan.start_offset)
    //         max_pos = Math.max(max_pos, chan.data_length)
    //     }
    //     for (var idx=0; idx < audio_channels.length; idx++) {
    //         var key = 'audio channel ' + (idx+1).toString()
    //         audio_data[key] = {}
    //         audio_data[key]['data'] = data
    //         audio_data[key]['pixel_offset'] = (chan.start_offset - min_pos) / zoomed_out_samples_per_waveform_pixel
    //     }
    //     console.log("warning: midi content data unimplemented")
    //     midi_data = {}
    //     start_idx = min_pos
    //     length_samples = (max_pos - min_pos)
    //     waveform_data = audio_data
    //     updating = false
    //     waveform_dataChanged()
    // }

    Row {
        id: toolbar
        anchors {
            top: parent.top
            left: parent.left
        }
        height: 40
        spacing: 5

        Button {
            height: 35
            width: 30
            onClicked: root.request_update_data()

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'refresh'
                color: Material.foreground
            }
        }

        Label {
            anchors.verticalCenter: zoom_combo.verticalCenter
            text: "Samples/pixel:"
        }

        // Slider {
        //     id: zoom_slider
        //     width: 150
        //     value: 0.0
        //     anchors {
        //         verticalCenter: parent.verticalCenter
        //     }
        // }

        ComboBox {
            id: zoom_combo
            model: ['1', '2', '4', '8', '16', '32', '64', '128', '256', '512', '1024', '2048', '4096']
            currentIndex: 8
        }

        Label {
            anchors.verticalCenter: zoom_combo.verticalCenter
            text: "Tool:"
        }

        ComboBox {
            id: tool_combo
            anchors.verticalCenter: zoom_combo.verticalCenter
            textRole: "text"
            valueRole: "value"

            model: [
                { value: LoopContentWidget.Tool.SetStartOffset, text: "set start" },
                { value: LoopContentWidget.Tool.SetLength, text: "set length" }
            ]
        }
    }    

    ScrollView {
        id: scroll
        anchors.top: toolbar.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOn
        contentWidth: waveforms.width
    
        Column {
            spacing: 4

            Mapper {
                id: waveforms
                model : root.channels_data ? root.channels_data['channels_data'] : []
                width : childrenRect.width

                Item {
                    id: delegate
                    property int index
                    property var mapped_item

                    function has_audio() { return ('audio' in mapped_item[1])}
                    function has_midi() { return ('midi_notes' in mapped_item[1])}
                    
                    readonly property var channel: [...loop.audio_channels(), ...loop.midi_channels()].find(c => c.obj_id == mapped_item[0])

                    height: 80
                    width: waveform.width

                    Connections {
                        target: delegate.channel
                        function onStart_offsetChanged() { root.data_needs_update = true }
                    }

                    WaveformCanvas {
                        id: waveform
                        anchors {
                            left: parent.left
                        }
                        height: 80
                        
                        waveform_data: delegate.has_audio() ? delegate.mapped_item[1]['audio']['rms'] : []
                        midi_notes: delegate.has_midi() ? delegate.mapped_item[1]['midi_notes'] : []
                        timesteps_per_pixel : root.samples_per_pixel
                        // min_db : widget.min_db
                        // max_db : widget.max_db

                        onClicked: (event) => {
                            var sample = event.x * root.channels_data['samples_per_bin']
                            root.on_waveform_clicked(this, delegate.channel, event, sample)
                        }
                    }

                    Rectangle {
                        id: data_window_rect
                        color: 'blue'
                        width: root.loop.length / root.channels_data['samples_per_bin']
                        height: parent.height
                        opacity: 0.3
                        x: root.channels_data ? root.channels_data['start_offset'] / root.channels_data['samples_per_bin'] : 0
                        y: 0
                    }

                    Rectangle {
                        color: 'green'
                        width: 2
                        height: parent.height
                        x: root.channels_data ? (root.loop.position + root.channels_data['start_offset']) / root.channels_data['samples_per_bin'] : 0
                        y: 0
                    }
                }
            }
        }
    }

    Label {
        anchors.fill: scroll
        background: Rectangle { color: 'black' }
        visible: !root.data_ready || root.data_needs_update
        color: Material.foreground
        text: "Updating..."
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }

    Label {
        anchors.fill: scroll
        background: Rectangle { color: 'black' }
        visible: ModeHelpers.is_recording_mode(root.loop.mode)
        color: Material.foreground
        text: "Recording..."
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
}