import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Item {
    id: root
    property var loop

    // INTERNAL

    property bool data_ready : last_requested_reqid == last_completed_reqid

    // For layout of this data, see the worker script. Its reply is stored in here directly.
    property var channels_data : null

    property int last_requested_reqid : 0
    property int last_completed_reqid : -1

    readonly property int samples_per_pixel : parseInt(zoom_combo.currentValue)
    onSamples_per_pixelChanged: request_update_data()

    onChannels_dataChanged: console.log("Got reply!")
    onLast_requested_reqidChanged: console.log("Sent request!")

    function request_update_data() {
        last_requested_reqid++;
        var input_data = {
            'request_id': last_requested_reqid,
            'samples_per_bin': samples_per_pixel,
            'channels_data': []
        }
        var audio_channels = loop.audio_channels()
        var channel_audio_data, channel_name, channel_start_offset, chan
        for (var chan_idx = 0; chan_idx < audio_channels.length; chan_idx++) {
            chan = audio_channels[chan_idx]
            channel_name = 'audio channel ' + (chan_idx + 1).toString()
            channel_audio_data = chan.get_data()
            channel_start_offset = chan.start_offset

            input_data['channels_data'].push([
                channel_name,
                {
                    "audio": channel_audio_data,
                    "start_offset": channel_start_offset
                }
            ])
        }

        data_worker.sendMessage(input_data)        
    }

    WorkerScript {
        id: data_worker
        source: "workers/prepare_loop_channels_data_for_display.mjs"

        onMessage: (output_data) => {
            root.channels_data = output_data
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

    // function on_waveform_clicked(waveform, event, sample) {
    //     console.log("Waveform clicked @ ", event.x, " (sample ", sample, ")")
    // }

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
        }

        Label {
            anchors.verticalCenter: zoom_combo.verticalCenter
            text: "Tool:"
        }

        ComboBox {
            id: tool_combo
            anchors.verticalCenter: zoom_combo.verticalCenter
            model: ['Set start pos']
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

            Repeater {
                id: waveforms
                model : root.channels_data ? root.channels_data['channels_data'].length : 0
                width : root.channels_data && root.channels_data['channels_data'].length > 0 ?
                    root.channels_data['channels_data'][0][1]['audio']['rms'].length : 0

                Item {
                    anchors {
                        left: parent.left
                    }
                    height: 80
                    width: waveforms.width

                    WaveformCanvas {
                        id: waveform
                        anchors {
                            left: parent.left
                        }
                        width: waveforms.width
                        height: 80
                        
                        waveform_data: root.channels_data['channels_data'][index][1]['audio']['rms']
                        // min_db : widget.min_db
                        // max_db : widget.max_db

                        // onClicked: (event) => {
                        //     var sample = (event.x - pixel_offset) * widget.samples_per_waveform_pixel
                        //     widget.on_waveform_clicked(this, event, sample)
                        // }
                    }

                    // Rectangle {
                    //     color: 'blue'
                    //     width: 2
                    //     height: parent.height
                    //     x: widget.loop ? parent.normalize(parent._start_offset) : 0
                    //     y: 0
                    // }

                    // Rectangle {
                    //     color: 'green'
                    //     width: 2
                    //     height: parent.height
                    //     x: widget.loop ? parent.normalize(widget.loop.position + parent._start_offset) : 0
                    //     y: 0
                    // }

                    // Rectangle {
                    //     color: 'blue'
                    //     anchors.fill: parent
                    //     visible: updating
                    // }

                    // Label {
                    //     anchors.fill: parent
                    //     background: Rectangle { color: 'black' }
                    //     visible: recording
                    //     color: Material.foreground
                    //     text: "Recording..."
                    //     horizontalAlignment: Text.AlignHCenter
                    //     verticalAlignment: Text.AlignVCenter
                    // }
                }
            }
        }
    }
}