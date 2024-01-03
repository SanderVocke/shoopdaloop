import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonFetchChannelData
import ShoopDaLoop.PythonLogger

import '../mode_helpers.js' as ModeHelpers

Item {
    id: root
    property var loop
    property var sync_loop

    property PythonLogger logger : default_logger

    enum Tool {
        None,
        SetStartOffsetAll,
        SetStartOffsetSingle,
        SetEnd,
        SetPreplaySamplesAll,
        SetPreplaySamplesSingle
    }

    function tool_clicked() {
        for (var idx=0; idx < channel_mapper.unsorted_instances.length; idx++) {
            var c = channel_mapper.unsorted_instances[idx]
            var channel = c.mapped_item
            var s = c.maybe_cursor_sample_idx
            if (s) {
                switch (tool_combo.currentValue) {
                    case LoopContentWidget.Tool.None:
                        break;
                    case LoopContentWidget.Tool.SetStartOffsetAll:
                    case LoopContentWidget.Tool.SetStartOffsetSingle:
                        channel.set_start_offset(s);
                        break;
                    case LoopContentWidget.Tool.SetPreplaySamplesAll:
                    case LoopContentWidget.Tool.SetPreplaySamplesSingle:
                        var n = Math.max(0, channel.start_offset - s)
                        channel.set_n_preplay_samples(n)
                        break;
                    case LoopContentWidget.Tool.SetEnd:
                        var len = s - channel.start_offset
                        if (len >= 0) { channel.loop.set_length(len); }
                        else { root.logger.error(() => ("Ignoring invalid end point: is before start offset.")) }
                        break;
                    default:
                        root.logger.error(() => ("Unimplemented tool."))
                        throw new Error("Unimplemented")
                }
            }
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

        Label {
            anchors.verticalCenter: zoom_slider.verticalCenter
            text: "Zoom:"
        }

        Slider {
            id: zoom_slider
            width: 150
            value: 8.0
            from: 16.0
            to: 0.0

            property real prev_samples_per_pixel: 0.0
            property real samples_per_pixel: Math.pow(2.0, value)
            Component.onCompleted: prev_samples_per_pixel = samples_per_pixel

            onSamples_per_pixelChanged: {
                let prev_n_samples = channels_column.width * prev_samples_per_pixel
                let new_n_samples = channels_column.width * samples_per_pixel
                let diff = new_n_samples - prev_n_samples
                let new_offset = channels_column.samples_offset - diff / 2
                prev_samples_per_pixel = samples_per_pixel

                zoomed(samples_per_pixel, new_offset)
            }

            signal zoomed (real new_samples_per_pixel, real new_offset)

            anchors {
                verticalCenter: parent.verticalCenter
            }
        }

        Label {
            anchors.verticalCenter: zoom_slider.verticalCenter
            text: "Tool:"
        }

        ShoopComboBox {
            id: tool_combo
            anchors.verticalCenter: zoom_slider.verticalCenter
            textRole: "text"
            valueRole: "value"
            width: 160

            model: [
                { value: LoopContentWidget.Tool.None, text: "none", all: false },
                { value: LoopContentWidget.Tool.SetStartOffsetAll, text: "set start (all)", all: true },
                { value: LoopContentWidget.Tool.SetStartOffsetSingle, text: "set start (single)", all: false },
                { value: LoopContentWidget.Tool.SetEnd, text: "set end (all)", all: true },
                { value: LoopContentWidget.Tool.SetPreplaySamplesAll, text: "set pre-play (all)", all: true },
                { value: LoopContentWidget.Tool.SetPreplaySamplesSingle, text: "set pre-play (single)", all: false },
            ]

            property bool is_for_all_channels: model[currentIndex].all
        }

        Label {
            anchors.verticalCenter: zoom_slider.verticalCenter
            text: "Grid:"
        }

        ShoopComboBox {
            id: minor_grid_divider
            anchors.verticalCenter: zoom_slider.verticalCenter
            textRole: "text"
            valueRole: "value"
            currentIndex : 6
            width: 120

            model: [
                { value: undefined, text: "None" },
                { value: 1, text: "Sync" },
                { value: 2, text: "Sync / 2" },
                { value: 3, text: "Sync / 3" },
                { value: 4, text: "Sync / 4" },
                { value: 6, text: "Sync / 6" },
                { value: 8, text: "Sync / 8" },
                { value: 16, text: "Sync / 16" },
            ]
        }

        Switch {
            id: snap_switch
            text: "Snap to grid:"
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
            onClicked: { root.logger.error(() => ("Unimplemented")) }

            anchors.verticalCenter: length_field.verticalCenter

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'dots-vertical'
                color: Material.foreground
            }
        }
    }

    Item {
        anchors {
            top: toolbar_2.bottom
            bottom: parent.bottom
            left: parent.left
            right: parent.right
        }

        Item {
            id: channels_combine_range
            visible: false

            property int padding: 0.2 * 48000
            
            // Overall data starting point, where 0 is each channel's start offset.
            // That is to say: if three channels exist with start offsets 10, 20 and 30,
            // the one with offset 30 dominates so that the overall data starting point
            // becomes -30.
            property int data_start : -Math.max(...loop.channels.map(c => c ? c.start_offset : 0)) - padding
            
            // Likewise for the end point.
            property int data_end : Math.max(...loop.channels.map(c => c ? c.data_length - c.start_offset : 0)) + padding

            property int data_length : data_end - data_start
        }

        Column {
            id: channels_column
            property real samples_offset : 0.0
            Mapper {
                id: channel_mapper
                model: loop.channels

                property var maybe_cursor_display_x: null

                ChannelDataRenderer {
                    visible: root.visible
                    fetch_active: root.visible

                    property var mapped_item
                    property int index

                    height: 100
                    width: root.width

                    channel: mapped_item

                    samples_per_pixel: 1.0
                    Component.onCompleted: samples_per_pixel = zoom_slider.samples_per_pixel

                    Connections {
                        target: zoom_slider
                        function onZoomed(spp, off) {
                            samples_per_pixel = spp;
                            channels_column.samples_offset = off
                        }
                    }

                    property int first_pixel_sample: (channel ? channel.start_offset : 0) + channels_combine_range.data_start

                    samples_offset: channels_column.samples_offset
                    onPan: (amount) => { channels_column.samples_offset += amount }

                    loop_length: root.loop ? root.loop.length : 0

                    property var maybe_cursor_display_x: {
                        var r = hover_ma.containsMouse ? hover_ma.mouseX : channel_mapper.maybe_cursor_display_x

                        if (r != undefined && r != null && snap_switch.checked) {
                            // Snap to grid
                            r = snap_visual_to_grid(r)                     
                        }

                        return r
                    }

                    property var maybe_cursor_sample_idx: maybe_cursor_display_x ?
                        samples_offset + maybe_cursor_display_x * samples_per_pixel :
                        null
                    
                    property bool enable_grid_lines: minor_grid_divider.currentValue != undefined && root.sync_loop && root.sync_loop.length >= 4800
                    major_grid_lines_interval: enable_grid_lines ? root.sync_loop.length : -1
                    minor_grid_lines_interval: enable_grid_lines && minor_grid_divider.currentValue > 1 ?
                                                major_grid_lines_interval / minor_grid_divider.currentValue : -1

                    Rectangle {
                        width: 1
                        height: parent.height
                        color: 'white'
                        x : parent.maybe_cursor_display_x
                        visible: parent.maybe_cursor_display_x != null

                        Label {
                            anchors {
                                left: parent.left
                                top: parent.top
                                margins: 3
                            }
                            text: tool_combo.currentText
                        }
                    }

                    MouseArea {
                        id: hover_ma
                        enabled: tool_combo.currentValue != LoopContentWidget.Tool.None
                        anchors.fill: parent
                        hoverEnabled: enabled
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor

                        onMouseXChanged: if(containsMouse &&
                                            tool_combo.is_for_all_channels) {
                            channel_mapper.maybe_cursor_display_x = mouseX
                        }
                        onContainsMouseChanged: if (!containsMouse) { channel_mapper.maybe_cursor_display_x = null; }

                        onClicked: {
                            root.tool_clicked()
                        }
                    }
                }
            }
        }
    }    
}