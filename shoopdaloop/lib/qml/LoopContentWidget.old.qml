import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

import '../mode_helpers.js' as ModeHelpers

Item {
    id: root
    property var loop
    property var master_loop

    // INTERNAL

    property bool data_ready : last_requested_reqid == last_completed_reqid
    property bool data_needs_update : true

    onData_needs_updateChanged: { if(visible && data_needs_update) { request_update_data() } }
    onVisibleChanged: { if(visible && data_needs_update) { request_update_data() } }

    onLoopChanged: { data_needs_update = true }
    Connections {
        target: root.loop
        property var prev_mode : null
        function onModeChanged() {
            if (prev_mode && ModeHelpers.is_recording_mode(prev_mode) &&
                !ModeHelpers.is_recording_mode(root.loop.mode)) {
                    data_needs_update = true
                }
            prev_mode = root.loop.mode
        }
    }

    // For layout of this data, see the worker script. Its reply is stored in here directly.
    property var channels_data : null

    property int last_requested_reqid : 0
    property int last_completed_reqid : channels_data ? channels_data.request_id : -1

    readonly property int samples_per_pixel : parseInt(zoom_combo.currentValue)
    onSamples_per_pixelChanged: {data_needs_update = true}

    enum Tool {
        SetStartOffsetAll,
        SetStartOffsetSingle,
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
            var do_once = function() {
                data_worker.readyChanged.disconnect(do_once);
                root.request_update_data();
            }
            data_worker.readyChanged.connect(do_once)
            return;
        }

        // Queue the actual request for the next event loop cycle,
        // because triggering events usually come in for all channels
        // at the same time. This prevents running the request multiple times.
        request_update_data_delayer.trigger()
    }

    ExecuteNextCycle {
        id: request_update_data_delayer
        onExecute: do_request_update_data()
    }

    function do_request_update_data() {
        last_requested_reqid++;
        var input_data = {
            'request_id': last_requested_reqid,
            'samples_per_bin': samples_per_pixel,
            'channels_data': []
        }
        var audio_channels = loop.audio_channels
        var midi_channels = loop.midi_channels
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
        //console.log("Waveform clicked @ ", event.x, " (sample ", sample, ")")
        var channels = loop.channels
        const clicked_chan_idx = channels.findIndex(c => c == channel)
        const clicked_pre_padding = channels_data['channels_data'][clicked_chan_idx][1]['included_pre_padding']
        switch (tool_combo.currentValue) {
            case LoopContentWidget.Tool.SetStartOffsetAll:
                var chan_sample = sample - clicked_pre_padding
                channels.forEach(c => c.set_start_offset(chan_sample))
                break;
            case LoopContentWidget.Tool.SetStartOffsetSingle:
                var chan_sample = sample - clicked_pre_padding
                channels[clicked_chan_idx].set_start_offset(chan_sample)
                break;
            case LoopContentWidget.Tool.SetLength:
                for(var chan_idx = 0; chan_idx < channels.length; chan_idx++) {
                    var chan = channels[chan_idx];
                    if (chan == channel) {
                        const pre_padding = channels_data['channels_data'][chan_idx][1]['included_pre_padding']
                        var chan_sample = sample - pre_padding
                        var chan_len = chan_sample - chan.start_offset
                        if (chan_len >= 0) {
                            root.loop.set_length (chan_len)
                        }
                    }
                }
                break;
        }
    }

    Row {
        id: toolbar_1
        anchors {
            top: parent.top
            left: parent.left
        }
        height: 40
        spacing: 5

        ExtendedButton {
            tooltip: "Re-fetch and render loop data."
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

        ShoopComboBox {
            id: zoom_combo
            model: ['1', '2', '4', '8', '16', '32', '64', '128', '256', '512', '1024', '2048', '4096']
            currentIndex: 8
        }

        Label {
            anchors.verticalCenter: zoom_combo.verticalCenter
            text: "Tool:"
        }

        ShoopComboBox {
            id: tool_combo
            anchors.verticalCenter: zoom_combo.verticalCenter
            textRole: "text"
            valueRole: "value"
            width: 120

            model: [
                { value: LoopContentWidget.Tool.SetStartOffsetAll, text: "set start (all)" },
                { value: LoopContentWidget.Tool.SetStartOffsetSingle, text: "set start (single)" },
                { value: LoopContentWidget.Tool.SetLength, text: "set length" }
            ]
        }

        Label {
            anchors.verticalCenter: zoom_combo.verticalCenter
            text: "Grid:"
        }

        ShoopComboBox {
            id: minor_grid_divider
            anchors.verticalCenter: zoom_combo.verticalCenter
            textRole: "text"
            valueRole: "value"
            currentIndex : 5
            width: 120

            model: [
                { value: undefined, text: "None" },
                { value: 2, text: "Master / 2" },
                { value: 3, text: "Master / 3" },
                { value: 4, text: "Master / 4" },
                { value: 6, text: "Master / 6" },
                { value: 8, text: "Master / 8" },
                { value: 16, text: "Master / 16" },
            ]
        }
    }

    Row {
        id: toolbar_2
        anchors {
            top: toolbar_1.bottom
            left: parent.left
        }
        height: 40
        spacing: toolbar_1.spacing

        Label {
            text: "length:"
            anchors.verticalCenter: length_field.verticalCenter
        }

        ShoopTextField {
            id: length_field
            validator: IntValidator {}
            text: root.loop.length.toString()
            onEditingFinished: {
                root.loop.set_length(parseInt(text))
                text = Qt.binding(() => root.loop.length.toString())
            }
        }

        ExtendedButton {
            tooltip: "Additional options."
            height: 35
            width: 30
            onClicked: { console.log ("Unimplemented") }

            anchors.verticalCenter: length_field.verticalCenter

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'dots-vertical'
                color: Material.foreground
            }
        }
    }

    ScrollView {
        id: scroll
        anchors.top: toolbar_2.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOn
        contentWidth: waveforms_column.width
    
        Column {
            id: waveforms_column
            spacing: 4
            width : childrenRect.width

            Mapper {
                id: waveforms
                model : root.channels_data ? root.channels_data['channels_data'] : []

                Item {
                    id: delegate
                    property int index
                    property var mapped_item

                    function has_audio() { return ('audio' in mapped_item[1])}
                    function has_midi() { return ('midi_notes' in mapped_item[1])}

                    width: Math.max(scroll.width, waveform.data_length, 1)
                    
                    readonly property var channel: loop.channels.find(c => c.obj_id == mapped_item[0])

                    height: 80

                    MouseArea {
                        x: 0
                        y: 0
                        height: parent.height
                        width: Math.max(parent.width, scroll.width)
                        onClicked: (event) => {
                            var sample = event.x * root.channels_data['samples_per_bin']
                            root.on_waveform_clicked(this, delegate.channel, event, sample)
                        }
                    }

                    Connections {
                        target: delegate.channel
                        function onStart_offsetChanged() { root.data_needs_update = true }
                    }

                    Rectangle {
                        id: background_rect
                        color: 'black'
                        anchors.fill: parent
                    }

                    Rectangle {
                        id: data_window_rect
                        color: 'blue'
                        width: root.loop.length / root.channels_data['samples_per_bin']
                        height: parent.height
                        opacity: 0.3
                        x: root.channels_data ? -root.channels_data['render_start_pos'] / root.channels_data['samples_per_bin'] : 0
                        y: 0
                    }

                    Repeater {
                        id: minor_grid_lines
                        anchors.fill: parent

                        property real start : data_window_rect.x
                        property real interval : minor_grid_divider.currentValue ? major_grid_lines.interval / minor_grid_divider.currentValue : undefined
                        property var positions : {
                            if (interval && interval > 0 && parent.width / interval < 1000) {
                                var r = []
                                function pos(offset) {
                                    return Math.round(start + offset*interval)
                                }
                                var off =  - Math.floor(start / interval)
                                for(; pos(off) < parent.width; off++) {
                                    r.push(pos(off))
                                }
                                return r
                            } else {
                                return []
                            }
                        }

                        model: positions.length

                        Rectangle {
                            color: 'grey'
                            width: 1
                            height: parent.height
                            x: minor_grid_lines.positions[index] - 1
                            y: 0
                        }
                    }

                    Repeater {
                        id: major_grid_lines
                        anchors.fill: parent

                        property real start : data_window_rect.x
                        property real interval : root.master_loop.length / root.channels_data['samples_per_bin']
                        property var positions : {
                            if (interval && interval > 0 && parent.width / interval < 1000) {
                                var r = []
                                function pos(offset) {
                                    return Math.round(start + offset*interval)
                                }
                                var off =  - Math.floor(start / interval)
                                for(; pos(off) < parent.width; off++) {
                                    r.push(pos(off))
                                }
                                return r
                            } else {
                                return []
                            }
                        }

                        model: positions.length

                        Rectangle {
                            color: 'white'
                            width: 1
                            height: parent.height
                            x: major_grid_lines.positions[index] - 1
                            y: 0
                        }
                    }

                    WaveformCanvas {
                        id: waveform
                        anchors {
                            left: parent.left
                        }
                        height: 80
                        width: parent.width
                        
                        waveform_data: delegate.has_audio() ? delegate.mapped_item[1]['audio']['rms'] : []
                        midi_notes: delegate.has_midi() ? delegate.mapped_item[1]['midi_notes'] : []
                        // min_db : widget.min_db
                        // max_db : widget.max_db
                    }

                    Rectangle {
                        color: 'green'
                        width: 2
                        height: parent.height
                        x: root.channels_data ? (root.loop.position - root.channels_data['render_start_pos']) / root.channels_data['samples_per_bin'] : 0
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